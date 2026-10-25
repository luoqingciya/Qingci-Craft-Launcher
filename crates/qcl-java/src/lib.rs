//! Qingci Craft Launcher 的 Java 运行时管理。
//!
//! 当前范围：
//! 1. 探测本机已有的 Java 并按需挑选（[`detect`]）；
//! 2. 解析 Mojang 官方运行时清单，生成安装计划（[`runtime`]）。
//!
//! 实际下载与安装属于后续 PR。设计约束：探测结果只依赖传入的路径与文件系统，
//! 不隐式修改全局状态；唯一读取的环境变量是 `JAVA_HOME` 与 `PATH`，且集中在 [`detect::detect`] 内。

pub mod detect;
pub mod runtime;
pub mod version;

pub use detect::{
    JavaInstallation, JavaSource, detect, discover_in, inspect, is_java_home, java_executable,
    java_executable_name, platform_roots, select,
};
pub use runtime::{
    RuntimeFileTask, RuntimeIndex, RuntimeManifest, RuntimePlan, plan_runtime_files,
    platform_key_for, runtime_install_path, runtime_platform_key, select_release,
};
pub use version::{JavaVersion, parse_java_version, parse_java_version_output, parse_release_file};
