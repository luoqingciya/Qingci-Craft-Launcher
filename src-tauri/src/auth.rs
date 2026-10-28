// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 微软账号登录的接入层。
//!
//! 协议细节全在 `qcl_auth`，这里只做三件事：把设备码流程跑成后台任务、把结果放进
//! [`AppState`]、把状态推给界面。
//!
//! **令牌不外流**：[`Account`] 序列化给前端时用 `#[serde(skip)]` 去掉令牌与 xuid。
//! 界面只需要显示名字，拿到令牌只会多一个泄露面。

use std::sync::Arc;
use std::sync::Mutex;

use qcl_auth::{
    AuthError, is_trusted_verification_uri, login_chain, poll_until_granted, refresh_access_token,
    request_device_code,
};
use qcl_core::launch::AccountInfo;
use qcl_net::HttpDownloader;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::token_store::{KeyringStore, NullStore, TokenStore};

/// 登录状态推送的事件名。
pub const LOGIN_EVENT: &str = "login://state";

/// 登录后的账号。
///
/// 前端拿到的只有 `name` 与 `uuid`——令牌与 xuid 不序列化。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// 玩家名。
    pub name: String,
    /// 账号 UUID（无连字符）。
    pub uuid: String,
    /// Minecraft 访问令牌。
    #[serde(skip)]
    access_token: String,
    /// Xbox 用户 ID。
    #[serde(skip)]
    xuid: Option<String>,
}

impl Account {
    fn from_info(info: AccountInfo) -> Self {
        Self {
            name: info.player_name,
            uuid: info.uuid,
            access_token: info.access_token,
            xuid: info.xuid,
        }
    }

    /// 转成启动命令需要的账户信息。
    pub fn to_account_info(&self) -> AccountInfo {
        AccountInfo {
            player_name: self.name.clone(),
            uuid: self.uuid.clone(),
            access_token: self.access_token.clone(),
            // 正版账户的 `user_type` 是 `msa`，离线是 `legacy`。
            user_type: "msa".to_owned(),
            client_id: None,
            xuid: self.xuid.clone(),
        }
    }
}

/// 推给界面的登录状态。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoginState {
    /// 已拿到设备码，等用户去浏览器输入。
    #[serde(rename_all = "camelCase")]
    Code {
        /// 要用户输入的短码。
        user_code: String,
        /// 要用户访问的地址。
        verification_uri: String,
        /// 短码有效期（秒）。
        expires_in: u64,
    },
    /// 登录成功。
    Done {
        /// 登录到的账号。
        account: Account,
    },
    /// 失败或被取消。
    Failed {
        /// 给用户看的原因。
        message: String,
    },
}

/// 界面看到的账号状态。
///
/// `restoring` 是必要的第三态：启动时会用存下的刷新令牌去换新令牌，这中间有
/// 一到几秒的空窗。只有 `account` 两态的话，界面在这段时间只能显示「离线账户」，
/// 等刷新回来再跳成「正版账户」——用户看到的就是账号时有时无。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    /// 当前账号，未登录为 `null`。
    pub account: Option<Account>,
    /// 是否仍在恢复上次的登录。
    pub restoring: bool,
}

/// 账号与登录会话。
pub struct Auth {
    account: Mutex<Option<Account>>,
    /// 同一时刻只允许一个登录在跑。
    ///
    /// 连点两次「登录」会拿到两个设备码，两个轮询各自完成时，先回来的那个会把
    /// 后登录的账号覆盖掉。所以新登录开始前先中止旧的。
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// 刷新令牌的存放位置。凭据库不可用时退化成不持久化。
    pub(crate) store: Arc<dyn TokenStore>,
    /// 是否仍在恢复上次的登录。界面据此显示「正在恢复」而不是「离线账户」。
    restoring: std::sync::atomic::AtomicBool,
    /// 账号状态代际。
    ///
    /// 启动时的恢复任务可能跑好几秒。这期间用户可能已经手动登录或退出登录，
    /// 恢复任务回来时若无条件写入，就会把用户刚做的操作盖掉。代际对不上就丢弃。
    epoch: Mutex<u64>,
}

