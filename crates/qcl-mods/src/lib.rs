// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 模组管理：从 Modrinth 搜索并安装到 `mods/`。
//!
//! 只做三件事，每件都可以单独测：
//!
//! 1. [`search`]：按游戏版本与加载器筛选项目
//! 2. [`resolve_file`]：把项目解析成「一个具体要下载的文件」
//! 3. [`install`]：下载并校验到 `mods/`
//!
//! 依赖解析（模组的依赖树）刻意没做：Modrinth 的依赖关系不保证完整，
//! 猜错了会让人装上一个启动即崩的组合。缺依赖时游戏自己的日志说得更清楚。

use qcl_net::{DownloadOps, DownloadOptions, DownloadPlan, DownloadTask, HttpDownloader};
use serde::Deserialize;
use std::sync::Arc;

/// Modrinth API 根地址。
pub const MODRINTH_API: &str = "https://api.modrinth.com/v2";

/// 请求时带的 User-Agent。
///
/// Modrinth 要求标明客户端身份，否则可能被限流；带上仓库地址也方便他们联系。
pub const USER_AGENT: &str =
    "luoqingci/QingciCraftLauncher/0.1.0 (github.com/luoqingciya/Qingci-Craft-Launcher)";

/// 一次搜索返回的项目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSummary {
    /// 项目 id（安装时用它）。
    pub project_id: String,
    /// 人类可读的短名，如 `sodium`。
    pub slug: String,
    /// 标题。
    pub title: String,
    /// 一句话描述。
    pub description: String,
    /// 下载量。
    pub downloads: u64,
    /// 作者。
    pub author: String,
}

/// 一个具体可下载的模组文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
    /// 所属项目 id。
    pub project_id: String,
    /// 版本号，如 `mc26.3-0.9.3-beta.1-fabric`。
    pub version_number: String,
    /// 文件名，如 `sodium-fabric-0.9.3-beta.1+mc26.3.jar`。
    pub filename: String,
    /// 下载地址。
    pub url: String,
    /// 官方给出的 SHA-1。
    pub sha1: String,
    /// 文件大小。
    pub size: u64,
}

/// 模组操作中的错误。
#[derive(Debug, thiserror::Error)]
pub enum ModError {
    /// 网络失败。
    #[error("网络错误：{0}")]
    Network(String),

    /// 响应无法解析。
    #[error("响应解析失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// 该游戏版本 + 加载器下没有可用文件。
    #[error("{0} 没有适配该游戏版本与加载器的文件")]
    NoCompatibleFile(String),

    /// 文件名不安全（含路径分隔符或 `..`）。
    #[error("文件名不安全：{0}")]
    UnsafeFilename(String),

    /// 下载或校验失败。
    #[error("下载失败：{0}")]
    Download(String),
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    hits: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    author: String,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    version_number: String,
    #[serde(default)]
    files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    hashes: Hashes,
}

#[derive(Debug, Default, Deserialize)]
struct Hashes {
    #[serde(default)]
    sha1: String,
}

/// 搜索模组。
///
/// `loader` 传 `fabric` / `quilt` / `forge` 等；传空串表示不按加载器筛选。
pub async fn search(
    downloader: &HttpDownloader,
    query: &str,
    game_version: &str,
    loader: &str,
    limit: usize,
) -> Result<Vec<ModSummary>, ModError> {
    search_from(downloader, MODRINTH_API, query, game_version, loader, limit).await
}

/// 同 [`search`]，但可指定 API 根地址。
pub async fn search_from(
    downloader: &HttpDownloader,
    api: &str,
    query: &str,
    game_version: &str,
    loader: &str,
    limit: usize,
) -> Result<Vec<ModSummary>, ModError> {
    let url = format!(
        "{}/search?query={}&limit={}&facets={}",
        api.trim_end_matches('/'),
        encode_query_value(query),
        limit.clamp(1, 50),
        encode_query_value(&facets(game_version, loader))
    );

    let raw = downloader
        .fetch_text_with_user_agent(&url, USER_AGENT)
        .await
        .map_err(|error| ModError::Network(error.to_string()))?;

    let parsed: SearchResponse = serde_json::from_str(&raw)?;

    Ok(parsed
        .hits
        .into_iter()
        .map(|hit| ModSummary {
            project_id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            description: hit.description,
            downloads: hit.downloads,
            author: hit.author,
        })
        .collect())
}

