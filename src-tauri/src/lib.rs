//! Qingci Craft Launcher 的 Tauri 应用层。
//!
//! 这一层刻意保持很薄：**只做 IPC 适配与状态持有，不实现业务算法**。
//! 版本解析、下载、安装、启动全部在 `qcl-*` 领域 crate 里，因此那些逻辑
//! 能在无 GUI 的 CI 中完整测试，不必为「命令处理函数」单独搭一套测试环境。
//!
//! 当前只暴露一条链路：`list_versions`。安装与启动在后续 PR 接入。

use std::path::PathBuf;
use std::sync::Arc;

use qcl_core::version::VersionManifest;
use qcl_net::HttpDownloader;
use serde::Serialize;
use tauri::Manager;

/// 官方版本清单地址。
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 应用状态。
struct AppState {
    /// 复用同一个 HTTP 客户端：每次命令都新建会重复建连接池与 TLS 上下文。
    downloader: Arc<HttpDownloader>,
    /// 游戏目录（`<app_data>/game`）。
    game_dir: PathBuf,
}

/// 传给前端的版本条目。
///
/// 只暴露界面真正需要的字段。把整份清单（含每条目的 url / sha1）直接丢给前端
/// 会让契约随上游 schema 变化而频繁变动。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionSummary {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    release_time: String,
}

/// 拉取官方版本清单。
#[tauri::command]
async fn list_versions(state: tauri::State<'_, AppState>) -> Result<Vec<VersionSummary>, String> {
    let raw = state
        .downloader
        .fetch_text(VERSION_MANIFEST_URL)
        .await
        .map_err(|error| format!("获取版本清单失败：{error}"))?;

    let manifest: VersionManifest =
        serde_json::from_str(&raw).map_err(|error| format!("解析版本清单失败：{error}"))?;

    Ok(manifest
        .versions
        .into_iter()
        .map(|version| VersionSummary {
            id: version.id,
            version_type: version.version_type.as_str().to_owned(),
            release_time: version.release_time,
        })
        .collect())
}

/// 当前游戏目录，供界面显示与排查问题。
#[tauri::command]
fn game_directory(state: tauri::State<'_, AppState>) -> String {
    state.game_dir.display().to_string()
}

/// 启动应用。
///
/// # Panics
///
/// Tauri 初始化失败（窗口无法创建、配置非法）时 panic——这是启动期错误，
/// 没有可恢复的路径。
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 游戏目录放在应用数据目录下，而不是安装目录：安装目录可能不可写
            // （macOS 的 .app、Windows 的 Program Files）。
            let game_dir = app.path().app_data_dir()?.join("game");
            std::fs::create_dir_all(&game_dir)?;

            let downloader = HttpDownloader::new(&game_dir)?;

            app.manage(AppState {
                downloader: Arc::new(downloader),
                game_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_versions, game_directory])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
