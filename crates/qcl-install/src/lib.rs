//! Qingci Craft Launcher 的安装编排。
//!
//! 这一层负责把「解析」与「下载」串成可执行的安装：决定先后顺序、处理版本间的
//! 兼容分支、把失败归因到具体阶段。它不实现自己的网络或落盘逻辑，一律委托给
//! [`qcl_net`]。
//!
//! 三个入口，粒度从粗到细：
//!
//! - [`install_version`]：装完一个版本所需的全部文件（推荐）
//! - [`install_assets`]：只装资产
//! - [`install_natives`]：只解压 natives 归档

pub mod assets;
pub mod natives;
pub mod version;

pub use assets::{AssetError, AssetInstallOptions, AssetInstallReport, install_assets};
pub use natives::{ExtractOutcome, NativeError, NativesReport, extract_natives, install_natives};
pub use version::{
    InstallPhase, InstallProgress, VersionError, VersionInstallOptions, VersionInstallReport,
    install_version,
};
