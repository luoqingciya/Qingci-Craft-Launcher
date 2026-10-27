// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Qingci Craft Launcher 的 Tauri 应用层。
//!
//! 这一层刻意保持很薄：**只做 IPC 适配与状态持有，不实现业务算法**。
//! 版本解析、下载、安装、启动全部在 `qcl-*` 领域 crate 里，因此那些逻辑
//! 能在无 GUI 的 CI 中完整测试，不必为「命令处理函数」单独搭一套测试环境。
//!
//! 当前暴露：版本清单、游戏目录设置、安装、启动与停止。

mod auth;
mod game;
mod install;
mod mods;
mod settings;
mod token_store;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use qcl_install::fetch_manifest;
use qcl_net::HttpDownloader;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::game::GameSession;

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
pub struct AppState {
    /// 配置文件路径（`<app_config>/settings.json`）。
    config_path: PathBuf,
    /// 安装目录，即可执行文件所在目录。
    install_dir: PathBuf,
    /// 默认目录不可写时的回退位置（`<app_data>/game`）。
    fallback_dir: PathBuf,
    inner: RwLock<Inner>,
    /// 正在运行的游戏。用 `Arc<Mutex<_>>` 而不是普通 `Mutex`，
    /// 是为了让等待进程的后台任务能直接持有它（任务里拿不到 `State`）。
    game: Arc<Mutex<Option<Arc<GameSession>>>>,
    /// 账号与登录会话。
    auth: auth::Auth,
}

impl AppState {
    /// 取当前的下载器与游戏目录。
    ///
    /// 两者必须成对读取：目录变了下载器会重建，分开取可能拿到不一致的组合。
    fn context(&self) -> Result<(Arc<HttpDownloader>, PathBuf), String> {
        let inner = self
            .inner
            .read()
            .map_err(|_| "内部状态锁已中毒".to_owned())?;
        Ok((Arc::clone(&inner.downloader), inner.game_dir.clone()))
    }

    /// 取一份设置副本。启动游戏时需要玩家名、内存与 Java 选择。
    fn settings_snapshot(&self) -> Result<settings::Settings, String> {
        let inner = self
            .inner
            .read()
            .map_err(|_| "内部状态锁已中毒".to_owned())?;
        Ok(inner.settings.clone())
    }
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
    /// 实际使用的玩家名（已回退到默认值）。
    player_name: String,
    /// 最大堆内存（MB）。`None` 表示采用官方默认值。
    memory_mb: Option<u32>,
    /// 显式指定的 Java 可执行文件。`None` 表示自动选择。
    java_path: Option<String>,
    /// 允许的内存范围，前端据此校验输入。
    memory_range: [u32; 2],
    /// 上次选择的版本。
    selected_version: Option<String>,
    /// 上次选择的加载器版本。
    loader_version: Option<String>,
}

/// 前端提交的设置。
///
/// 全量提交而不是逐字段：设置页有「保存」按钮，一次写清更符合用户预期，
/// 也避免 `Option` 无法区分「没传」与「清空」。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsInput {
    /// 空字符串或 `None` 表示恢复默认目录。
    game_dir: Option<String>,
    /// 空字符串表示使用默认玩家名。
    player_name: Option<String>,
    /// `None` 表示采用官方默认内存。
    memory_mb: Option<u32>,
    /// `None` 表示自动选择 Java。
    java_path: Option<String>,
}

/// 一个可用的 Java 安装，供设置页下拉选择。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JavaOption {
    /// 可执行文件路径（同时作为选项的值）。
    path: String,
    /// 版本字符串，如 `26.0.2`。
    version: String,
    /// 主版本号，便于界面按需求标注。
    major: u32,
    /// 来源描述，如「JAVA_HOME」「启动器管理」。
    source: String,
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
            player_name: inner.settings.player_name_or_default(),
            memory_mb: inner.settings.memory_mb,
            java_path: inner
                .settings
                .java_path
                .as_ref()
                .map(|path| path.display().to_string()),
            memory_range: [settings::MIN_MEMORY_MB, settings::MAX_MEMORY_MB],
            selected_version: inner.settings.selected_version.clone(),
            loader_version: inner.settings.loader_version.clone(),
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

    let manifest = fetch_manifest(&downloader)
        .await
        .map_err(|error| error.to_string())?;

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