/// 把项目解析成适配该游戏版本与加载器的文件。
pub async fn resolve_file(
    downloader: &HttpDownloader,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<ModFile, ModError> {
    resolve_file_from(downloader, MODRINTH_API, project_id, game_version, loader).await
}

/// 同 [`resolve_file`]，但可指定 API 根地址。
pub async fn resolve_file_from(
    downloader: &HttpDownloader,
    api: &str,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<ModFile, ModError> {
    let url = format!(
        "{}/project/{}/version?game_versions={}&loaders={}",
        api.trim_end_matches('/'),
        encode_query_value(project_id),
        encode_query_value(&format!("[\"{game_version}\"]")),
        encode_query_value(&format!("[\"{loader}\"]"))
    );

    let raw = downloader
        .fetch_text_with_user_agent(&url, USER_AGENT)
        .await
        .map_err(|error| ModError::Network(error.to_string()))?;

    let versions: Vec<VersionEntry> = serde_json::from_str(&raw)?;

    // 上游按发布时间倒序返回，取第一个有文件的版本即最新。
    for version in versions {
        let Some(file) = pick_primary(&version.files) else {
            continue;
        };

        if file.hashes.sha1.len() != 40 {
            return Err(ModError::Download(format!(
                "{} 缺少有效的 SHA-1，无法校验",
                file.filename
            )));
        }

        return Ok(ModFile {
            project_id: project_id.to_owned(),
            version_number: version.version_number,
            filename: file.filename.clone(),
            url: file.url.clone(),
            sha1: file.hashes.sha1.clone(),
            size: file.size,
        });
    }

    Err(ModError::NoCompatibleFile(project_id.to_owned()))
}

/// 把文件下载到 `<游戏目录>/mods/`，并校验 SHA-1。
pub async fn install(downloader: Arc<HttpDownloader>, file: &ModFile) -> Result<String, ModError> {
    let safe = sanitize_filename(&file.filename)?;
    let path = format!("mods/{safe}");

    let task = DownloadTask {
        url: file.url.clone(),
        path: path.clone(),
        sha1: file.sha1.clone(),
        size: file.size,
    };

    let ops: Arc<dyn DownloadOps> = downloader;
    let report = qcl_net::execute_plan(
        &DownloadPlan { tasks: vec![task] },
        &DownloadOptions::default(),
        ops,
        |_| {},
    )
    .await;

    if let Some(failure) = report.failures.into_iter().next() {
        return Err(ModError::Download(failure.error.to_string()));
    }

    Ok(path)
}

/// 只保留文件名部分，并拒绝任何可能导致越界的写法。
///
/// 文件名来自远端，不能假设它规矩：`../../evil.jar` 这种一旦被接受就会写到游戏目录之外。
fn sanitize_filename(filename: &str) -> Result<String, ModError> {
    let trimmed = filename.trim();
    let unsafe_name = trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.starts_with('.');

    if unsafe_name {
        return Err(ModError::UnsafeFilename(filename.to_owned()));
    }

    Ok(trimmed.to_owned())
}

/// 优先取 `primary` 文件，没有就取第一个。
fn pick_primary(files: &[VersionFile]) -> Option<&VersionFile> {
    files
        .iter()
        .find(|file| file.primary)
        .or_else(|| files.first())
}

/// 构造 Modrinth 的 facets 参数：项目类型 + 游戏版本 + 加载器。
fn facets(game_version: &str, loader: &str) -> String {
    let mut groups = vec![
        "[\"project_type:mod\"]".to_owned(),
        format!("[\"versions:{game_version}\"]"),
    ];
    if !loader.is_empty() {
        groups.push(format!("[\"categories:{loader}\"]"));
    }
    format!("[{}]", groups.join(","))
}

/// 查询参数转义。
///
/// 只保留 RFC 3986 的 unreserved 字符与查询里合法的 `:`、`,`——
/// `[`、`]`、`"` 必须转义，否则 facets 里的 JSON 会破坏 URL。
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b',' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
