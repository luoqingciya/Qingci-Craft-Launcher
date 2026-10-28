// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 模组命令：把 `qcl-mods` 接到 IPC 上。
//!
//! 这里只做参数整理与错误转换；搜索、解析、下载校验都在 crate 里，
//! 因此那些逻辑仍然能在无 GUI 的测试中覆盖。

use qcl_mods::ModSummary;
use serde::Serialize;

use crate::AppState;

/// 搜索结果条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModHit {
    /// 项目 id，安装时回传。
    pub project_id: String,
    /// 标题。
    pub title: String,
    /// 一句话描述。
    pub description: String,
    /// 下载量。
    pub downloads: u64,
    /// 作者。
    pub author: String,
    /// 图标地址。
    pub icon_url: Option<String>,
    /// 项目短名，用于拼 Modrinth 页面地址。
    pub slug: String,
    /// 中文译名。译名库里没有就是 `None`，界面退回英文标题。
    pub zh_name: Option<String>,
}

impl From<ModSummary> for ModHit {
    fn from(summary: ModSummary) -> Self {
        Self {
            project_id: summary.project_id,
            title: summary.title,
            description: summary.description,
            downloads: summary.downloads,
            author: summary.author,
            icon_url: summary.icon_url,
            zh_name: qcl_mods::zh_names::zh_name(&summary.slug),
            slug: summary.slug,
        }
    }
}

/// 搜索结果：附带实际使用的查询词。
///
/// 用户输入中文（如「钠」）时会被换成英文关键词（`sodium`）再去请求，
/// 把实际用的词回传，界面才能如实说明「已按 sodium 搜索」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutcome {
    /// 实际发给 Modrinth 的查询词。
    pub query: String,
    /// 命中的项目。
    pub hits: Vec<ModHit>,
}

/// 搜索模组。
///
/// `loader` 传 `fabric` 这类分类名；空串表示不按加载器筛选。
/// `categories` 是内容分类（如 `optimization`），多个之间是「或」。
#[tauri::command]
pub async fn search_mods(
    state: tauri::State<'_, AppState>,
    query: String,
    version: String,
    loader: String,
    categories: Vec<String>,
) -> Result<SearchOutcome, String> {
    let (downloader, _game_dir) = state.context()?;

    // 中文输入先换成英文关键词：Modrinth 的索引只有英文。
    let resolved = qcl_mods::zh_names::resolve_query(&query);

    // 关键词和分类都为空才真的没得搜；只选分类是「按类别浏览」，必须放行
    // ——实测空查询 + 分类筛选能返回几百条结果，这是最不需要英文能力的入口。
    if resolved.is_empty() && categories.is_empty() {
        return Ok(SearchOutcome {
            query: resolved,
            hits: Vec::new(),
        });
    }

    let results =
        qcl_mods::search_with_filters(&downloader, &resolved, &version, &loader, &categories, 20)
            .await
            .map_err(|error| error.to_string())?;

    Ok(SearchOutcome {
        query: resolved,
        hits: results.into_iter().map(ModHit::from).collect(),
    })
}

/// 一个可筛选的内容分类。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryOption {
    /// Modrinth 的分类 id，如 `optimization`。
    pub id: String,
    /// 中文标签。
    pub label: String,
}

/// 列出可用于筛选的模组分类。
///
/// 分类是写死的 19 个（Modrinth 的固定标签），中文标签来自本地译名表；
/// 因此不需要联网。
#[tauri::command]
pub fn list_mod_categories() -> Vec<CategoryOption> {
    const IDS: [&str; 19] = [
        "optimization",
        "technology",
        "magic",
        "adventure",
        "decoration",
        "worldgen",
        "mobs",
        "equipment",
        "storage",
        "transportation",
        "food",
        "management",
        "utility",
        "game-mechanics",
        "economy",
        "minigame",
        "social",
        "library",
        "cursed",
    ];

    IDS.iter()
        .map(|id| CategoryOption {
            id: (*id).to_owned(),
            label: qcl_mods::zh_names::category_label(id).unwrap_or_else(|| (*id).to_owned()),
        })
        .collect()
}

/// 安装模组及其必需依赖，返回本次实际落地的路径列表。
///
/// 必须连依赖一起装：大多数 Fabric 模组依赖 Fabric API，只装本体游戏会在
/// 加载阶段报 `requires fabric-api`。
#[tauri::command]
pub async fn install_mod(
    state: tauri::State<'_, AppState>,
    project_id: String,
    version: String,
    loader: String,
) -> Result<Vec<String>, String> {
    let (downloader, _game_dir) = state.context()?;

    qcl_mods::install_with_dependencies(downloader, &project_id, &version, &loader)
        .await
        .map_err(|error| error.to_string())
}

/// 一个已安装的模组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModView {
    /// `mods/` 下的文件名。
    pub file_name: String,
    /// Modrinth 项目 id；手动放进 `mods/` 的 jar 为 null。
    pub project_id: Option<String>,
    /// 安装时的版本号；未知来源时为 null。
    pub version_number: Option<String>,
}