/// 保存设置。整体提交，一次写清。
#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, AppState>,
    input: SettingsInput,
) -> Result<SettingsView, String> {
    let normalize = |value: Option<String>| {
        value
            .map(|raw| raw.trim().to_owned())
            .filter(|raw| !raw.is_empty())
    };

    let requested_dir = normalize(input.game_dir).map(PathBuf::from);

    // 自定义目录不可写时直接报错，而不是静默回退——用户刚选完就回退会让人困惑。
    if let Some(dir) = &requested_dir {
        settings::ensure_writable(dir)
            .map_err(|error| format!("{} 不可写：{error}", dir.display()))?;
    }

    // 保留已有选择：保存设置是表单提交，不该顺手把「上次玩的版本」清掉。
    let previous = state.settings_snapshot()?;
    let saved = settings::Settings {
        game_dir: requested_dir,
        player_name: normalize(input.player_name),
        memory_mb: input.memory_mb,
        java_path: normalize(input.java_path).map(PathBuf::from),
        selected_version: previous.selected_version,
        loader_version: previous.loader_version,
    };

    // 校验放在写盘之前：任何一项不合法都不该留下半套配置。
    saved.validate()?;

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

/// 记住当前选择（版本 + 加载器）。
///
/// 与 `save_settings` 分开：切换版本是高频操作，不该连带把设置表单里
/// 还没提交的改动写进去；反过来保存设置也不该清掉选择。
#[tauri::command]
fn remember_selection(
    state: tauri::State<'_, AppState>,
    version: Option<String>,
    loader: Option<String>,
) -> Result<(), String> {
    let mut inner = state
        .inner
        .write()
        .map_err(|_| "内部状态锁已中毒".to_owned())?;

    let saved = inner.settings.with_selection(version, loader);
    settings::save(&state.config_path, &saved).map_err(|error| format!("保存选择失败：{error}"))?;
    inner.settings = saved;

    Ok(())
}

/// 列出本机可用的 Java，供设置页选择。
///
/// 探测顺序与自动选择一致（JAVA_HOME → PATH → 自管目录 → 平台常见目录），
/// 因此下拉里的顺序本身就能说明「自动会选哪个」。
#[tauri::command]
fn list_java(state: tauri::State<'_, AppState>) -> Result<Vec<JavaOption>, String> {
    let (_, game_dir) = state.context()?;

    let installations = qcl_java::detect(&[], Some(&game_dir.join("runtime")));

    Ok(installations
        .into_iter()
        .map(|installation| JavaOption {
            path: installation.executable.display().to_string(),
            version: installation.version.raw.clone(),
            major: installation.version.major,
            source: match installation.source {
                qcl_java::JavaSource::Explicit => "手动指定",
                qcl_java::JavaSource::JavaHome => "JAVA_HOME",
                qcl_java::JavaSource::Path => "PATH",
                qcl_java::JavaSource::Managed => "启动器管理",
                qcl_java::JavaSource::PlatformDirectory => "系统安装目录",
            }
            .to_owned(),
        })
        .collect())
}

/// 本程序的运行环境信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    /// 是否由安装程序装入。
    ///
    /// 判据是安装目录里有没有 NSIS 写的 `uninstall.exe`。用它而不是「路径在
    /// %LOCALAPPDATA% 下」，是因为安装时允许自选路径，而卸载器在哪个目录都会写。
    pub installed: bool,
    /// 本次构建是否内置了微软登录所需的客户端 ID。
    ///
    /// 为 false 时界面要说明「此构建未启用微软登录」，而不是给一个点了必然失败的按钮。
    pub msa_login: bool,
}

/// 去掉首尾空白，空串视为「没有」。
///
/// 单独拆出来是因为它是这段逻辑里唯一值得测的部分：`option_env!` 本身没法在测试里改，
/// 但「空白算不算有效 ID」是可以断言的。
fn normalize_client_id(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|id| !id.is_empty())
}

/// 内置的微软应用（公共客户端）ID。
///
/// 编译期由 `QCL_MS_CLIENT_ID` 注入，仓库里没有这个值——它是发布者自己的 Azure
/// 应用注册，不该出现在源码里。
///
/// 需要说清楚的一点：**它不是机密**。OAuth 2.0 的公共客户端（设备码流程）没有
/// client secret，ID 在每次请求的表单里明文出现，抓包就能看到。所以这里防的不是
/// 「别人拿去用」，而是「把发布者的应用注册固化进仓库与每一份源码副本」。
/// 真要防止别人复用，只能靠 Azure 那边的配置，代码层面做不到。
pub fn ms_client_id() -> Option<&'static str> {
    normalize_client_id(option_env!("QCL_MS_CLIENT_ID"))
}

/// 设置与回退目录该放哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigHome {
    /// 安装版：`%APPDATA%\<标识符>\`。
    Installed,
    /// 便携版且 exe 所在目录可写：设置就放在 exe 旁边。
    Portable,
    /// 便携版但 exe 目录不可写：退到 `%APPDATA%\<标识符>\portable\`。
    PortableFallback,
}

/// 判断设置该放哪。
///
/// 便携版与安装版用的是**同一个应用标识符**，`app_config_dir()` 自然指向同一个目录，
/// 于是一边选的版本会出现在另一边。分开之后：
///
/// - 安装版留在 `%APPDATA%\<标识符>\`——安装形态下用户目录才是应用数据该在的地方
/// - 便携版把设置放在 exe 旁边，整个目录拷走即可迁移
/// - exe 旁边不可写（放在 Program Files 之类）时退到 `%APPDATA%\<标识符>\portable\`，
///   **仍然与安装版分开**
fn config_home(installed: bool, install_dir_writable: bool) -> ConfigHome {
    match (installed, install_dir_writable) {
        (true, _) => ConfigHome::Installed,
        (false, true) => ConfigHome::Portable,
        (false, false) => ConfigHome::PortableFallback,
    }
}

