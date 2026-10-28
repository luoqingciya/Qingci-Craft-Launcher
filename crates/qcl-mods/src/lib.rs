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

pub mod manifest;
pub mod zh_names;

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
    /// 图标地址。项目没上传图标时为 `None`。
    pub icon_url: Option<String>,
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
    /// 必需的依赖（Modrinth 项目 id）。
    ///
    /// 大多数 Fabric 模组依赖 Fabric API（项目 id `P7dR8mSH`），
    /// 只装模组本体、不装依赖，游戏会在加载阶段报「requires fabric-api」。
    pub dependencies: Vec<String>,
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
    #[serde(default)]
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    version_number: String,
    #[serde(default)]
    files: Vec<VersionFile>,
    #[serde(default)]
    dependencies: Vec<DependencyEntry>,
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

#[derive(Debug, Deserialize)]
struct DependencyEntry {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
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
    search_with_filters(downloader, query, game_version, loader, &[], limit).await
}

/// 带内容分类筛选的搜索。
///
/// `categories` 是 Modrinth 的分类 id（如 `optimization`、`technology`）；
/// 多个之间是「或」——用户同时点「优化」和「科技」时想看的是两类都算。
pub async fn search_with_filters(
    downloader: &HttpDownloader,
    query: &str,
    game_version: &str,
    loader: &str,
    categories: &[String],
    limit: usize,
) -> Result<Vec<ModSummary>, ModError> {
    search_with_filters_from(
        downloader,
        MODRINTH_API,
        query,
        game_version,
        loader,
        categories,
        limit,
    )
    .await
}

