// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 版本清单与版本元数据的获取。
//!
//! 这是所有安装流程的第一步，CLI 与桌面端都需要，因此放在编排层而不是各自的
//! 入口里——否则两处会各写一份「拉清单、找版本、校验 SHA-1」，而其中任何一处
//! 漏掉校验都不会被立刻发现。

use std::path::{Path, PathBuf};

use qcl_core::version::{VersionEntry, VersionManifest, VersionMeta};
use qcl_net::{DownloadError, HttpDownloader, verify_sha1};

/// 官方版本清单地址。
pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 获取过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// 网络失败。
    #[error("获取失败：{0}")]
    Download(#[from] DownloadError),

    /// JSON 解析失败。
    #[error("解析失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// 版本元数据的 SHA-1 与清单声明不符。
    ///
    /// 元数据决定了要下载哪些文件、用什么参数启动，被篡改的后果比单个文件
    /// 损坏严重得多，因此宁可在这里失败。
    #[error("版本元数据校验失败（可能被截断或篡改）：{0}")]
    Checksum(String),

    /// 清单里没有这个版本。
    #[error("版本清单中不存在 `{0}`")]
    NotFound(String),
}

/// 拉取官方版本清单。
pub async fn fetch_manifest(downloader: &HttpDownloader) -> Result<VersionManifest, ManifestError> {
    fetch_manifest_from(downloader, VERSION_MANIFEST_URL).await
}

/// 同 [`fetch_manifest`]，但可指定地址。
///
/// 地址若是常量，测试就只能去访问公网——那种「测试」要么不稳定，要么根本没在跑。
pub async fn fetch_manifest_from(
    downloader: &HttpDownloader,
    url: &str,
) -> Result<VersionManifest, ManifestError> {
    let raw = downloader.fetch_text(url).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// 在清单里定位版本。
///
/// `requested` 为 `latest` 时取 `latest.release` 指针指向的版本，而不是列表第一项——
/// 清单顺序是 Mojang 的更新时间顺序，不保证等价于版本新旧。
pub fn resolve_version<'a>(
    manifest: &'a VersionManifest,
    requested: &str,
) -> Result<&'a VersionEntry, ManifestError> {
    let id = if requested == "latest" {
        manifest.latest.release.as_str()
    } else {
        requested
    };

    manifest
        .find(id)
        .ok_or_else(|| ManifestError::NotFound(id.to_owned()))
}

/// 拉取并校验某个版本的元数据。
pub async fn fetch_meta(
    downloader: &HttpDownloader,
    entry: &VersionEntry,
) -> Result<VersionMeta, ManifestError> {
    let raw = downloader.fetch_text(&entry.url).await?;

    if let Some(expected) = entry.sha1.as_deref()
        && !verify_sha1(expected, raw.as_bytes())
    {
        return Err(ManifestError::Checksum(entry.url.clone()));
    }

    Ok(serde_json::from_str(&raw)?)
}

/// 版本元数据的落盘路径：`<game>/versions/<id>/<id>.json`。
///
/// 与官方启动器的布局一致。合并后的加载器版本也写在这里，所以启动前能直接读，
/// 不必每次去上游取 profile。
#[must_use]
pub fn meta_path(game_dir: &Path, version: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(version)
        .join(format!("{version}.json"))
}

/// 读本地版本元数据；文件不存在或内容损坏时返回 `None`。
///
/// 损坏不报错是刻意的：这份文件可以由上游重新生成，读不出来就当作「没装过」，
/// 让调用方走一遍安装流程。
#[must_use]
pub fn read_meta(game_dir: &Path, version: &str) -> Option<VersionMeta> {
    let raw = std::fs::read_to_string(meta_path(game_dir, version)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// 把版本元数据写到标准位置。
pub fn write_meta(game_dir: &Path, meta: &VersionMeta) -> std::io::Result<()> {
    let path = meta_path(game_dir, &meta.id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(meta).map_err(std::io::Error::other)?;
    std::fs::write(path, raw)
}
