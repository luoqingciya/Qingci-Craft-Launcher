// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 官方版本清单（`version_manifest_v2.json`）的领域模型。

use std::fmt;

use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};

/// 官方版本清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    /// 最新正式版与最新快照的版本号。
    pub latest: LatestVersions,
    /// 全部版本条目，按发布时间从新到旧排列。
    pub versions: Vec<VersionEntry>,
}

/// 最新版本指针。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersions {
    /// 最新正式版版本号。
    pub release: String,
    /// 最新快照版本号。
    pub snapshot: String,
}

/// 清单中的单个版本条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    /// 版本号，如 `26.3`。
    pub id: String,
    /// 版本类型。
    #[serde(rename = "type")]
    pub version_type: VersionType,
    /// 该版本 `version.json` 的下载地址。
    pub url: String,
    /// 该条目在清单中的最后更新时间（ISO 8601）。
    pub time: String,
    /// 版本发布时间（ISO 8601）。
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    /// 版本文件的 SHA-1。仅 v2 清单提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// 合规级别。`0` 表示版本过旧、不包含最新的玩家安全特性。
    #[serde(
        default,
        rename = "complianceLevel",
        skip_serializing_if = "Option::is_none"
    )]
    pub compliance_level: Option<u8>,
}

/// 版本类型。
///
/// 保留 [`VersionType::Other`] 变体：Mojang 可能引入新类型，
/// 未知取值不应导致整份清单解析失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionType {
    /// 正式版。
    Release,
    /// 快照版。
    Snapshot,
    /// 远古 Beta 版。
    OldBeta,
    /// 远古 Alpha 版。
    OldAlpha,
    /// 未识别的类型，保留原始字符串。
    Other(String),
}

impl VersionType {
    /// 是否为正式版。
    #[must_use]
    pub fn is_release(&self) -> bool {
        matches!(self, Self::Release)
    }

    /// 返回原始字符串形式。
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Release => "release",
            Self::Snapshot => "snapshot",
            Self::OldBeta => "old_beta",
            Self::OldAlpha => "old_alpha",
            Self::Other(raw) => raw,
        }
    }
}

impl fmt::Display for VersionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VersionType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            _ => Self::Other(raw),
        })
    }
}

impl Serialize for VersionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl VersionManifest {
    /// 按版本号查找条目。
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|entry| entry.id == id)
    }

    /// 查找最新正式版条目。
    ///
    /// 以 `latest.release` 为准而非取第一个 `release` 类型的条目，
    /// 因为清单顺序是 Mojang 的更新时间顺序，不保证等价于版本新旧。
    #[must_use]
    pub fn latest_release(&self) -> Option<&VersionEntry> {
        self.find(&self.latest.release)
    }
}
