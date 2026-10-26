// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 模组命令：把 `qcl-mods` 接到 IPC 上。
//!
//! 这里只做参数整理与错误转换；搜索、解析、下载校验都在 crate 里，
//! 因此那些逻辑仍然能在无 GUI 的测试中覆盖。

use std::sync::Arc;

use qcl_mods::{ModFile, ModSummary};
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
}

impl From<ModSummary> for ModHit {
    fn from(summary: ModSummary) -> Self {
        Self {
            project_id: summary.project_id,
            title: summary.title,
            description: summary.description,
            downloads: summary.downloads,
            author: summary.author,
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

/// 安装模组，返回落地路径（相对游戏目录）。
#[tauri::command]
pub async fn install_mod(
    state: tauri::State<'_, AppState>,
    project_id: String,
    version: String,
    loader: String,
) -> Result<String, String> {
    let (downloader, _game_dir) = state.context()?;

    let file: ModFile = qcl_mods::resolve_file(&downloader, &project_id, &version, &loader)
        .await
        .map_err(|error| error.to_string())?;

    qcl_mods::install(Arc::clone(&downloader), &file)
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
