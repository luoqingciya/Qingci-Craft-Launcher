//! 启动与停止游戏。
//!
//! 进程的生命周期由 `qcl-runtime` 管理，这里负责：
//!
//! 1. 选 Java、组装命令（领域 crate 提供）
//! 2. 把日志与退出结果转成事件推给界面
//! 3. 让界面上的「停止」能真正终止进程

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use qcl_core::launch::{AccountInfo, LaunchLayout, LaunchOptions, build_launch_command};
use qcl_java::{detect, select};
use qcl_platform::launch_context;
use qcl_runtime::{LogStream, spawn};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::AppState;
use crate::install::load_meta;

/// 日志事件名。
pub const GAME_LOG_EVENT: &str = "game-log";
/// 进程退出事件名。
pub const GAME_EXIT_EVENT: &str = "game-exit";

/// 一行日志。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLogEvent {
    /// `stdout` 或 `stderr`。
    pub stream: &'static str,
    /// 行内容（不含换行符）。
    pub text: String,
}

/// 进程退出。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameExitEvent {
    /// 退出码。被信号终止时为 `null`。
    pub code: Option<i32>,
    /// 运行时长（秒）。
    pub duration_secs: f64,
}

/// 一次游戏运行。
///
/// 停止请求用「标志 + Notify」而不是单纯 `Notify`：若界面在等待逻辑注册之前
/// 就点了停止，只靠 `notify_waiters()` 会丢失这次请求。
pub struct GameSession {
    /// 正在运行的版本。
    pub version: String,
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl GameSession {
    fn new(version: String) -> Arc<Self> {
        Arc::new(Self {
            version,
            cancelled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        })
    }

    /// 请求停止。
    pub fn request_stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// 等待停止请求。已经请求过则立即返回。
    async fn cancelled(self: Arc<Self>) {
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return;
            }
            // 先注册再复查，避免「复查与注册之间」到达的请求被丢掉。
            let notified = self.notify.notified();
            if self.cancelled.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// 当前运行状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    /// 是否正在运行。
    pub running: bool,
    /// 正在运行的版本号。
    pub version: Option<String>,
}

/// 启动游戏。
///
/// 立即返回：进程在后台任务里等待，日志与退出通过事件推送。否则界面会在整个
/// 游戏会话期间无法响应其他命令。
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    version: String,
) -> Result<(), String> {
    let (downloader, game_dir) = state.context()?;

    if state.game.lock().map_err(|_| "状态锁已中毒")?.is_some() {
        return Err("游戏已在运行".to_owned());
    }

    let meta = load_meta(&downloader, &version).await?;

    // 选 Java。缺版本时明确失败——用低版本硬跑只会得到 UnsupportedClassVersionError。
    let required = meta.required_java_major().unwrap_or(8);
    let installations = detect(&[], Some(&game_dir.join("runtime")));
    let Some(java) = select(&installations, required) else {
        let found = if installations.is_empty() {
            "（无）".to_owned()
        } else {
            installations
                .iter()
                .map(|java| java.version.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(format!(
            "需要 Java {required}，但本机未找到。已探测到：{found}"
        ));
    };

    let mut layout = LaunchLayout::standard(&meta.id);
    // `${game_directory}` 传绝对路径：游戏会用它拼存档与日志路径。
    layout.game_dir = game_dir.display().to_string();

    let command = build_launch_command(
        &java.executable,
        &meta,
        &launch_context(),
        &layout,
        &offline_account(),
        &LaunchOptions::default(),
    )
    .map_err(|error| format!("组装启动命令失败：{error}"))?;

    let mut process = spawn(&command, &game_dir).map_err(|error| error.to_string())?;

    let session = GameSession::new(meta.id.clone());
    {
        let mut slot = state.game.lock().map_err(|_| "状态锁已中毒")?;
        *slot = Some(Arc::clone(&session));
    }

    let handle = app.clone();
    let slot_handle = Arc::clone(&state.game);
    tauri::async_runtime::spawn(async move {
        let log_handle = handle.clone();
        let cancel = Arc::clone(&session).cancelled();

        let outcome = process
            .wait_or_cancel(cancel, move |line| {
                let _ = log_handle.emit(
                    GAME_LOG_EVENT,
                    GameLogEvent {
                        stream: match line.stream {
                            LogStream::Stdout => "stdout",
                            LogStream::Stderr => "stderr",
                        },
                        text: line.text,
                    },
                );
            })
            .await;

        // 无论成功失败都要释放槽位，否则界面会一直显示「运行中」。
        if let Ok(mut slot) = slot_handle.lock() {
            // 只清理自己这一场：期间可能已经开始了新的一局。
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &session))
            {
                *slot = None;
            }
        }

        let payload = match outcome {
            Ok(outcome) => GameExitEvent {
                code: outcome.exit_code,
                duration_secs: outcome.duration.as_secs_f64(),
            },
            Err(error) => {
                let _ = handle.emit(
                    GAME_LOG_EVENT,
                    GameLogEvent {
                        stream: "stderr",
                        text: format!("等待游戏进程失败：{error}"),
                    },
                );
                GameExitEvent {
                    code: None,
                    duration_secs: 0.0,
                }
            }
        };
        let _ = handle.emit(GAME_EXIT_EVENT, payload);
    });

    Ok(())
}

/// 请求停止游戏。
#[tauri::command]
pub fn stop_game(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let slot = state.game.lock().map_err(|_| "状态锁已中毒")?;
    match slot.as_ref() {
        Some(session) => {
            session.request_stop();
            Ok(())
        }
        None => Err("当前没有正在运行的游戏".to_owned()),
    }
}

/// 查询运行状态。界面重新挂载时用它同步。
#[tauri::command]
pub fn game_status(state: tauri::State<'_, AppState>) -> Result<GameStatus, String> {
    let slot = state.game.lock().map_err(|_| "状态锁已中毒")?;
    Ok(match slot.as_ref() {
        Some(session) => GameStatus {
            running: true,
            version: Some(session.version.clone()),
        },
        None => GameStatus {
            running: false,
            version: None,
        },
    })
}

/// 离线账户。账号体系尚未接入，先用占位凭据。
fn offline_account() -> AccountInfo {
    AccountInfo {
        player_name: "Player".to_owned(),
        // 全零 UUID 是离线模式的常见约定；正版服务器会拒绝，单机与离线服可用。
        uuid: "00000000000000000000000000000000".to_owned(),
        access_token: "0".to_owned(),
        user_type: "legacy".to_owned(),
        client_id: None,
        xuid: None,
    }
}
