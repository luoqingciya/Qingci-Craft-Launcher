// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Qingci Craft Launcher 的安装编排。
//!
//! 这一层负责把「解析」与「下载」串成可执行的安装：决定先后顺序、处理版本间的
//! 兼容分支、把失败归因到具体阶段。它不实现自己的网络或落盘逻辑，一律委托给
//! [`qcl_net`]。
//!
//! 入口，粒度从粗到细：
//!
//! - [`install_version`]：装完一个版本所需的全部文件（推荐）
//! - [`install_assets`]：只装资产
//! - [`install_natives`]：只解压 natives 归档
//! - [`fetch_manifest`] / [`resolve_version`] / [`fetch_meta`]：安装前的元数据准备

pub mod assets;
pub mod loader;
pub mod manifest;
pub mod natives;
pub mod version;

pub use assets::{AssetError, AssetInstallOptions, AssetInstallReport, install_assets};
pub use loader::{
    FABRIC_MAVEN_BASE, FABRIC_META_BASE, FabricLibrary, FabricProfile, LoaderError, LoaderInfo,
    fetch_loader_list, fetch_loader_list_from, fetch_profile, fetch_profile_from, inherits_from,
    merge, merged_version_id, resolve_libraries,
};
pub use manifest::{
    ManifestError, VERSION_MANIFEST_URL, fetch_manifest, fetch_manifest_from, fetch_meta,
    resolve_version,
};
pub use natives::{ExtractOutcome, NativeError, NativesReport, extract_natives, install_natives};
pub use version::{
    InstallPhase, InstallProgress, VersionError, VersionInstallOptions, VersionInstallReport,
    install_version,
};
