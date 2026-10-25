//! Qingci Craft Launcher 的 Java 运行时管理。
//!
//! 当前范围：
//! 1. 探测本机已有的 Java 并按需挑选（[`detect`]）；
//! 2. 解析 Mojang 官方运行时清单，生成安装计划（[`runtime`]）；
//! 3. 按清单下载并安装运行时（[`install`]）。
//!
//! 设计约束：探测结果只依赖传入的路径与文件系统，不隐式修改全局状态；
//! 唯一读取的环境变量是 `JAVA_HOME` 与 `PATH`，且集中在 [`detect::detect`] 内。
//! 网络与落盘完全委托给 `qcl-net`，因此安装过程复用同一套重试与进度机制。

pub mod detect;
pub mod install;
pub mod runtime;
pub mod version;

pub use detect::{
    JavaInstallation, JavaSource, detect, discover_in, inspect, is_java_home, java_executable,
    java_executable_name, platform_roots, select,
};
pub use install::{InstallError, install_runtime};
pub use runtime::{
    RuntimeFileTask, RuntimeIndex, RuntimeManifest, RuntimePlan, plan_runtime_files,
    platform_key_for, runtime_home, runtime_install_path, runtime_platform_key, select_release,
};
pub use version::{JavaVersion, parse_java_version, parse_java_version_output, parse_release_file};
