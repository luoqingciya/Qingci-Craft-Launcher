// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 安装命令：把 `qcl-install` 的安装流程接到 IPC 上。
//!
//! 这里只做三件事：取上下文、转发进度、把结果整理成前端能直接显示的形状。
//! 真正的安装顺序与失败归类都在 `qcl_install` 里，因此那部分逻辑仍可在无 GUI
//! 的测试中验证。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use qcl_core::version::VersionMeta;
use qcl_install::{
    InstallPhase, InstallProgress, VersionInstallOptions, fetch_loader_list, fetch_manifest,
    fetch_meta, fetch_profile, inherits_from, install_version, merge, resolve_libraries,
    resolve_version,
};
use qcl_net::HttpDownloader;
use qcl_platform::launch_context;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::AppState;

/// 进度事件名。前端 `listen` 同一个字符串。
pub const INSTALL_PROGRESS_EVENT: &str = "install-progress";

/// 安装进度事件负载。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgressEvent {
    /// `versionFiles` 或 `assets`。
    pub phase: &'static str,
    /// 已完成文件数（含跳过）。
    pub completed: usize,
    /// 该阶段文件总数。
    pub total: usize,
    /// 按文件数计算的百分比。
    pub percent: u32,
}

/// 安装结果摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSummary {
    /// 版本号。
    pub version: String,
    /// 版本文件计划数。
    pub files: usize,
    /// 实际下载数。
    pub downloaded: usize,
    /// 跳过数（已存在且校验通过）。
    pub skipped: usize,
    /// 资产对象数。
    pub assets: usize,
    /// 解压出的 natives 文件数。
    pub natives: usize,
}

/// 拉取版本元数据。安装与启动都要用，抽出来避免两处各写一遍。
///
/// 先看本地再回上游：加载器版本（如 `fabric-loader-0.20.1-26.3`）**不在官方清单里**，
/// 它是安装时合并出来的，合并结果已写进 `versions/<id>/<id>.json`。
/// 不先查本地的话，装完加载器版本会因为「清单里找不到这个版本」而启动失败。
pub(crate) async fn load_meta(
    downloader: &HttpDownloader,
    version: &str,
) -> Result<VersionMeta, String> {
    if let Some(meta) = read_local_meta(downloader.game_dir(), version) {
        return Ok(meta);
    }

    let manifest = fetch_manifest(downloader)
        .await
        .map_err(|error| error.to_string())?;
    let entry = resolve_version(&manifest, version).map_err(|error| error.to_string())?;
    fetch_meta(downloader, entry)
        .await
        .map_err(|error| error.to_string())
}

/// 读取本地已合并的版本元数据。文件不存在或损坏时返回 `None`，让调用方回上游。
fn read_local_meta(game_dir: &Path, version: &str) -> Option<VersionMeta> {
    let path = meta_path(game_dir, version);
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 把 Forge / NeoForge 版本 JSON 的 `inheritsFrom` 合并进来。
///
/// 安装器写出的那份只有 29 个库，没有 `assetIndex`、没有 `downloads`，游戏参数里也没有
/// `--username` 一类——这些都在原版版本里。不合并就直接启动，得到的是一条缺 88 个库、
/// 缺资产索引、缺玩家参数的命令。
async fn resolve_forge_meta(
    downloader: &HttpDownloader,
    meta: VersionMeta,
) -> Result<VersionMeta, String> {
    qcl_install::resolve_inherited(downloader, downloader.game_dir(), meta)
        .await
        .map_err(|error| format!("合并原版版本信息失败：{error}"))
}

/// 写入合并后的版本元数据。
fn write_local_meta(game_dir: &Path, meta: &VersionMeta) -> Result<(), String> {
    let path = meta_path(game_dir, &meta.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(meta)
        .map_err(|error| format!("序列化版本元数据失败：{error}"))?;
    std::fs::write(&path, raw).map_err(|error| format!("写入 {} 失败：{error}", path.display()))
}

fn meta_path(game_dir: &Path, version: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(version)
        .join(format!("{version}.json"))
}

/// 取版本元数据，可选叠加加载器。
///
/// `loader` 为 `None` 时就是原版。加载器版本会先拉取 Fabric 的增量 profile、
/// 补齐构件信息，再与原版合并，并把合并结果落盘——这样下次启动不必重新请求。
/// 加载器种类。IPC 传字符串，缺省（或无法识别）按 Fabric 处理——
/// 这个字段是后加的，之前的调用方只传版本号，含义就是 Fabric。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoaderKind {
    Fabric,
    Forge(qcl_install::ForgeKind),
}

impl LoaderKind {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).unwrap_or_default() {
            "" | "fabric" => Self::Fabric,
            other => match qcl_install::ForgeKind::parse(other) {
                Some(kind) => Self::Forge(kind),
                // 认不出来时退回 Fabric 而不是报错：旧客户端不传这个字段。
                None => Self::Fabric,
            },
        }
    }
}

