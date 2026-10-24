//! Qingci Craft Launcher 的领域核心。
//!
//! 本 crate 不依赖 Tauri，可在无 GUI 环境中独立编译与测试，因此承担全部可自动化
//! 验证的业务逻辑：版本清单解析、依赖规则求值、启动参数组装与进程编排。
//!
//! 当前进度：Phase 1，已建立官方版本清单与单版本元数据的领域模型。

pub mod error;
pub mod version;

pub use error::{CoreError, Result};
pub use version::{
    Argument, ArgumentValue, Arguments, Artifact, AssetIndex, Extract, JavaVersion, LatestVersions,
    Library, LibraryDownloads, Logging, LoggingClient, OsRule, Rule, RuleAction, VersionDownloads,
    VersionEntry, VersionManifest, VersionMeta, VersionRange, VersionType,
};

/// 启动器标识，用于 Minecraft 的 `--versionType` 参数与日志前缀。
pub const LAUNCHER_NAME: &str = "QingciCraftLauncher";

/// 启动器版本，取自 Cargo 包版本，随 workspace 版本号同步。
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{LAUNCHER_NAME, LAUNCHER_VERSION};

    #[test]
    fn launcher_identity_is_stable() {
        assert_eq!(LAUNCHER_NAME, "QingciCraftLauncher");
        assert!(!LAUNCHER_VERSION.is_empty());
    }
}
