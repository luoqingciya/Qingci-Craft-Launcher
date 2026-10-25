//! Qingci Craft Launcher 的 Java 运行时管理。
//!
//! 当前范围：**探测本机已有的 Java 并按要求挑选**。按需下载运行时属于后续 PR。
//!
//! 设计约束：探测结果只依赖传入的路径与文件系统，不隐式修改全局状态；
//! 唯一读取的环境变量是 `JAVA_HOME` 与 `PATH`，且集中在 [`detect::detect`] 内。

pub mod detect;
pub mod version;

pub use detect::{
    JavaInstallation, JavaSource, detect, discover_in, inspect, is_java_home, java_executable,
    java_executable_name, platform_roots, select,
};
pub use version::{JavaVersion, parse_java_version, parse_java_version_output, parse_release_file};
