// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Fabric 加载器：拉取 profile、补齐构件信息、与原版合并。
//!
//! Fabric 的 `profile/json` 不是完整的 `version.json`，而是一个**增量**描述：
//!
//! | 字段 | 含义 |
//! |---|---|
//! | `inheritsFrom` | 父版本（如 `26.3`），其余信息都从它继承 |
//! | `mainClass` | 换成 `KnotClient`，这是加载器真正生效的地方 |
//! | `libraries` | 只有加载器自己的依赖，且**扁平结构**（name/url/sha1/size） |
//! | `arguments` | 只追加 JVM 参数，游戏参数为空 |
//!
//! 因此合并必须做三件事：拼接库列表、替换主类、合并参数。合并后仍然是一个
//! 完整的 [`VersionMeta`]，后续的安装与启动流程不需要知道「这是带加载器的版本」。

use qcl_core::launch::maven_to_path;
use qcl_core::version::{Arguments, Artifact, Library, LibraryDownloads, VersionMeta, VersionType};
use qcl_net::{DownloadError, HttpDownloader};
use serde::Deserialize;

/// Fabric 元数据服务根地址。
pub const FABRIC_META_BASE: &str = "https://meta.fabricmc.net";

/// Fabric Maven 根地址。库条目未给 `url` 时的兜底。
pub const FABRIC_MAVEN_BASE: &str = "https://maven.fabricmc.net/";

