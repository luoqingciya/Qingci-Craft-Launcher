// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 安装命令：把 `qcl-install` 的安装流程接到 IPC 上。
//!
//! 这里只做三件事：取上下文、转发进度、把结果整理成前端能直接显示的形状。
//! 真正的安装顺序与失败归类都在 `qcl_install` 里，因此那部分逻辑仍可在无 GUI
//! 的测试中验证。

use std::sync::{Arc, Mutex};

use qcl_core::version::VersionMeta;
use qcl_install::{
    InstallPhase, InstallProgress, VersionInstallOptions, fetch_manifest, fetch_meta,
    install_version, resolve_version,
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
pub(crate) async fn load_meta(
    downloader: &HttpDownloader,
    version: &str,
) -> Result<VersionMeta, String> {
    let manifest = fetch_manifest(downloader)
        .await
        .map_err(|error| error.to_string())?;
    let entry = resolve_version(&manifest, version).map_err(|error| error.to_string())?;
    fetch_meta(downloader, entry)
        .await
        .map_err(|error| error.to_string())
}

/// 安装指定版本。
///
/// 幂等：已就绪的文件会被跳过，因此「启动前先调一次」也能起到修复作用。
#[tauri::command]
pub async fn install_game_version(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    version: String,
) -> Result<InstallSummary, String> {
    let (downloader, _game_dir) = state.context()?;

    let meta = load_meta(&downloader, &version).await?;
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
