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
            zh_name: qcl_mods::zh_names::zh_name(&summary.slug).map(str::to_owned),
            slug: summary.slug,
        }
    }
}

/// 搜索模组。
///
/// `loader` 传 `fabric` 这类分类名；空串表示不按加载器筛选（原版也能搜到通用模组）。
#[tauri::command]
pub async fn search_mods(
    state: tauri::State<'_, AppState>,
    query: String,
    version: String,
    loader: String,
) -> Result<Vec<ModHit>, String> {
    let (downloader, _game_dir) = state.context()?;

    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let results = qcl_mods::search(&downloader, trimmed, &version, &loader, 20)
        .await
        .map_err(|error| error.to_string())?;

    Ok(results.into_iter().map(ModHit::from).collect())
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

/// 列出已安装的模组文件名。
#[tauri::command]
pub fn list_mods(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let (_, game_dir) = state.context()?;
    let mods_dir = game_dir.join("mods");

    let Ok(entries) = std::fs::read_dir(&mods_dir) else {
        // 目录不存在就是「还没装过模组」，不是错误。
        return Ok(Vec::new());
    };

    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".jar"))
        .collect();

    names.sort();
    Ok(names)
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