impl Auth {
    /// 建一份账号状态。
    ///
    /// 凭据库不可用时（比如 Linux 上没有 Secret Service）退化成 [`NullStore`]：
    /// 登录照常可用，只是下次启动要重新登录。
    pub fn new() -> Self {
        let store: Arc<dyn TokenStore> = match KeyringStore::new() {
            Some(store) => Arc::new(store),
            None => Arc::new(NullStore),
        };
        Self {
            account: Mutex::new(None),
            task: Mutex::new(None),
            store,
            // 启动即进入「恢复中」；没有可恢复的令牌时 `restore_session` 会立刻清掉。
            restoring: std::sync::atomic::AtomicBool::new(true),
            epoch: Mutex::new(0),
        }
    }

    /// 当前账号。
    pub fn account(&self) -> Option<Account> {
        self.account.lock().ok().and_then(|slot| slot.clone())
    }

    /// 给界面用的完整状态。
    pub fn state(&self) -> AccountState {
        AccountState {
            account: self.account(),
            restoring: self.restoring.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    fn set_account(&self, account: Option<Account>) {
        if let Ok(mut slot) = self.account.lock() {
            *slot = account;
        }
    }

    /// 恢复流程结束（成功、失败、没有令牌，都算）。
    fn finish_restore(&self) {
        self.restoring
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// 取当前代际。恢复任务在开始前记下它，回来时比对。
    fn epoch(&self) -> u64 {
        self.epoch.lock().map(|value| *value).unwrap_or(0)
    }

    /// 让正在进行的恢复任务作废。
    ///
    /// 手动登录、退出登录都要调：这两件事都表达了用户当下的意愿，
    /// 后台那次「恢复上次登录」的结果已经过时了。
    fn invalidate_restore(&self) {
        if let Ok(mut value) = self.epoch.lock() {
            *value = value.wrapping_add(1);
        }
        self.finish_restore();
    }

    /// 中止正在进行的登录（如果有）。
    fn abort(&self) {
        if let Ok(mut slot) = self.task.lock()
            && let Some(handle) = slot.take()
        {
            handle.abort();
        }
    }
}

/// 取当前账号。未登录时返回 `null`。
#[tauri::command]
pub fn current_account(state: tauri::State<'_, AppState>) -> AccountState {
    state.auth.state()
}

/// 退出登录。
#[tauri::command]
pub fn sign_out(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.auth.abort();
    // 用户明确要退出，后台那次「恢复上次登录」的结果就作废了。
    state.auth.invalidate_restore();
    state.auth.set_account(None);
    // 不清掉的话，下次启动会自动把账号登回来——用户点「退出」显然不期望这个。
    state.auth.store.clear();
    Ok(())
}

/// 开始设备码登录。
///
/// 立即返回：设备码与最终结果都通过 [`LOGIN_EVENT`] 推给界面。做成事件而不是
/// 让前端反复轮询，是因为轮询节奏（含 `slow_down` 要加间隔）属于协议细节，
/// 放在前端就要复制一份，两边一旦不一致就会出难查的问题。
#[tauri::command]
pub fn begin_login(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let Some(client_id) = crate::ms_client_id() else {
        return Err("此构建未启用微软登录（编译时未注入客户端 ID）".to_owned());
    };

    // 用户主动登录，启动时那次恢复的结果不再作数——否则它晚一步回来会把
    // 刚登录的账号顶掉，表现为「登录成功了但界面还是旧账号」。
    state.auth.invalidate_restore();

    let (downloader, _) = state.context()?;

    // 先中止旧的：见 `Auth::task` 的注释。
    state.auth.abort();

    let handle = tauri::async_runtime::spawn({
        let app = app.clone();
        let client_id = client_id.to_owned();
        let store = Arc::clone(&state.auth.store);
        async move {
            let outcome = run_login(&app, &downloader, &client_id, store.as_ref()).await;
            let payload = match outcome {
                Ok(account) => {
                    if let Some(state) = app.try_state::<AppState>() {
                        state.auth.set_account(Some(account.clone()));
                    }
                    LoginState::Done { account }
                }
                Err(message) => LoginState::Failed { message },
            };
            let _ = app.emit(LOGIN_EVENT, payload);
        }
    });

    if let Ok(mut slot) = state.auth.task.lock() {
        *slot = Some(handle);
    }

    Ok(())
}

/// 取消正在进行的登录。
#[tauri::command]
pub fn cancel_login(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.auth.abort();
    // 被 abort 的任务不会自己发事件，界面会一直停在「等待中」，所以这里补一条。
    let _ = app.emit(
        LOGIN_EVENT,
        LoginState::Failed {
            message: "已取消登录".to_owned(),
        },
    );
    Ok(())
}

/// 跑完一次设备码登录。
async fn run_login(
    app: &AppHandle,
    downloader: &HttpDownloader,
    client_id: &str,
    store: &dyn TokenStore,
) -> Result<Account, String> {
    let device = request_device_code(downloader, client_id)
        .await
        .map_err(|error| error.to_string())?;

    // 地址来自响应，界面会把它显示出来再交给浏览器。不校验就等于把「给用户看的
    // 安全地址」交给服务端决定——虽然这条链路是 HTTPS 直连微软，但校验成本极低。
    if !is_trusted_verification_uri(&device.verification_uri) {
        return Err(format!("验证地址不可信：{}", device.verification_uri));
    }

    let _ = app.emit(
        LOGIN_EVENT,
        LoginState::Code {
            user_code: device.user_code.clone(),
            verification_uri: device.verification_uri.clone(),
            expires_in: device.expires_in,
        },
    );

    let tokens = poll_until_granted(downloader, client_id, &device, || {})
        .await
        .map_err(|error| error.to_string())?;

    // 先存刷新令牌再继续：后面换 Minecraft 令牌失败（比如账号没买游戏）时，
    // 用户下次仍然不必重新走一遍设备码。
    if let Some(refresh_token) = tokens.refresh_token.as_deref() {
        let _ = store.save(refresh_token);
    }

    // 微软令牌 → Xbox → XSTS → Minecraft → 档案，一次跑完。
    let info = login_chain(downloader, &tokens.access_token)
        .await
        .map_err(|error| error.to_string())?;

    Ok(Account::from_info(info))
}

/// 启动时用存下的刷新令牌恢复登录。
///
/// 三种结果分开处理，这是之前混在一起导致的两个问题：
///
/// - **没有令牌 / 令牌真失效**（`invalid_grant`）：当作未登录，后者顺手清掉存储。
/// - **网络类失败**：**保留令牌**。以前这里 `Err(_) => clear()`，一次断网就把刷新令牌
///   删了，下次启动只能重新登录——表现成「重启后偶尔要求重新登录」。
/// - 恢复期间界面显示「正在恢复」，而不是先显示「离线账户」再跳成「正版账户」。
pub fn restore_session(app: AppHandle) {
    let Some(client_id) = crate::ms_client_id() else {
        if let Some(state) = app.try_state::<AppState>() {
            state.auth.finish_restore();
        }
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok((downloader, _)) = state.context() else {
        state.auth.finish_restore();
        return;
    };
    let store = Arc::clone(&state.auth.store);
    let epoch = state.auth.epoch();

    // 没有可恢复的令牌时立刻退出「恢复中」，否则界面会一直显示正在恢复。
    if store.load().is_none() {
        state.auth.finish_restore();
        return;
    }

    tauri::async_runtime::spawn(async move {
        let Some(refresh_token) = store.load() else {
            if let Some(state) = app.try_state::<AppState>() {
                state.auth.finish_restore();
            }
            return;
        };

        let outcome =
            refresh_and_login(&downloader, client_id, store.as_ref(), &refresh_token).await;

        let Some(state) = app.try_state::<AppState>() else {
            return;
        };

        // 恢复期间用户可能已经手动登录或退出登录，代际变了就丢弃这次结果。
        if state.auth.epoch() != epoch {
            state.auth.finish_restore();
            return;
        }

        match outcome {
            Ok(account) => {
                state.auth.set_account(Some(account.clone()));
                state.auth.finish_restore();
                // 界面可能已经挂载并取过一次账号，所以这里要推一条事件。
                let _ = app.emit(LOGIN_EVENT, LoginState::Done { account });
            }
            Err(error) => {
                let expired = should_forget_token(&error);
                if expired {
                    // 只有「刷新令牌真失效」才清：留着它下次启动还会白试一次。
                    store.clear();
                }
                state.auth.finish_restore();
                let message = if expired {
                    "登录已过期，请重新登录".to_owned()
                } else {
                    // 网络、协议、解析失败：令牌可能是好的，留到下次启动再试。
                    format!("恢复登录失败：{error}（可继续用离线账户）")
                };
                let _ = app.emit(LOGIN_EVENT, LoginState::Failed { message });
            }
        }
    });
}

/// 这次恢复失败该不该把刷新令牌删掉？
///
/// **只有**「令牌真的不能用了」（`invalid_grant` → [`AuthError::Expired`]）才删。
/// 网络抖动、解析失败、协议异常都可能是一时的，删掉等于把用户踢下线——
/// 这正是「重启后偶尔要求重新登录」的来源。
fn should_forget_token(error: &AuthError) -> bool {
    matches!(error, AuthError::Expired)
}

/// 刷新令牌 → 新的 Minecraft 令牌 → 账号信息。
///
/// 保留 `AuthError` 的具体类型而不是转成字符串：调用方要区分「令牌失效」
/// （该清）与「网络失败」（不该清）。
async fn refresh_and_login(
    downloader: &HttpDownloader,
    client_id: &str,
    store: &dyn TokenStore,
    refresh_token: &str,
) -> Result<Account, AuthError> {
    let tokens = refresh_access_token(downloader, client_id, refresh_token).await?;

    // 微软可能轮换刷新令牌。不把新值存回去，下一次刷新就会拿到 `invalid_grant`，
    // 表现成「用着用着突然要重新登录」。
    if let Some(rotated) = tokens.refresh_token.as_deref() {
        let _ = store.save(rotated);
    }

    let info = login_chain(downloader, &tokens.access_token).await?;

    Ok(Account::from_info(info))
}

#[cfg(test)]
mod tests {
    use super::Account;
    use qcl_core::launch::AccountInfo;

    fn sample() -> Account {
        Account::from_info(AccountInfo {
            player_name: "Steve".to_owned(),
            uuid: "069a79f444e94726a5befca90e38aaf5".to_owned(),
            access_token: "secret-token".to_owned(),
            user_type: "msa".to_owned(),
            client_id: None,
            xuid: Some("2535400000000000".to_owned()),
        })
    }

    #[test]
    fn 序列化给前端时不带令牌() {
        // 界面只需要名字；令牌多传一次就多一个泄露面。
        let json = serde_json::to_string(&sample()).expect("序列化应成功");
        assert!(json.contains("Steve"));
        assert!(
            !json.contains("secret-token"),
            "令牌不该出现在前端载荷里：{json}"
        );
        assert!(!json.contains("2535400000000000"), "xuid 同理：{json}");
    }

    #[test]
    fn 转回启动用的账户信息() {
        let info = sample().to_account_info();
        assert_eq!(info.user_type, "msa");
        assert_eq!(info.access_token, "secret-token");
        assert_eq!(info.uuid, "069a79f444e94726a5befca90e38aaf5");
        assert_eq!(info.xuid.as_deref(), Some("2535400000000000"));
    }
}

#[cfg(test)]
mod token_lifetime_tests {
    use super::should_forget_token;
    use qcl_auth::AuthError;

    #[test]
    fn only_an_expired_token_is_forgotten() {
        // 令牌真失效：留着只会每次启动白试一次。
        assert!(should_forget_token(&AuthError::Expired));
    }

    #[test]
    fn transient_failures_keep_the_token() {
        // 这些都可能是一时的。删掉等于把用户踢下线——「重启后偶尔要求重新登录」
        // 就是从这里来的。
        assert!(!should_forget_token(&AuthError::Network(
            "连接超时".to_owned()
        )));
        assert!(!should_forget_token(&AuthError::Protocol(
            "HTTP 503".to_owned()
        )));
    }
}
