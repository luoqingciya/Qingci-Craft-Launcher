// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 启动与停止游戏。
//!
//! 进程的生命周期由 `qcl-runtime` 管理，这里负责：
//!
//! 1. 选 Java、组装命令（领域 crate 提供）
//! 2. 把日志与退出结果转成事件推给界面
//! 3. 让界面上的「停止」能真正终止进程

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use qcl_core::launch::{AccountInfo, LaunchLayout, LaunchOptions, build_launch_command};
use qcl_java::{JavaSource, detect, inspect, select};
use qcl_platform::launch_context;
use qcl_runtime::{CrashReport, LogStream, spawn};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::AppState;

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

/// 崩溃报告里要送到界面的部分。
///
/// 不整份传：报告动辄几百 KB，其中九成是模组清单与系统信息。界面要的只是
/// 「Description 是什么」和紧随其后的几行堆栈。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportPayload {
    /// 报告文件路径，用于「打开所在文件夹」。
    pub path: String,
    /// `Description:` 那一行。
    pub description: Option<String>,
    /// 结论下方紧接着的若干行。
    pub excerpt: Vec<String>,
}

impl From<&CrashReport> for CrashReportPayload {
    fn from(report: &CrashReport) -> Self {
        Self {
            path: report.path.display().to_string(),
            description: report.description.clone(),
            excerpt: report.excerpt.clone(),
        }
    }
}

