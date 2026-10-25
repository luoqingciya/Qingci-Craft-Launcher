//! Qingci Craft Launcher 的 Tauri 应用层。
//!
//! 这一层刻意保持很薄：**只做 IPC 适配与状态持有，不实现业务算法**。
//! 版本解析、下载、安装、启动全部在 `qcl-*` 领域 crate 里，因此那些逻辑
//! 能在无 GUI 的 CI 中完整测试，不必为「命令处理函数」单独搭一套测试环境。
//!
//! 当前暴露：版本清单、游戏目录的读取与修改。

mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use qcl_core::version::VersionManifest;
use qcl_net::HttpDownloader;
use serde::Serialize;
use tauri::Manager;

/// 官方版本清单地址。
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 运行时状态。设置、生效目录与下载器必须一起变更，因此放在同一把锁下。
struct Inner {
    /// 用户显式保存的设置。
    settings: settings::Settings,
    /// 当前实际使用的游戏目录。
    game_dir: PathBuf,
    /// 默认目录不可用时回退的原因，供界面提示。
    fallback_reason: Option<String>,
    /// 与 `game_dir` 绑定的下载器：目录变了必须重建。
    downloader: Arc<HttpDownloader>,
}

/// 应用状态。
struct AppState {
    /// 配置文件路径（`<app_config>/settings.json`）。
    config_path: PathBuf,
    /// 安装目录，即可执行文件所在目录。
    install_dir: PathBuf,
    /// 默认目录不可写时的回退位置（`<app_data>/game`）。
    fallback_dir: PathBuf,
    inner: RwLock<Inner>,
}

/// 传给前端的设置视图。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    /// 当前生效的游戏目录。
    game_dir: String,
    /// 默认目录（安装目录下），供「恢复默认」使用。
    default_game_dir: String,
    /// 是否使用了自定义目录。
    is_custom: bool,
    /// 默认目录不可用时的回退原因。
    fallback_reason: Option<String>,
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

/// 解析出实际生效的游戏目录。
///
/// 自定义目录优先；未自定义时用安装目录下的 `game`。两者都先探测可写性，
/// 失败则退到应用数据目录并记录原因——便携布局在 Program Files 下必然不可写，
/// 静默失败会让用户看到「下载成功但文件不见了」。
fn resolve_game_dir(
    saved: &settings::Settings,
    install_dir: &Path,
    fallback_dir: &Path,
) -> (PathBuf, Option<String>) {
    let preferred = saved
        .game_dir
        .clone()
        .unwrap_or_else(|| settings::default_game_dir(install_dir));

    match settings::ensure_writable(&preferred) {
        Ok(resolved) => (resolved, None),
        Err(error) => {
            let reason = format!(
                "{} 不可写（{error}），已回退到 {}",
                preferred.display(),
                fallback_dir.display()
            );
            (fallback_dir.to_path_buf(), Some(reason))
        }
    }
}

impl AppState {
    /// 组装设置视图。
    fn view(&self, inner: &Inner) -> SettingsView {
        SettingsView {
            game_dir: inner.game_dir.display().to_string(),
            default_game_dir: settings::default_game_dir(&self.install_dir)
                .display()
                .to_string(),
            is_custom: inner.settings.game_dir.is_some(),
            fallback_reason: inner.fallback_reason.clone(),
        }
    }
}

/// 拉取官方版本清单。
#[tauri::command]
async fn list_versions(state: tauri::State<'_, AppState>) -> Result<Vec<VersionSummary>, String> {
    let downloader = {
        let inner = state
            .inner
            .read()
            .map_err(|_| "内部状态锁已中毒".to_owned())?;
        Arc::clone(&inner.downloader)
    };

    let raw = downloader
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

/// 读取当前设置。
#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsView, String> {
    let inner = state
        .inner
        .read()
        .map_err(|_| "内部状态锁已中毒".to_owned())?;
    Ok(state.view(&inner))
}

/// 设置游戏目录。传入 `None` 或空串表示恢复默认。
#[tauri::command]
async fn set_game_dir(
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> Result<SettingsView, String> {
    let requested = path
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from);

    // 自定义目录不可写时直接报错，而不是静默回退——用户刚选完就回退会让人困惑。
    if let Some(dir) = &requested {
        settings::ensure_writable(dir)
            .map_err(|error| format!("{} 不可写：{error}", dir.display()))?;
    }

    let saved = settings::Settings {
        game_dir: requested,
    };
    let (game_dir, fallback_reason) =
        resolve_game_dir(&saved, &state.install_dir, &state.fallback_dir);

    let downloader = HttpDownloader::new(&game_dir)
        .map_err(|error| format!("无法在 {} 初始化下载器：{error}", game_dir.display()))?;

    settings::save(&state.config_path, &saved).map_err(|error| format!("保存设置失败：{error}"))?;

    let mut inner = state
        .inner
        .write()
        .map_err(|_| "内部状态锁已中毒".to_owned())?;
    inner.settings = saved;
    inner.game_dir = game_dir;
    inner.fallback_reason = fallback_reason;
    inner.downloader = Arc::new(downloader);

    Ok(state.view(&inner))
}

/// 启动应用。
///
/// # Panics
///
/// Tauri 初始化失败（窗口无法创建、配置非法、游戏目录完全不可用）时 panic——
/// 这是启动期错误，没有可恢复的路径。
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 便携布局：默认游戏目录在可执行文件旁边，整个目录拷走即可迁移。
            let install_dir = std::env::current_exe()?
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "无法定位安装目录".to_string())?;

            let config_path = app.path().app_config_dir()?.join("settings.json");
            let fallback_dir = app.path().app_data_dir()?.join("game");

            let saved = settings::load(&config_path);
            let (game_dir, fallback_reason) = resolve_game_dir(&saved, &install_dir, &fallback_dir);

            // 连回退目录都不可写时无路可走，明确失败好过后续每一步都报错。
            let game_dir = settings::ensure_writable(&game_dir)?;
            let downloader = HttpDownloader::new(&game_dir)?;

            app.manage(AppState {
                config_path,
                install_dir,
                fallback_dir,
                inner: RwLock::new(Inner {
                    settings: saved,
                    game_dir,
                    fallback_reason,
                    downloader: Arc::new(downloader),
                }),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_versions,
            get_settings,
            set_game_dir
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
