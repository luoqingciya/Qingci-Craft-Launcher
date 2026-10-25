//! Qingci Craft Launcher 的安装编排。
//!
//! 这一层负责把「解析」与「下载」串成一次可执行的安装：决定先后顺序、处理
//! 版本间的兼容分支、把失败归因到具体阶段。它不实现自己的网络或落盘逻辑，
//! 一律委托给 [`qcl_net`]。
//!
//! 当前范围：**资产安装**（含 1.6 及更早的虚拟目录展开）。
//! 版本文件与 natives 解压属于后续 PR。

pub mod assets;

pub use assets::{AssetError, AssetInstallOptions, AssetInstallReport, install_assets};