/// 加载器相关错误。
#[derive(Debug, thiserror::Error)]
pub enum LoaderError {
    /// 网络失败。
    #[error("获取失败：{0}")]
    Download(#[from] DownloadError),

    /// JSON 解析失败。
    #[error("解析失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// Maven 坐标无法解析成路径。
    #[error("无法解析依赖坐标 `{0}`")]
    InvalidCoordinate(String),

    /// 构件信息缺失且无法补齐。
    #[error("构件信息不完整：{0}")]
    IncompleteArtifact(String),
}

/// 一个加载器版本。
#[derive(Debug, Clone, Deserialize)]
pub struct LoaderInfo {
    /// 版本号，如 `0.20.1`。
    pub version: String,
    /// 是否为稳定版。
    #[serde(default)]
    pub stable: bool,
}

/// `/v2/versions/loader/<game>` 的单个条目。只取需要的字段，
/// 其余（`intermediary`、`launcherMeta`）不解析——它们体积大且与合并无关。
#[derive(Debug, Clone, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

/// Fabric 的增量 profile。
#[derive(Debug, Clone, Deserialize)]
pub struct FabricProfile {
    /// 合并后应使用的版本 id，形如 `fabric-loader-0.20.1-26.3`。
    pub id: String,
    /// 父版本号。
    #[serde(rename = "inheritsFrom")]
    pub inherits_from: String,
    /// 加载器主类。
    #[serde(rename = "mainClass")]
    pub main_class: String,
    /// 发布时间。
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    /// 更新时间。
    pub time: String,
    /// 版本类型。
    #[serde(rename = "type")]
    pub version_type: VersionType,
    /// 加载器自身的依赖，扁平结构。
    #[serde(default)]
    pub libraries: Vec<FabricLibrary>,
    /// 追加的启动参数。
    #[serde(default)]
    pub arguments: Option<Arguments>,
}

/// Fabric profile 里的依赖条目。
///
/// 与官方 `version.json` 的 `downloads.artifact` 不同，这里是**扁平**的，
/// 而且 `fabric-loader` 自身只有 `name` 与 `url`——没有 sha1 与大小。
#[derive(Debug, Clone, Deserialize)]
pub struct FabricLibrary {
    /// Maven 坐标。
    pub name: String,
    /// Maven 仓库根地址。
    #[serde(default)]
    pub url: Option<String>,
    /// 摘要。
    #[serde(default)]
    pub sha1: Option<String>,
    /// 大小。
    #[serde(default)]
    pub size: Option<u64>,
}

/// 拉取某游戏版本可用的加载器列表。
pub async fn fetch_loader_list(
    downloader: &HttpDownloader,
    game_version: &str,
) -> Result<Vec<LoaderInfo>, LoaderError> {
    fetch_loader_list_from(downloader, FABRIC_META_BASE, game_version).await
}

/// 同 [`fetch_loader_list`]，但可指定元数据服务地址。
///
/// 地址若是常量，测试就只能访问公网——那种「测试」要么不稳定，要么根本没在跑。
pub async fn fetch_loader_list_from(
    downloader: &HttpDownloader,
    base: &str,
    game_version: &str,
) -> Result<Vec<LoaderInfo>, LoaderError> {
    let url = format!(
        "{}/v2/versions/loader/{game_version}",
        base.trim_end_matches('/')
    );
    let raw = downloader.fetch_text(&url).await?;
    let entries: Vec<LoaderEntry> = serde_json::from_str(&raw)?;
    Ok(entries.into_iter().map(|entry| entry.loader).collect())
}

/// 拉取指定加载器版本的 profile。
pub async fn fetch_profile(
    downloader: &HttpDownloader,
    game_version: &str,
    loader_version: &str,
) -> Result<FabricProfile, LoaderError> {
    fetch_profile_from(downloader, FABRIC_META_BASE, game_version, loader_version).await
}

/// 同 [`fetch_profile`]，但可指定元数据服务地址。
pub async fn fetch_profile_from(
    downloader: &HttpDownloader,
    base: &str,
    game_version: &str,
    loader_version: &str,
) -> Result<FabricProfile, LoaderError> {
    let url = format!(
        "{}/v2/versions/loader/{game_version}/{loader_version}/profile/json",
        base.trim_end_matches('/')
    );
    let raw = downloader.fetch_text(&url).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// 把 Fabric 的扁平库条目转成官方结构，并补齐缺失的摘要与大小。
///
/// `fabric-loader` 自身在 profile 里只有坐标，需要回 maven 补 `.sha1` 与
/// `Content-Length`。不补的话，摘要为空会让下载计划直接丢弃这个构件——
/// 表现为「装完了但启动时报找不到加载器类」。
pub async fn resolve_libraries(
    downloader: &HttpDownloader,
    profile: &FabricProfile,
) -> Result<Vec<Library>, LoaderError> {
    let mut libraries = Vec::with_capacity(profile.libraries.len());

    for entry in &profile.libraries {
        let path = maven_to_path(&entry.name)
            .ok_or_else(|| LoaderError::InvalidCoordinate(entry.name.clone()))?;

        let base = entry.url.as_deref().unwrap_or(FABRIC_MAVEN_BASE);
        let base = base.trim_end_matches('/');
        let url = format!("{base}/{path}");

        let sha1 = match &entry.sha1 {
            Some(sha1) if !sha1.is_empty() => sha1.clone(),
            // maven 仓库为每个构件提供 `.sha1` 纯文本。
            _ => {
                let raw = downloader.fetch_text(&format!("{url}.sha1")).await?;
                let digest = raw.split_whitespace().next().unwrap_or_default();
                if digest.len() != 40 {
                    return Err(LoaderError::IncompleteArtifact(format!(
                        "{} 的 .sha1 内容异常",
                        entry.name
                    )));
                }
                digest.to_owned()
            }
        };

        let size = match entry.size {
            Some(size) => size,
            None => downloader.fetch_size(&url).await?,
        };

        libraries.push(Library {
            name: entry.name.clone(),
            downloads: Some(LibraryDownloads {
                artifact: Some(Artifact {
                    id: None,
                    path: Some(path),
                    sha1,
                    size,
                    url,
                }),
                classifiers: None,
            }),
            rules: None,
            natives: None,
            extract: None,
        });
    }

    Ok(libraries)
}

/// 把加载器 profile 合并进原版元数据。
///
/// 合并后的 `id` 用加载器 profile 的 id，因此版本目录、客户端 jar 与 classpath
/// 都自然落在 `versions/<loader-id>/` 下——同一个原版版本可以并存多个加载器版本，
/// 互不覆盖。代价是每个组合各存一份客户端 jar（约 40 MB），这是可接受的。
#[must_use]
pub fn merge(
    parent: &VersionMeta,
    profile: &FabricProfile,
    libraries: Vec<Library>,
) -> VersionMeta {
    let mut merged = parent.clone();

    merged.id = profile.id.clone();
    merged.main_class = profile.main_class.clone();
    merged.version_type = profile.version_type.clone();
    merged.release_time = profile.release_time.clone();
    merged.time = profile.time.clone();

    // 父版本的库在前、加载器的库在后：与官方启动器一致，也让 classpath 顺序可预测。
    merged.libraries = parent.libraries.iter().cloned().chain(libraries).collect();

    // 参数同样是「父在前、加载器追加在后」。JVM 参数里 Fabric 只加一条
    // `-DFabricMcEmu`，顺序对它没有影响；但游戏参数必须保持原版顺序。
    let mut arguments = merged.arguments.clone().unwrap_or(Arguments {
        game: Vec::new(),
        jvm: Vec::new(),
        default_user_jvm: None,
    });
    if let Some(extra) = &profile.arguments {
        arguments.game.extend(extra.game.iter().cloned());
        arguments.jvm.extend(extra.jvm.iter().cloned());
    }
    merged.arguments = Some(arguments);

    merged
}

/// 判断某个 profile 是否声称继承自给定版本。
///
/// 加载器元数据服务理论上只会返回匹配的组合，但拿到不匹配的 profile 时
/// 合并出来的版本会缺库或主类错误，因此在合并前显式校验一次。
#[must_use]
pub fn inherits_from(profile: &FabricProfile, game_version: &str) -> bool {
    profile.inherits_from == game_version
}
