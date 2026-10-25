//! Qingci Craft Launcher 的游戏进程层。
//!
//! 职责单一：接收 `qcl-core` 组装好的 [`LaunchCommand`](qcl_core::launch::LaunchCommand)，
//! 启动进程、逐行转发日志、等待退出、必要时终止。
//!
//! 之所以单独成 crate：进程管理必须依赖异步运行时与平台 API，
//! 而 `qcl-core` 要保持无 IO、可在任意平台完整测试。

pub mod process;

pub use process::{GameProcess, LaunchError, LaunchOutcome, LogLine, LogStream, spawn};