/// 进程退出。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameExitEvent {
    /// 退出码。被信号终止时为 `null`。
    pub code: Option<i32>,
    /// 运行时长（秒）。
    pub duration_secs: f64,
    /// 本次运行产生的崩溃报告（没有则为 `null`）。
    pub crash_report: Option<CrashReportPayload>,
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
    loader: Option<String>,
) -> Result<(), String> {
    let (downloader, game_dir) = state.context()?;
    let settings = state.settings_snapshot()?;

    if state.game.lock().map_err(|_| "状态锁已中毒")?.is_some() {
        return Err("游戏已在运行".to_owned());
    }

    // 目标版本由「版本 + 加载器」确定性推导，不依赖界面上的临时状态：
    // 选了 Fabric 却启动原版，模组就白装了，而界面上完全看不出来。
    let loader = loader.filter(|value| !value.trim().is_empty());
    let meta =
        crate::install::load_meta_with_loader(&downloader, &version, loader.as_deref()).await?;

    // 启动前补齐文件。幂等，已就绪时几乎不花时间；
    // 缺文件时直接启动只会得到一句「找不到主类」，用户无从判断缺什么。
    let options = qcl_install::VersionInstallOptions::for_version(&meta.id);
    crate::install::install_with_progress(&app, &downloader, &meta, &options).await?;

    // 选 Java。缺版本时明确失败——用低版本硬跑只会得到 UnsupportedClassVersionError。
    let required = meta.required_java_major().unwrap_or(8);
    let java = match &settings.java_path {
        // 手动指定时仍要校验版本：「我选的」不等于「能跑」，否则错误会推迟到游戏启动后。
        Some(executable) => {
            let home = executable
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| format!("无法从 {} 推断 Java 目录", executable.display()))?;
            let installation = inspect(home, JavaSource::Explicit)
                .ok_or_else(|| format!("无法识别指定的 Java：{}", executable.display()))?;

            if installation.version.major < required {
                return Err(format!(
                    "指定的 Java {} 低于该版本所需的 {required}，请更换或改为自动选择",
                    installation.version
                ));
            }
            installation
        }
        None => {
            let installations = detect(&[], Some(&game_dir.join("runtime")));
            let Some(selected) = select(&installations, required) else {
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
            selected.clone()
        }
    };

    let mut layout = LaunchLayout::standard(&meta.id);
    // `${game_directory}` 传绝对路径：游戏会用它拼存档与日志路径。
    layout.game_dir = game_dir.display().to_string();

    let launch_options = launch_options_from(&settings);

    // 已登录就用真实账号，否则回落到离线账户。离线模式在界面上有明确标注，
    // 两者不会混淆。
    let account = state
        .auth
        .account()
        .map(|account| account.to_account_info())
        .unwrap_or_else(|| offline_account(&settings.player_name_or_default()));

    let command = build_launch_command(
        &java.executable,
        &meta,
        &launch_context(),
        &layout,
        &account,
        &launch_options,
    )
    .map_err(|error| format!("组装启动命令失败：{error}"))?;

    let mut process = spawn(&command, &game_dir).map_err(|error| error.to_string())?;

    let session = GameSession::new(meta.id.clone());
    {
        let mut slot = state.game.lock().map_err(|_| "状态锁已中毒")?;
        *slot = Some(Arc::clone(&session));
    }

    // 记下启动时刻：只有这之后写出的崩溃报告才属于这一局。
    let started = SystemTime::now();
    // 崩溃报告在后台任务里找，所以目录要活过这个函数。
    let report_dir = game_dir.clone();
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

        // 只有异常退出才找报告：正常退出（0）时目录里那份一定是旧的。
        let exit_code = outcome.as_ref().ok().and_then(|outcome| outcome.exit_code);
        let crash_report = if exit_code.is_some_and(|code| code != 0) {
            qcl_runtime::crash::find_since(&report_dir, started)
                .and_then(|path| qcl_runtime::crash::read(&path))
                .as_ref()
                .map(CrashReportPayload::from)
        } else {
            None
        };

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
                crash_report,
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
                    crash_report: None,
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

/// 在系统文件管理器里选中崩溃报告。
///
/// 路径由前端回传，所以这里必须再校验一次：确认它落在当前游戏目录的
/// `crash-reports/` 下。否则这个命令就成了「让系统用默认程序打开任意文件」的通道。
#[tauri::command]
pub fn reveal_crash_report(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let (_, game_dir) = state.context()?;
    let reports = dunce::canonicalize(qcl_runtime::crash::directory(&game_dir))
        .map_err(|error| format!("崩溃报告目录不可用：{error}"))?;
    let report = dunce::canonicalize(Path::new(&path))
        .map_err(|error| format!("崩溃报告不存在：{error}"))?;

    if !report.starts_with(&reports) {
        return Err("崩溃报告不在游戏目录内，已拒绝".to_owned());
    }

    app.opener()
        .reveal_item_in_dir(&report)
        .map_err(|error| format!("打开文件管理器失败：{error}"))
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
fn offline_account(player_name: &str) -> AccountInfo {
    AccountInfo {
        player_name: player_name.to_owned(),
        // 全零 UUID 是离线模式的常见约定；正版服务器会拒绝，单机与离线服可用。
        uuid: "00000000000000000000000000000000".to_owned(),
        access_token: "0".to_owned(),
        user_type: "legacy".to_owned(),
        client_id: None,
        xuid: None,
    }
}

/// 把设置映射成启动选项。
///
/// 单独抽出来是为了能测：这段映射是「设置页改了内存，游戏却还用默认值」这类
/// 缺陷的唯一发生地，而它本身是纯函数。
fn launch_options_from(settings: &crate::settings::Settings) -> LaunchOptions {
    LaunchOptions {
        memory_mb: settings.memory_mb,
        ..LaunchOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{CrashReportPayload, GameExitEvent, launch_options_from, offline_account};
    use crate::settings::{DEFAULT_PLAYER_NAME, Settings};
    use qcl_runtime::CrashReport;
    use std::path::PathBuf;

    #[test]
    fn exit_event_carries_crash_report_in_camel_case() {
        // 字段名是 IPC 契约，前端按 `crashReport.path` 取值；
        // serde 的 rename 写错了界面会静默地拿不到报告。
        let report = CrashReport {
            path: PathBuf::from("/game/crash-reports/crash-1-client.txt"),
            description: Some("Rendering overlay".to_owned()),
            excerpt: vec!["java.lang.NullPointerException: boom".to_owned()],
        };
        let event = GameExitEvent {
            code: Some(1),
            duration_secs: 12.5,
            crash_report: Some(CrashReportPayload::from(&report)),
        };

        let json = serde_json::to_value(&event).expect("序列化退出事件");
        assert_eq!(json["code"], 1);
        assert_eq!(json["durationSecs"], 12.5);
        assert_eq!(
            json["crashReport"]["path"],
            "/game/crash-reports/crash-1-client.txt"
        );
        assert_eq!(json["crashReport"]["description"], "Rendering overlay");
        assert_eq!(
            json["crashReport"]["excerpt"][0],
            "java.lang.NullPointerException: boom"
        );
    }

    #[test]
    fn exit_event_without_report_serializes_null() {
        let event = GameExitEvent {
            code: Some(0),
            duration_secs: 1.0,
            crash_report: None,
        };

        let json = serde_json::to_value(&event).expect("序列化退出事件");
        assert!(json["crashReport"].is_null());
    }

    #[test]
    fn memory_setting_reaches_launch_options() {
        let settings = Settings {
            memory_mb: Some(6144),
            ..Settings::default()
        };
        assert_eq!(launch_options_from(&settings).memory_mb, Some(6144));
    }

    #[test]
    fn unset_memory_keeps_official_default() {
        // `None` 表示「交给官方 default-user-jvm」，不能变成 0 或某个写死的值。
        assert_eq!(launch_options_from(&Settings::default()).memory_mb, None);
    }

    #[test]
    fn player_name_reaches_offline_account() {
        assert_eq!(offline_account("清辞").player_name, "清辞");
        assert_eq!(
            offline_account(DEFAULT_PLAYER_NAME).player_name,
            DEFAULT_PLAYER_NAME
        );
    }

    #[test]
    fn offline_account_has_no_real_credentials() {
        let account = offline_account("x");
        assert_eq!(account.access_token, "0");
        assert_eq!(account.user_type, "legacy");
        assert!(account.client_id.is_none());
    }
}
