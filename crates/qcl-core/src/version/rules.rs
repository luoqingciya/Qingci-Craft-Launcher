//! 依赖与启动参数的平台规则。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 单条平台规则。
///
/// 求值语义（默认拒绝、按序匹配）在后续的规则求值模块中实现，
/// 本模块只负责结构建模。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// 命中后的动作。
    pub action: RuleAction,
    /// 操作系统条件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    /// 功能开关条件，如 `{"is_demo_user": true}`。
    ///
    /// 使用 `BTreeMap` 而非 `HashMap`：序列化顺序必须可复现，
    /// 否则快照测试与写出合并后的 `version.json` 都会产生无意义差异。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

/// 规则动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    /// 允许。
    Allow,
    /// 拒绝。
    Deny,
}

/// 操作系统条件。三个字段均为可选，缺省表示不限制。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    /// 系统名：`windows` / `osx` / `linux`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 架构：`x86` / `x86_64` / `arm64`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// 系统版本正则，如 `^10\\.`。旧版本使用该形式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 系统版本区间。新版本改用该形式，与 `version` 互斥。
    #[serde(
        default,
        rename = "versionRange",
        skip_serializing_if = "Option::is_none"
    )]
    pub version_range: Option<VersionRange>,
}

/// 系统版本区间，如 Windows 的 `{"min": "10.0.17134"}`。
///
/// 两端均为可选，缺省表示该侧不设限；比较语义为版本号逐段数值比较而非字符串比较。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionRange {
    /// 下界（含）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// 上界（不含）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}