/// 判断是否为安装版。
///
/// 判据是安装目录里有没有 NSIS 写的 `uninstall.exe`。用它而不是「路径在
/// %LOCALAPPDATA% 下」，是因为安装时允许自选路径，而卸载器在哪个目录都会写。
fn is_installed(install_dir: &Path) -> bool {
    install_dir.join("uninstall.exe").is_file()
}

/// 判断当前是否为安装版。
///
/// 界面要据此决定「检查更新」之后能不能自动安装：便携版自更新得替换正在运行的
/// exe，Windows 不允许，只能引导用户去下载。
#[tauri::command]
fn runtime_info() -> Result<RuntimeInfo, String> {
    let exe = std::env::current_exe().map_err(|error| format!("无法定位可执行文件：{error}"))?;
    let installed = exe.parent().map(is_installed).unwrap_or(false);

    Ok(RuntimeInfo {
        installed,
        msa_login: ms_client_id().is_some(),
    })
}

/// 启动应用。
///
/// # Panics
///
/// Tauri 初始化失败（窗口无法创建、配置非法、游戏目录完全不可用）时 panic——
/// 这是启动期错误，没有可恢复的路径。
pub fn run() {
    tauri::Builder::default()
        // 必须排在所有插件最前面：第二个实例要在其余初始化之前就被拦下，
        // 否则会先把窗口、下载器、配置都建起来，再被结束——看起来就是「闪一下」。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二次双击：把已有窗口拉到前台，而不是再开一个。
            // 只 show() 不够——窗口可能是最小化的，用户会以为没反应。
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // 在线更新：updater 负责校验签名并安装，process 负责装完重启自己。
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 便携布局：默认游戏目录在可执行文件旁边，整个目录拷走即可迁移。
            let install_dir = std::env::current_exe()?
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "无法定位安装目录".to_string())?;

            let installed = is_installed(&install_dir);
            let app_config = app.path().app_config_dir()?;
            let app_data = app.path().app_data_dir()?;

            // 安装版与便携版共用同一个应用标识符，不分开的话两份安装会读同一份
            // settings.json——在一边选的版本会出现在另一边。
            let (config_path, fallback_dir) =
                match config_home(installed, settings::is_writable(&install_dir)) {
                    ConfigHome::Installed => (app_config.join("settings.json"), app_data.join("game")),
                    ConfigHome::Portable => (
                        install_dir.join("settings.json"),
                        app_data.join("portable").join("game"),
                    ),
                    ConfigHome::PortableFallback => (
                        app_config.join("portable").join("settings.json"),
                        app_data.join("portable").join("game"),
                    ),
                };

            // 便携版第一次用新位置：把旧共用文件里的用户偏好带过来（只带偏好，
            // 不带版本选择——那正是要分开的东西）。
            if !installed {
                settings::migrate_preferences(&app_config.join("settings.json"), &config_path);
            }

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
                game: Arc::new(Mutex::new(None)),
                auth: auth::Auth::new(),
            });

            // 用存下的刷新令牌恢复登录。失败静默——刚打开的启动器上不该因为
            // 「上次的令牌过期了」弹一条错误。
            auth::restore_session(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_versions,
            get_settings,
            save_settings,
            list_java,
            remember_selection,
            runtime_info,
            auth::current_account,
            auth::begin_login,
            auth::cancel_login,
            auth::sign_out,
            install::install_game_version,
            install::list_loaders,
            mods::search_mods,
            mods::list_mod_categories,
            mods::install_mod,
            mods::list_mods,
            mods::remove_mod,
            mods::check_mod_updates,
            mods::update_mod,
            game::launch_game,
            game::stop_game,
            game::game_status,
            game::reveal_crash_report
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

#[cfg(test)]
mod tests {
    use super::{ConfigHome, config_home, normalize_client_id};

    #[test]
    fn 客户端_id_去掉首尾空白() {
        assert_eq!(normalize_client_id(Some("  abc-123 \n")), Some("abc-123"));
    }

    #[test]
    fn 空白与空串都算没有() {
        // 环境变量被设成空串是常见情况（CI 里 secret 没配），不能当成有效 ID。
        assert_eq!(normalize_client_id(Some("")), None);
        assert_eq!(normalize_client_id(Some("   ")), None);
        assert_eq!(normalize_client_id(None), None);
    }

    #[test]
    fn 安装版用用户目录() {
        assert_eq!(config_home(true, true), ConfigHome::Installed);
        // 装到可写目录也一样：安装形态下「应用数据」就该在用户目录。
        assert_eq!(config_home(true, false), ConfigHome::Installed);
    }

    #[test]
    fn 便携版把设置放在自己旁边() {
        assert_eq!(config_home(false, true), ConfigHome::Portable);
    }

    #[test]
    fn 便携版目录不可写时退回用户目录但仍是独立子目录() {
        // 关键是「独立」：退到 %APPDATA% 也不能和安装版共用一份，
        // 否则又回到「一边选的版本出现在另一边」。
        assert_eq!(config_home(false, false), ConfigHome::PortableFallback);
        assert_ne!(config_home(false, false), config_home(true, false));
    }
}