/// 同 [`search_with_filters`]，但可指定 API 根地址。
pub async fn search_with_filters_from(
    downloader: &HttpDownloader,
    api: &str,
    query: &str,
    game_version: &str,
    loader: &str,
    categories: &[String],
    limit: usize,
) -> Result<Vec<ModSummary>, ModError> {
    let url = format!(
        "{}/search?query={}&limit={}&facets={}",
        api.trim_end_matches('/'),
        encode_query_value(query),
        limit.clamp(1, 50),
        encode_query_value(&facets(game_version, loader, categories))
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
            icon_url: hit.icon_url,
        })
        .collect())
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
    search_with_filters_from(downloader, api, query, game_version, loader, &[], limit).await
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

        // 只取「必需」的依赖：可选依赖（如联动模组）装上去反而可能冲突。
        let dependencies = version
            .dependencies
            .iter()
            .filter(|dep| dep.dependency_type == "required")
            .filter_map(|dep| dep.project_id.clone())
            .collect();

        return Ok(ModFile {
            project_id: project_id.to_owned(),
            version_number: version.version_number,
            filename: file.filename.clone(),
            url: file.url.clone(),
            sha1: file.hashes.sha1.clone(),
            size: file.size,
            dependencies,
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

    let mods_dir = downloader.game_dir().join("mods");
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

    // 记下来源，更新检查才有依据。失败不影响安装本身——
    // 最坏情况是这个模组不参与更新检查。
    if let Err(error) = manifest::remember(&mods_dir, &safe, &file.project_id, &file.version_number)
    {
        eprintln!("记录模组来源失败：{error}");
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

/// 从 mods/ 里删除一个模组文件。
///
/// 只允许删 mods/ 下的 .jar：文件名来自界面，而界面上的文件名又来自远端，
/// 两道都当作不可信处理。删除前会把路径规范化，确保它确实落在 mods 目录内
/// （防止通过符号链接跳到别处）。
pub fn remove(game_dir: &std::path::Path, filename: &str) -> Result<(), ModError> {
    let safe = sanitize_filename(filename)?;
    if !safe.ends_with(".jar") {
        return Err(ModError::UnsafeFilename(filename.to_owned()));
    }

    let mods_dir = game_dir.join("mods");
    let target = mods_dir.join(&safe);

    // 用规范化路径比较，符号链接指向目录外时会被拦下。
    let Ok(root) = std::fs::canonicalize(&mods_dir) else {
        return Err(ModError::Download("mods 目录不存在".to_owned()));
    };
    let Ok(real) = std::fs::canonicalize(&target) else {
        return Err(ModError::Download(format!("{safe} 不存在")));
    };

    if !real.starts_with(&root) {
        return Err(ModError::UnsafeFilename(filename.to_owned()));
    }

    std::fs::remove_file(&real).map_err(|error| ModError::Download(error.to_string()))?;

    // 清单里的条目一起清掉，否则更新检查会对一个不存在的文件反复报「有新版本」。
    manifest::forget(&mods_dir, &safe)?;

    Ok(())
}

/// 优先取 primary 文件，没有就取第一个。
fn pick_primary(files: &[VersionFile]) -> Option<&VersionFile> {
    files
        .iter()
        .find(|file| file.primary)
        .or_else(|| files.first())
}

/// 构造 Modrinth 的 facets 参数：项目类型 + 游戏版本 + 加载器。
fn facets(game_version: &str, loader: &str, categories: &[String]) -> String {
    let mut groups = vec![
        "[\"project_type:mod\"]".to_owned(),
        format!("[\"versions:{game_version}\"]"),
    ];
    if !loader.is_empty() {
        groups.push(format!("[\"categories:{loader}\"]"));
    }
    if !categories.is_empty() {
        // 同一组内是「或」：选「优化 + 科技」应该两类都出，而不是取交集。
        let items: Vec<String> = categories
            .iter()
            .map(|category| format!("\"categories:{category}\""))
            .collect();
        groups.push(format!("[{}]", items.join(",")));
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

/// 递归安装一个模组及其必需依赖。
///
/// 为什么必须做：大多数 Fabric 模组依赖 **Fabric API**，只装模组本体的话
/// 游戏会在加载阶段直接报 `requires fabric-api`。依赖关系由 Modrinth 的
/// `dependencies` 字段给出（`dependency_type == "required"`），比自己猜可靠。
///
/// 边界：
/// - 深度上限 [`MAX_DEPENDENCY_DEPTH`]，防止上游数据出问题时无限递归
/// - 同一个项目只装一次（用访问集合去重，也顺带处理循环依赖）
/// - 已存在同名文件的跳过，因此重复点「安装」不会重复下载
pub async fn install_with_dependencies(
    downloader: Arc<HttpDownloader>,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<String>, ModError> {
    install_with_dependencies_from(downloader, MODRINTH_API, project_id, game_version, loader).await
}

/// 一个必需依赖的展示信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyInfo {
    /// Modrinth 项目 id。
    pub project_id: String,
    /// 项目标题；查不到时为 `None`——依赖本身仍然有效，只是没有名字可显示。
    pub title: Option<String>,
}

/// 解析一个模组的必需依赖，并尽量补上项目标题。
///
/// 只取 `dependency_type == "required"`：可选依赖（联动模组）装上去反而可能冲突，
/// 安装流程也是这么筛的，两边必须一致，否则界面说的和实际装的会对不上。
pub async fn dependencies(
    downloader: &HttpDownloader,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<DependencyInfo>, ModError> {
    dependencies_from(downloader, MODRINTH_API, project_id, game_version, loader).await
}

/// 同 [`dependencies`]，但可指定 API 根地址。
pub async fn dependencies_from(
    downloader: &HttpDownloader,
    api: &str,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<DependencyInfo>, ModError> {
    let file = resolve_file_from(downloader, api, project_id, game_version, loader).await?;
    if file.dependencies.is_empty() {
        return Ok(Vec::new());
    }

    // 标题查不到不该让整件事失败：用户至少要知道「有依赖、会被装」。
    let titles = project_titles_from(downloader, api, &file.dependencies)
        .await
        .unwrap_or_default();

    Ok(file
        .dependencies
        .into_iter()
        .map(|id| DependencyInfo {
            title: titles.get(&id).cloned(),
            project_id: id,
        })
        .collect())
}

/// 批量取项目标题。
///
/// 用 `/projects?ids=[...]` 一次拿完：逐个请求会让「看依赖」这种随手一点的操作
/// 变成好几次往返。
async fn project_titles_from(
    downloader: &HttpDownloader,
    api: &str,
    ids: &[String],
) -> Result<std::collections::HashMap<String, String>, ModError> {
    let list = serde_json::to_string(ids)?;
    let url = format!(
        "{}/projects?ids={}",
        api.trim_end_matches('/'),
        encode_query_value(&list)
    );

    let raw = downloader
        .fetch_text_with_user_agent(&url, USER_AGENT)
        .await
        .map_err(|error| ModError::Network(error.to_string()))?;

    #[derive(Deserialize)]
    struct Project {
        id: String,
        title: String,
    }

    let projects: Vec<Project> = serde_json::from_str(&raw)?;
    Ok(projects
        .into_iter()
        .map(|project| (project.id, project.title))
        .collect())
}

/// 依赖递归的最大深度。
///
/// 实测 Fabric 生态的依赖链通常只有 1 层（模组 → Fabric API），
/// 取 4 是留出余量同时兜住上游数据异常。
pub const MAX_DEPENDENCY_DEPTH: usize = 4;

/// 同 [`install_with_dependencies`]，但可指定 API 根地址。
pub async fn install_with_dependencies_from(
    downloader: Arc<HttpDownloader>,
    api: &str,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<String>, ModError> {
    let mut installed = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    let mut queue: Vec<(String, usize)> = vec![(project_id.to_owned(), 0)];

    while let Some((id, depth)) = queue.pop() {
        if visited.iter().any(|seen| seen == &id) {
            continue;
        }
        visited.push(id.clone());

        let file = resolve_file_from(&downloader, api, &id, game_version, loader).await?;

        // 同名文件已存在就跳过：重复安装同一模组不该再下载一次。
        let relative = format!("mods/{}", file.filename);
        if !downloader.game_dir().join(&relative).is_file() {
            installed.push(install(Arc::clone(&downloader), &file).await?);
        }

        if depth + 1 < MAX_DEPENDENCY_DEPTH {
            for dependency in &file.dependencies {
                queue.push((dependency.clone(), depth + 1));
            }
        }
    }

    Ok(installed)
}

/// 一个可更新的模组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModUpdate {
    /// 已安装的文件名。
    pub file_name: String,
    /// Modrinth 项目 id。
    pub project_id: String,
    /// 当前版本号。
    pub current_version: String,
    /// 最新版本号。
    pub latest_version: String,
    /// 最新版本的文件名。
    pub latest_file_name: String,
}

/// 检查已安装模组有没有新版本。
///
/// 判断依据是**文件名不同**：Modrinth 的每个版本文件名都带版本号，
/// 同名意味着同一个文件。这比解析版本号字符串可靠——各作者的命名格式差别很大。
///
/// 清单里没有的（用户手动丢进 `mods/` 的 jar）不参与检查：无法可靠地反推项目 id。
pub async fn check_updates(
    downloader: &HttpDownloader,
    game_version: &str,
    loader: &str,
) -> Result<Vec<ModUpdate>, ModError> {
    check_updates_from(downloader, MODRINTH_API, game_version, loader).await
}

/// 同 [`check_updates`]，但可指定 API 根地址。
pub async fn check_updates_from(
    downloader: &HttpDownloader,
    api: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<ModUpdate>, ModError> {
    let mods_dir = downloader.game_dir().join("mods");
    let manifest = manifest::prune(&mods_dir, manifest::load(&mods_dir));

    let mut updates = Vec::new();

    for (file_name, installed) in manifest {
        // 某个模组在新版本里没有适配文件时跳过，而不是让整次检查失败。
        let Ok(latest) =
            resolve_file_from(downloader, api, &installed.project_id, game_version, loader).await
        else {
            continue;
        };

        if latest.filename != file_name {
            updates.push(ModUpdate {
                file_name,
                project_id: installed.project_id,
                current_version: installed.version_number,
                latest_version: latest.version_number,
                latest_file_name: latest.filename,
            });
        }
    }

    Ok(updates)
}
