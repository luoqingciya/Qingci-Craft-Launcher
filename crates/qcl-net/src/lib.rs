// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 下载层：下载计划、完整性校验与重试策略。
//!
//! 当前只包含**纯逻辑**，不做网络与磁盘 IO。这样划分是为了让「该下载什么、跳过什么、
//! 失败后重试几次」这些判断能在 CI 中确定性验证，而不依赖真实网络。
//! 异步执行器（reqwest + tokio）在后续 PR 中基于这些原语实现。

pub mod executor;
pub mod http;
pub mod plan;
pub mod retry;
pub mod verify;

pub use executor::{
    DownloadError, DownloadFailure, DownloadOps, DownloadOptions, DownloadReport, Progress,
    execute_plan,
};
pub use http::HttpDownloader;
pub use plan::{
    DownloadPlan, DownloadTask, asset_index_task, plan_assets, plan_assets_with_base,
    plan_version_files,
};
pub use retry::{RetryPolicy, is_retryable_status};
pub use verify::{Sha1Hasher, hex_sha1, verify_sha1};
