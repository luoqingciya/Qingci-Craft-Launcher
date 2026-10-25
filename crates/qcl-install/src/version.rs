// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 完整版本安装：一次调用装完某个版本所需的全部文件。
//!
//! 顺序是**刻意的**：
//!
//! 1. 版本文件（客户端 jar、日志配置、依赖库、natives 归档）
//! 2. natives 解压（依赖第 1 步下载的归档）
//! 3. 资产（需要先拉索引才能展开对象）
//!
//! 版本文件放最前有两个原因：它是启动的硬前提（缺 jar 一定起不来），而且体量最小
//! （26.3 约 100 MB），失败得最早、最省时间。资产占首次启动下载量的 80% 以上，
//! 放在最后可以让「版本不可用」这类问题在浪费流量之前就暴露。

use std::sync::Arc;

use qcl_core::version::{LaunchContext, VersionMeta};
use qcl_net::{
    DownloadError, DownloadOps, DownloadOptions, HttpDownloader, Progress, execute_plan,
    plan_version_files,
};

use crate::assets::{AssetError, AssetInstallOptions, AssetInstallReport, install_assets};
use crate::natives::{NativeError, NativesReport, install_natives};

/// 安装阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    /// 客户端 jar、日志配置、依赖库与 natives 归档。
    VersionFiles,
    /// 资产对象。
    Assets,
}

/// 带阶段的进度。
///
/// 两阶段的计数互不相干（版本文件是 76 个、资产是 5147 个），直接复用同一个
/// [`Progress`] 会让界面上的进度条突然回退，因此必须标明阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallProgress {
    /// 当前处于哪个阶段。
    pub phase: InstallPhase,
    /// 该阶段的进度。
    pub progress: Progress,
}

/// 安装参数。
#[derive(Debug, Clone)]
pub struct VersionInstallOptions {
    /// 版本目录（相对游戏目录），如 `versions/26.3`。
    pub version_dir: String,
    /// 资产安装参数。
    pub assets: AssetInstallOptions,
}

impl VersionInstallOptions {
    /// 按版本号构造，版本目录取官方的 `versions/<id>`。
    #[must_use]
    pub fn for_version(id: &str) -> Self {
        Self {
            version_dir: format!("versions/{id}"),
            assets: AssetInstallOptions::default(),
        }
    }
}

/// 安装结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInstallReport {
    /// 版本文件计划中的任务数。
    pub planned_files: usize,
    /// 版本文件实际下载数。
    pub downloaded: usize,
    /// 版本文件跳过数（已存在且校验通过）。
    pub skipped: usize,
    /// natives 解压结果。
    pub natives: NativesReport,
    /// 资产安装结果。
    pub assets: AssetInstallReport,
}

/// 安装过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// 版本文件下载失败。
    #[error("版本文件下载失败：{0}")]
    Files(#[from] DownloadError),

    /// natives 解压失败。
    #[error("natives 安装失败：{0}")]
    Natives(#[from] NativeError),

    /// 资产安装失败。
    #[error("资产安装失败：{0}")]
    Assets(#[from] AssetError),
}

/// 安装某个版本的全部文件。
///
/// 幂等：已存在且校验通过的文件会被跳过，因此可以每次启动前都调用它做「修复/补齐」。
pub async fn install_version<F>(
    downloader: Arc<HttpDownloader>,
    meta: &VersionMeta,
    context: &LaunchContext,
    options: &VersionInstallOptions,
    on_progress: F,
) -> Result<VersionInstallReport, VersionError>
where
    F: Fn(InstallProgress) + Send + Sync + 'static,
{
    // 1. 版本文件。
    let plan = plan_version_files(meta, context, &options.version_dir);

    let concrete: Arc<HttpDownloader> = Arc::clone(&downloader);
    let ops: Arc<dyn DownloadOps> = concrete;

    // 两个阶段各自需要 `'static` 的回调，用 Arc 共享同一个 Fn：
    // 让闭包借用外部的 on_progress 过不了 await。
    let on_progress = Arc::new(on_progress);
    let files_progress = Arc::clone(&on_progress);

    let files = execute_plan(&plan, &DownloadOptions::default(), ops, move |progress| {
        files_progress(InstallProgress {
            phase: InstallPhase::VersionFiles,
            progress,
        });
    })
    .await;

    let downloaded = files.downloaded;
    let skipped = files.skipped;
    if let Some(failure) = files.failures.into_iter().next() {
        return Err(VersionError::Files(failure.error));
    }

    // 2. natives 解压（归档已在第 1 步落盘）。
    let natives = install_natives(downloader.game_dir(), meta, context, &options.version_dir)?;

    // 3. 资产。
    let assets_progress = Arc::clone(&on_progress);
    let assets = install_assets(downloader, meta, &options.assets, move |progress| {
        assets_progress(InstallProgress {
            phase: InstallPhase::Assets,
            progress,
        });
    })
    .await?;

    Ok(VersionInstallReport {
        planned_files: plan.len(),
        downloaded,
        skipped,
        natives,
        assets,
    })
}