/// 加载器安装要用的代理（`host:port`），来自设置里的「安装器代理」。
///
/// 现在只有 1.12.2 及更早的官方安装器真的需要它：1.13 起的 Forge / NeoForge
/// 走自研 processor 链，联网那一步（Mojang mappings）已经改走下载器与镜像。
pub(crate) fn install_proxy(settings: &crate::settings::Settings) -> Option<String> {
    settings
        .installer_proxy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// 安装器用的 Java。它要求 Java 8+（processor 也是独立 JVM），与游戏本身的 Java
/// 版本无关，所以不去复用启动路径那套「按版本选 Java」的逻辑。
fn resolve_installer_java(game_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let installations = qcl_java::detect(&[], Some(&game_dir.join("runtime")));
    qcl_java::select(&installations, 8)
        .map(|installation| installation.executable.clone())
        .ok_or_else(|| {
            "安装 Forge / NeoForge 需要一个 Java 运行时，但没有检测到。请先安装 Java。".to_owned()
        })
}

pub(crate) async fn load_meta_with_loader(
    downloader: &HttpDownloader,
    version: &str,
    loader: Option<&str>,
    kind: LoaderKind,
    java: Option<&std::path::Path>,
    proxy: Option<&str>,
    loader_progress: Option<qcl_install::StepCallback<'_>>,
) -> Result<VersionMeta, String> {
    let parent = load_meta(downloader, version).await?;

    let Some(loader_version) = loader else {
        return Ok(parent);
    };

    // Forge / NeoForge 的版本 JSON 是**安装器生成的**，不是我们从上游合并出来的。
    // 装过一次之后它就在 `versions/<id>/` 里，直接读。
    if let LoaderKind::Forge(kind) = kind {
        let id = qcl_install::forge_version_id(kind, &parent.id, loader_version);
        if let Some(meta) = read_local_meta(downloader.game_dir(), &id) {
            return resolve_forge_meta(downloader, meta).await;
        }

        let Some(java) = java else {
            return Err(format!(
                "{} {loader_version} 还没有安装，请先安装该加载器",
                kind.display()
            ));
        };

        let noop = |_: usize, _: usize| {};
        qcl_install::install_forge(
            downloader,
            kind,
            java,
            &parent.id,
            loader_version,
            downloader.game_dir(),
            qcl_install::InstallHooks {
                proxy,
                on_step: loader_progress.unwrap_or(&noop),
            },
        )
        .await
        .map_err(|error| format!("安装 {} 失败：{error}", kind.display()))?;

        let meta = read_local_meta(downloader.game_dir(), &id)
            .ok_or_else(|| format!("{} 安装器没有生成版本文件", kind.display()))?;
        return resolve_forge_meta(downloader, meta).await;
    }

    // 装过一次之后，合并结果就落在 `versions/<id>/<id>.json`。启动前先看本地：
    // 每次启动都去上游取 profile 与库摘要，断网就等于拒绝启动——而文件其实早就齐了。
    let merged_id = qcl_install::merged_version_id(&parent.id, loader_version);
    if let Some(meta) = read_local_meta(downloader.game_dir(), &merged_id) {
        return Ok(meta);
    }

    let profile = fetch_profile(downloader, &parent.id, loader_version)
        .await
        .map_err(|error| format!("获取 Fabric profile 失败：{error}"))?;

    // 上游理论上只返回匹配的组合，但拿到不匹配的 profile 会合并出缺库的版本，
    // 那种错误要到启动时才暴露，不如在这里明确失败。
    if !inherits_from(&profile, &parent.id) {
        return Err(format!(
            "该加载器版本对应的原版是 {}，与所选 {} 不一致",
            profile.inherits_from, parent.id
        ));
    }

    let libraries = resolve_libraries(downloader, &profile)
        .await
        .map_err(|error| format!("解析加载器依赖失败：{error}"))?;
    let merged = merge(&parent, &profile, libraries);

    write_local_meta(downloader.game_dir(), &merged)?;

    Ok(merged)
}

/// 安装指定版本，可选叠加加载器。
///
/// 幂等：已就绪的文件会被跳过，因此「启动前先调一次」也能起到修复作用。
#[tauri::command]
pub async fn install_game_version(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    version: String,
    loader: Option<String>,
    loader_kind: Option<String>,
) -> Result<InstallSummary, String> {
    let (downloader, game_dir) = state.context()?;

    let loader = loader.filter(|value| !value.trim().is_empty());
    let kind = LoaderKind::parse(loader_kind.as_deref());
    // 只有 Forge / NeoForge 需要跑安装器，才需要 Java。
    let java = match kind {
        LoaderKind::Fabric => None,
        LoaderKind::Forge(_) => Some(resolve_installer_java(&game_dir)?),
    };
    // 读一次设置拿代理。锁在这里立即释放，后面要 await 的安装过程不持锁。
    let proxy = {
        let inner = state
            .inner
            .read()
            .map_err(|_| "内部状态锁已中毒".to_owned())?;
        install_proxy(&inner.settings)
    };
    // 加载器安装要跑 processor（独立 JVM，一步几秒到几十秒），没有反馈用户只能干等。
    // 单独一个 `loader` 阶段，按步骤数报百分比。
    let on_loader_step = |completed: usize, total: usize| {
        let percent = u32::try_from(completed * 100 / total.max(1)).unwrap_or(100);
        let _ = app.emit(
            INSTALL_PROGRESS_EVENT,
            InstallProgressEvent {
                phase: "loader",
                completed,
                total,
                percent,
            },
        );
    };

    // Forge / NeoForge 的 processor 要切分并给**原版 client jar** 打补丁，
    // 而那个 jar 本来是由下面的 `install_with_progress` 下载的——顺序反了就会报
    // 「缺少原版 client jar」。所以装加载器之前，先把原版文件备齐。
    if matches!(kind, LoaderKind::Forge(_)) && loader.is_some() {
        ensure_vanilla_ready(&app, &downloader, &version).await?;
    }

    let meta = load_meta_with_loader(
        &downloader,
        &version,
        loader.as_deref(),
        kind,
        java.as_deref(),
        proxy.as_deref(),
        Some(&on_loader_step),
    )
    .await?;
    let options = VersionInstallOptions::for_version(&meta.id);

    let report = install_with_progress(&app, &downloader, &meta, &options).await?;

    Ok(InstallSummary {
        version: meta.id,
        files: report.planned_files,
        downloaded: report.downloaded,
        skipped: report.skipped,
        assets: report.assets.objects,
        natives: report.natives.extracted,
    })
}

/// 安装并向前端推送进度。
///
/// 启动前也要调它做「补齐」，所以抽出来而不是留在命令里。
/// 确保原版文件已就绪——加载器安装的前置条件。
///
/// Forge / NeoForge 的 processor 要切分并给**原版 client jar** 打补丁。那个 jar 由
/// [`install_with_progress`] 下载，而加载器安装排在它前面，于是第一次装加载器必然
/// 报「缺少原版 client jar」，用户只能自己去点一次「安装/检查文件」再回来。
/// 这里先补上这一步，幂等：原版已就绪时只做一次存在性检查。
async fn ensure_vanilla_ready(
    app: &AppHandle,
    downloader: &Arc<HttpDownloader>,
    version: &str,
) -> Result<(), String> {
    let jar = downloader
        .game_dir()
        .join("versions")
        .join(version)
        .join(format!("{version}.jar"));
    if jar.is_file() {
        return Ok(());
    }

    let meta = load_meta(downloader, version).await?;
    let options = VersionInstallOptions::for_version(&meta.id);
    install_with_progress(app, downloader, &meta, &options).await?;
    Ok(())
}

pub(crate) async fn install_with_progress(
    app: &AppHandle,
    downloader: &Arc<HttpDownloader>,
    meta: &VersionMeta,
    options: &VersionInstallOptions,
) -> Result<qcl_install::VersionInstallReport, String> {
    // 进度事件按 1% 粒度节流：5147 个资产对象逐个 emit 会把 IPC 淹掉，
    // 而界面上的进度条只需要 1% 的精度。
    let last = Arc::new(Mutex::new((None::<InstallPhase>, u32::MAX)));
    // 全部命中缓存时这次调用只持续几百毫秒（只是逐个 stat 一遍），进度条闪一下
    // 比不显示更烦人——尤其是「启动前补齐文件」这条路径，每次启动都会走。
    // 真有下载时会自然跨过这个窗口。
    let started = std::time::Instant::now();
    let handle = app.clone();
    let progress = move |InstallProgress { phase, progress }| {
        if started.elapsed() < std::time::Duration::from_millis(300) {
            return;
        }

        let percent = (progress.completed_files * 100)
            .checked_div(progress.total_files)
            .unwrap_or(100) as u32;

        {
            let mut last = match last.lock() {
                Ok(last) => last,
                // 锁中毒说明前一次回调 panic 了，此处不再放大问题。
                Err(_) => return,
            };
            if last.0 != Some(phase) {
                *last = (Some(phase), u32::MAX);
            }
            if last.1 == percent {
                return;
            }
            last.1 = percent;
        }

        let _ = handle.emit(
            INSTALL_PROGRESS_EVENT,
            InstallProgressEvent {
                phase: match phase {
                    InstallPhase::VersionFiles => "versionFiles",
                    InstallPhase::Assets => "assets",
                },
                completed: progress.completed_files,
                total: progress.total_files,
                percent,
            },
        );
    };

    install_version(
        Arc::clone(downloader),
        meta,
        &launch_context(),
        options,
        progress,
    )
    .await
    .map_err(|error| error.to_string())
}

/// 一个可选的加载器版本，供界面下拉选择。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderOption {
    /// 加载器版本号，如 `0.20.1`。
    pub version: String,
    /// 是否为稳定版。
    pub stable: bool,
}

