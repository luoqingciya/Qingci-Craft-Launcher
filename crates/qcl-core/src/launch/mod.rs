// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 启动命令组装：classpath、natives 构件与参数模板。
//!
//! 本模块只负责**计算**（拼路径、展开变量），不做 IO。
//! 下载、校验、解压、起进程属于后续的安装层与运行时层。

pub mod classpath;
pub mod command;
pub mod template;

pub use classpath::{
    NativesArtifact, build_classpath, classpath_separator, join_classpath, library_path,
    maven_to_path, natives_artifacts,
};
pub use command::{AccountInfo, LaunchCommand, LaunchLayout, LaunchOptions, build_launch_command};
pub use template::LaunchVariables;
