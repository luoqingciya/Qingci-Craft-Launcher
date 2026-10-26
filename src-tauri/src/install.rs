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
pub(crate) async fn load_meta_with_loader(
    downloader: &HttpDownloader,
    version: &str,
    loader: Option<&str>,
) -> Result<VersionMeta, String> {
    let parent = load_meta(downloader, version).await?;

    let Some(loader_version) = loader else {
        return Ok(parent);
    };

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
) -> Result<InstallSummary, String> {
    let (downloader, _game_dir) = state.context()?;

    let loader = loader.filter(|value| !value.trim().is_empty());
    let meta = load_meta_with_loader(&downloader, &version, loader.as_deref()).await?;
    let options = VersionInstallOptions::for_version(&meta.id);

    // 进度事件按 1% 粒度节流：5147 个资产对象逐个 emit 会把 IPC 淹掉，
    // 而界面上的进度条只需要 1% 的精度。
    let last = Arc::new(Mutex::new((None::<InstallPhase>, u32::MAX)));
    let handle = app.clone();
    let progress = move |InstallProgress { phase, progress }| {
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

    let report = install_version(
        Arc::clone(&downloader),
        &meta,
        &launch_context(),
        &options,
        progress,
    )
    .await
    .map_err(|error| error.to_string())?;

    Ok(InstallSummary {
        version: meta.id,
        files: report.planned_files,
        downloaded: report.downloaded,
        skipped: report.skipped,
        assets: report.assets.objects,
        natives: report.natives.extracted,
    })
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
) -> Result<Vec<LoaderOption>, String> {
    let (downloader, _game_dir) = state.context()?;

    let loaders = fetch_loader_list(&downloader, &version)
        .await
        .map_err(|error| format!("获取加载器列表失败：{error}"))?;

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

        let merged = load_meta_with_loader(&downloader, "26.3", Some("0.20.1"))
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