/// 可选的加载器版本上限。
///
/// Fabric 对一个游戏版本会列出上百个 loader 版本，全塞进下拉框既难选也浪费 IPC。
/// 上游按新到旧返回，取前若干个足够覆盖实际需要。
const MAX_LOADER_OPTIONS: usize = 30;

/// 列出某个游戏版本可用的 Fabric 加载器版本。
#[tauri::command]
pub async fn list_loaders(
    state: tauri::State<'_, AppState>,
    version: String,
    loader_kind: Option<String>,
) -> Result<Vec<LoaderOption>, String> {
    let (downloader, _game_dir) = state.context()?;

    let loaders = match LoaderKind::parse(loader_kind.as_deref()) {
        LoaderKind::Fabric => fetch_loader_list(&downloader, &version)
            .await
            .map_err(|error| format!("获取 Fabric 列表失败：{error}"))?,
        LoaderKind::Forge(kind) => qcl_install::fetch_forge_versions(&downloader, kind, &version)
            .await
            .map_err(|error| format!("获取 {} 列表失败：{error}", kind.display()))?,
    };

    Ok(loaders
        .into_iter()
        .take(MAX_LOADER_OPTIONS)
        .map(|loader| LoaderOption {
            version: loader.version,
            stable: loader.stable,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{load_meta, meta_path, read_local_meta, write_local_meta};
    use qcl_core::version::VersionMeta;
    use qcl_net::HttpDownloader;
    use tempfile::TempDir;

    /// 造一份最小的合并结果：只要能区分 id 即可，字段内容不重要。
    fn merged_meta(id: &str) -> VersionMeta {
        let raw = r#"{
            "id": "__ID__",
            "type": "release",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "releaseTime": "2026-09-15T11:23:02+00:00",
            "time": "2026-09-15T11:23:02+00:00",
            "libraries": [],
            "arguments": { "game": [], "jvm": [] }
        }"#
        .replace("__ID__", id);

        serde_json::from_str(&raw).expect("夹具应可解析")
    }

    #[test]
    fn local_meta_roundtrip() {
        let dir = TempDir::new().expect("临时目录");
        let meta = merged_meta("fabric-loader-0.20.1-26.3");

        write_local_meta(dir.path(), &meta).expect("应能写入");

        let path = meta_path(dir.path(), "fabric-loader-0.20.1-26.3");
        assert!(path.is_file(), "应写到 versions/<id>/<id>.json");

        let read_back = read_local_meta(dir.path(), "fabric-loader-0.20.1-26.3").expect("应能读回");
        assert_eq!(read_back.id, meta.id);
        assert_eq!(read_back.main_class, meta.main_class);
    }

    #[test]
    fn corrupt_local_meta_is_ignored() {
        let dir = TempDir::new().expect("临时目录");
        let path = meta_path(dir.path(), "broken");
        std::fs::create_dir_all(path.parent().expect("应有父目录")).expect("应能建目录");
        std::fs::write(&path, b"{ not json").expect("应能写入");

        assert!(
            read_local_meta(dir.path(), "broken").is_none(),
            "损坏的文件应被忽略，让调用方回上游重新拉取"
        );
    }

    #[tokio::test]
    async fn load_meta_prefers_the_local_copy() {
        // 加载器版本不在官方清单里，只能从本地读。若这里改成先请求网络，
        // 装完 Fabric 之后启动就会因为「清单里找不到这个版本」而失败。
        let dir = TempDir::new().expect("临时目录");
        let meta = merged_meta("fabric-loader-0.20.1-26.3");
        write_local_meta(dir.path(), &meta).expect("应能写入");

        let downloader = HttpDownloader::new(dir.path()).expect("应能创建下载器");
        let loaded = load_meta(&downloader, "fabric-loader-0.20.1-26.3")
            .await
            .expect("应从本地读到");

        assert_eq!(loaded.id, "fabric-loader-0.20.1-26.3");
    }
}

#[cfg(test)]
mod live_tests {
    //! 对着真实上游跑一遍的测试。默认 `#[ignore]`：CI 不该依赖外部网络，
    //! 但排查「装了加载器却启动不了」这类问题时，先跑它们比猜快得多。
    //!
    //! ```bash
    //! cargo test -p qcl-desktop -- --ignored live_
    //! ```

    use super::{load_meta, load_meta_with_loader, read_local_meta};
    use qcl_net::HttpDownloader;
    use tempfile::TempDir;

    #[tokio::test]
    #[ignore = "需要访问 meta.fabricmc.net 与 Mojang 端点"]
    async fn live_fabric_merge_roundtrip() {
        let dir = TempDir::new().expect("临时目录");
        let downloader = HttpDownloader::new(dir.path()).expect("应能创建下载器");

        let merged = load_meta_with_loader(
            &downloader,
            "26.3",
            Some("0.20.1"),
            super::LoaderKind::Fabric,
            None,
            None,
            None,
        )
        .await
        .expect("应能合并出加载器版本");

        // 主类必须换成加载器入口，否则游戏起来还是原版。
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert!(
            merged.id.starts_with("fabric-loader-0.20.1-"),
            "id 为 {}",
            merged.id
        );

        // 父版本的关键信息必须还在，否则装不上或起不来。
        assert!(merged.asset_index.is_some(), "资产索引应继承自原版");
        assert!(
            merged
                .downloads
                .as_ref()
                .and_then(|d| d.client.as_ref())
                .is_some()
        );
        assert!(merged.required_java_major().is_some());

        // 加载器的库要并进来，而且 fabric-loader 自身必须有摘要与大小——
        // 它在上游 profile 里只有坐标，是我们回 maven 补齐的。
        let loader_lib = merged
            .libraries
            .iter()
            .find(|library| library.name.starts_with("net.fabricmc:fabric-loader:"))
            .expect("应含 fabric-loader");
        let artifact = loader_lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .expect("fabric-loader 应有主构件");
        assert_eq!(artifact.sha1.len(), 40, "摘要应是完整 SHA-1");
        assert!(artifact.size > 0, "大小应已探测");

        // 落盘后应能从本地读回——启动走的就是这条路径。
        let reloaded = load_meta(&downloader, &merged.id)
            .await
            .expect("应能从本地读回");
        assert_eq!(reloaded.id, merged.id);
        assert_eq!(reloaded.main_class, merged.main_class);
        assert_eq!(reloaded.libraries.len(), merged.libraries.len());
        assert!(
            read_local_meta(dir.path(), &merged.id).is_some(),
            "合并结果应已写入 versions/<id>/<id>.json"
        );
    }
}

#[cfg(test)]
mod offline_tests {
    use super::{load_meta_with_loader, write_local_meta};
    use qcl_core::version::VersionMeta;
    use qcl_net::HttpDownloader;
    use tempfile::TempDir;

    fn meta(id: &str, main_class: &str) -> VersionMeta {
        let raw = r#"{
            "id": "__ID__",
            "type": "release",
            "mainClass": "__MAIN__",
            "releaseTime": "2026-09-15T11:23:02+00:00",
            "time": "2026-09-15T11:23:02+00:00",
            "libraries": [],
            "arguments": { "game": [], "jvm": [] }
        }"#
        .replace("__ID__", id)
        .replace("__MAIN__", main_class);

        serde_json::from_str(&raw).expect("夹具应可解析")
    }

    #[tokio::test]
    async fn merged_meta_comes_from_disk_when_present() {
        // 装过一次之后合并结果就在 `versions/<id>/<id>.json`。启动不该再去上游
        // 取 profile 与库摘要——断网就等于拒绝启动，而文件其实早就齐了。
        let dir = TempDir::new().expect("临时目录");
        write_local_meta(dir.path(), &meta("26.3", "net.minecraft.client.main.Main"))
            .expect("写入原版");

        // 塞一个上游绝不可能产生的标记：返回它就证明走的是本地文件。
        write_local_meta(
            dir.path(),
            &meta("fabric-loader-0.20.1-26.3", "qcl.test.LocalMarker"),
        )
        .expect("写入合并结果");

        let downloader = HttpDownloader::new(dir.path()).expect("下载器");
        let loaded = load_meta_with_loader(
            &downloader,
            "26.3",
            Some("0.20.1"),
            super::LoaderKind::Fabric,
            None,
            None,
            None,
        )
        .await
        .expect("本地已有合并结果");

        assert_eq!(
            loaded.main_class, "qcl.test.LocalMarker",
            "不该用上游结果覆盖本地的合并结果"
        );
        assert_eq!(loaded.id, "fabric-loader-0.20.1-26.3");
    }
}
