// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 版本元数据：官方版本清单与单版本 `version.json` 的领域模型。
//!
//! 设计约束：
//!
//! 1. **不使用 `deny_unknown_fields`**。Mojang 会持续向 `version.json` 增加字段，
//!    严格拒绝未知字段会导致启动器在版本发布当天直接失效。
//! 2. **所有可能缺失的字段都用 `Option`**。旧版本（尤其是 1.6 之前）的字段集合
//!    与现代版本差异很大，用 `#[serde(default)]` 兜底而非假设字段必然存在。
//! 3. **未知枚举值不报错**。`type` 字段出现过新取值，未来仍可能出现。

pub mod manifest;
pub mod meta;
pub mod rules;

pub use manifest::{LatestVersions, VersionEntry, VersionManifest, VersionType};
pub use meta::{
    Argument, ArgumentValue, Arguments, Artifact, AssetIndex, Extract, JavaVersion, Library,
    LibraryDownloads, Logging, LoggingClient, VersionDownloads, VersionMeta,
};
pub use rules::{
    LaunchContext, OsRule, Rule, RuleAction, VersionRange, current_arch, current_os_name,
    evaluate_rules,
};