/// 列出已安装的模组。
///
/// 带来源信息：界面靠 `projectId` 判断搜索结果「装没装过」。
/// 手动放进 `mods/` 的 jar 也会列出来，只是没有来源。
#[tauri::command]
pub fn list_mods(state: tauri::State<'_, AppState>) -> Result<Vec<InstalledModView>, String> {
    let (_, game_dir) = state.context()?;

    Ok(qcl_mods::manifest::list_installed(&game_dir.join("mods"))
        .into_iter()
        .map(|entry| InstalledModView {
            file_name: entry.file_name,
            project_id: entry.project_id,
            version_number: entry.version_number,
        })
        .collect())
}

/// 一个必需依赖。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyView {
    /// Modrinth 项目 id。
    pub project_id: String,
    /// 项目标题；查不到时为 null。
    pub title: Option<String>,
    /// 是否已经装上了。
    pub installed: bool,
}

/// 查一个模组的必需依赖。
///
/// 界面上是「点开才查」：每个项目要一次 version 请求，塞进搜索会拖慢列表。
#[tauri::command]
pub async fn mod_dependencies(
    state: tauri::State<'_, AppState>,
    project_id: String,
    version: String,
    loader: String,
) -> Result<Vec<DependencyView>, String> {
    let (downloader, game_dir) = state.context()?;

    let dependencies = qcl_mods::dependencies(&downloader, &project_id, &version, &loader)
        .await
        .map_err(|error| error.to_string())?;

    let installed: std::collections::HashSet<String> =
        qcl_mods::manifest::list_installed(&game_dir.join("mods"))
            .into_iter()
            .filter_map(|entry| entry.project_id)
            .collect();

    Ok(dependencies
        .into_iter()
        .map(|item| DependencyView {
            installed: installed.contains(&item.project_id),
            project_id: item.project_id,
            title: item.title,
        })
        .collect())
}

/// 删除一个已安装的模组。
///
/// 只删 mods/ 下的 .jar，并且要求文件名通过安全校验——
/// 界面传来的名字同样不可信。
#[tauri::command]
pub fn remove_mod(state: tauri::State<'_, AppState>, filename: String) -> Result<(), String> {
    let (_, game_dir) = state.context()?;
    qcl_mods::remove(&game_dir, &filename).map_err(|error| error.to_string())
}

/// 一个可更新的模组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdateDto {
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

/// 检查已安装模组是否有新版本。
/// 一个与当前版本/加载器不兼容的已装模组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompatibleModDto {
    /// 已安装的文件名。
    pub file_name: String,
    /// Modrinth 项目 id。
    pub project_id: String,
}

/// 一次更新检查的结果。
///
/// 两半对用户的意义不同：`updates` 点一下就解决，`incompatible` 得自己换版本或换模组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReportDto {
    /// 有新版本的模组。
    pub updates: Vec<ModUpdateDto>,
    /// 当前版本/加载器下没有可用文件的模组。
    pub incompatible: Vec<IncompatibleModDto>,
}

#[tauri::command]
pub async fn check_mod_updates(
    state: tauri::State<'_, AppState>,
    version: String,
    loader: String,
) -> Result<UpdateReportDto, String> {
    let (downloader, _game_dir) = state.context()?;

    let report = qcl_mods::check_updates(&downloader, &version, &loader)
        .await
        .map_err(|error| error.to_string())?;

    Ok(UpdateReportDto {
        updates: report
            .updates
            .into_iter()
            .map(|update| ModUpdateDto {
                file_name: update.file_name,
                project_id: update.project_id,
                current_version: update.current_version,
                latest_version: update.latest_version,
                latest_file_name: update.latest_file_name,
            })
            .collect(),
        incompatible: report
            .incompatible
            .into_iter()
            .map(|item| IncompatibleModDto {
                file_name: item.file_name,
                project_id: item.project_id,
            })
            .collect(),
    })
}

/// 更新一个模组：装最新版本，再删掉旧文件。
///
/// 顺序不能反：先删后装的话，装失败就会丢掉一个能用的模组。
#[tauri::command]
pub async fn update_mod(
    state: tauri::State<'_, AppState>,
    project_id: String,
    old_file: String,
    version: String,
    loader: String,
) -> Result<Vec<String>, String> {
    let (downloader, game_dir) = state.context()?;

    let installed = qcl_mods::install_with_dependencies(downloader, &project_id, &version, &loader)
        .await
        .map_err(|error| error.to_string())?;

    // 新版本换了文件名时，旧文件要删掉，否则游戏会同时加载两个版本。
    let replaced = installed
        .iter()
        .any(|path| path == &format!("mods/{old_file}"));
    if !replaced {
        qcl_mods::remove(&game_dir, &old_file).map_err(|error| error.to_string())?;
    }

    Ok(installed)
}
