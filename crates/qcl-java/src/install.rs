// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 按官方清单安装 Java 运行时。
//!
//! 流程：查索引 → 拉文件清单（校验 SHA-1）→ 生成计划 → 复用下载执行器落盘 →
//! 置可执行位 → 交给探测逻辑确认可用。
//!
//! 这是第一个把 `qcl-java` 与 `qcl-net` 串起来的模块：清单解析与版本选择留在本 crate，
//! 网络与落盘完全委托给下载层，因此安装过程仍然复用同一套重试与进度机制。

use std::sync::Arc;

use qcl_net::{
    DownloadError, DownloadOps, DownloadOptions, DownloadPlan, DownloadTask, HttpDownloader,
    Progress, execute_plan, verify_sha1,
};

use crate::detect::{JavaInstallation, JavaSource, inspect};
use crate::runtime::{
    RuntimeIndex, RuntimeManifest, plan_runtime_files, runtime_home, select_release,
};

/// 安装过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// 官方未为该组件与平台组合提供运行时。
    ///
    /// 32 位 x86 就是典型情形：组件键存在但条目为空。
    #[error("官方未提供 `{component}` 在平台 `{platform}` 上的运行时，请手动安装 Java")]
    Unavailable {
        /// 组件名。
        component: String,
        /// 平台键。
        platform: String,
    },

    /// 清单内容的 SHA-1 与索引声明不符。
    #[error("运行时清单校验失败（可能被截断或篡改）")]
    ManifestChecksum,

    /// 清单 JSON 解析失败。
    #[error("运行时清单解析失败：{0}")]
    ManifestParse(#[from] serde_json::Error),

    /// 网络或落盘失败。
    #[error("安装失败：{0}")]
    Download(#[from] DownloadError),

    /// 安装完成后仍未探测到可用的 Java。
    #[error("安装完成但未探测到可用的 Java：{0}")]
    NotDetected(String),

    /// 设置可执行权限失败。
    #[error("设置可执行权限失败：{0}")]
    Permission(String),
}

/// 安装指定组件与平台的 Java 运行时。
///
/// 返回安装后探测到的 [`JavaInstallation`]，调用方可直接用于组装启动命令。
/// `on_progress` 会收到逐文件的下载进度。
pub async fn install_runtime<F>(
    downloader: Arc<HttpDownloader>,
    index: &RuntimeIndex,
    component: &str,
    platform: &str,
    on_progress: F,
) -> Result<JavaInstallation, InstallError>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    let release =
        select_release(index, component, platform).ok_or_else(|| InstallError::Unavailable {
            component: component.to_owned(),
            platform: platform.to_owned(),
        })?;

    // 1. 拉清单并校验。清单本身也有 SHA-1，校验成本极低，
    //    却能挡住代理注入与截断——它是后续所有文件的可信来源。
    let raw = downloader.fetch_text(&release.manifest.url).await?;
    if !verify_sha1(&release.manifest.sha1, raw.as_bytes()) {
        return Err(InstallError::ManifestChecksum);
    }

    // 2. 解析并生成计划。
    let manifest: RuntimeManifest = serde_json::from_str(&raw)?;
    let plan = plan_runtime_files(&manifest, component, platform);

    let mut tasks = Vec::with_capacity(plan.len());
    for file in &plan.files {
        tasks.push(DownloadTask {
            url: file.url.clone(),
            path: file.path.clone(),
            sha1: file.sha1.clone(),
            size: file.size,
        });
    }
    let download_plan = DownloadPlan { tasks };

    // 3. 交给下载执行器。
    //    先取具体类型再在绑定处做 unsize——直接对 &Arc<HttpDownloader> 调 Arc::clone
    //    会被推断成 Arc<dyn DownloadOps>::clone，类型对不上。
    let concrete: Arc<HttpDownloader> = Arc::clone(&downloader);
    let ops: Arc<dyn DownloadOps> = concrete;
    let report = execute_plan(
        &download_plan,
        &DownloadOptions::default(),
        ops,
        on_progress,
    )
    .await;
    if let Some(failure) = report.failures.into_iter().next() {
        return Err(InstallError::Download(failure.error));
    }

    // 4. 置可执行位。类 Unix 上 bin/java 没有 +x 就完全跑不起来。
    for file in plan.files.iter().filter(|file| file.executable) {
        let absolute = downloader.game_dir().join(&file.path);
        set_executable(&absolute)?;
    }

    // 5. 用探测逻辑确认结果，而不是假定安装成功。
    let home = downloader
        .game_dir()
        .join(runtime_home(component, platform));
    inspect(&home, JavaSource::Managed)
        .ok_or_else(|| InstallError::NotDetected(home.display().to_string()))
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::metadata(path)
        .map_err(|error| InstallError::Permission(format!("{}: {error}", path.display())))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| InstallError::Permission(format!("{}: {error}", path.display())))
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> Result<(), InstallError> {
    // Windows 没有可执行位概念，扩展名决定一切。
    Ok(())
}
