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

use std::sync::Mutex;

use qcl_auth::{is_trusted_verification_uri, login_chain, poll_until_granted, request_device_code};
use qcl_core::launch::AccountInfo;
use qcl_net::HttpDownloader;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

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

/// 账号与登录会话。
#[derive(Default)]
pub struct Auth {
    account: Mutex<Option<Account>>,
    /// 同一时刻只允许一个登录在跑。
    ///
    /// 连点两次「登录」会拿到两个设备码，两个轮询各自完成时，先回来的那个会把
    /// 后登录的账号覆盖掉。所以新登录开始前先中止旧的。
    task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Auth {
    /// 当前账号。
    pub fn account(&self) -> Option<Account> {
        self.account.lock().ok().and_then(|slot| slot.clone())
    }

    fn set_account(&self, account: Option<Account>) {
        if let Ok(mut slot) = self.account.lock() {
            *slot = account;
        }
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
pub fn current_account(state: tauri::State<'_, AppState>) -> Option<Account> {
    state.auth.account()
}

/// 退出登录。
#[tauri::command]
pub fn sign_out(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.auth.abort();
    state.auth.set_account(None);
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

    let (downloader, _) = state.context()?;

    // 先中止旧的：见 `Auth::task` 的注释。
    state.auth.abort();

    let handle = tauri::async_runtime::spawn({
        let app = app.clone();
        let client_id = client_id.to_owned();
        async move {
            let outcome = run_login(&app, &downloader, &client_id).await;
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

    // 微软令牌 → Xbox → XSTS → Minecraft → 档案，一次跑完。
    let info = login_chain(downloader, &tokens.access_token)
        .await
        .map_err(|error| error.to_string())?;

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
