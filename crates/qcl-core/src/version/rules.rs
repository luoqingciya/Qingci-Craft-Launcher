//! 依赖与启动参数的平台规则，以及求值逻辑。
//!
//! 求值语义直接决定「哪些库进 classpath、哪些参数进命令行」，写错会导致游戏启动失败
//! 或行为异常，因此这里的每个判断都以官方启动器行为为准：
//!
//! - **无 `rules` 字段，或为空数组** → 允许。
//! - **否则默认拒绝**：按数组顺序逐条匹配，命中 `allow` 立即允许，命中 `deny` 立即拒绝。
//! - 全部未命中 → 拒绝。
//! - 同一 `os` 条件内，`name` / `arch` / `version` / `versionRange` 之间是**与**关系；
//!   一条规则的多个 `features` 之间也是**与**关系。

use std::cmp::Ordering;
use std::collections::BTreeMap;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// 单条平台规则。
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

impl Rule {
    /// 判断本规则是否命中给定环境。
    ///
    /// 注意：命中与否**不代表最终允许**，最终结果由 [`evaluate_rules`] 结合 `action` 决定。
    #[must_use]
    pub fn matches(&self, context: &LaunchContext) -> bool {
        if let Some(os) = &self.os
            && !os.matches(context)
        {
            return false;
        }

        if let Some(required) = &self.features {
            // 上下文中未声明的功能开关一律按 false 处理，与官方行为一致。
            let satisfied = required.iter().all(|(name, expected)| {
                context.features.get(name).copied().unwrap_or(false) == *expected
            });
            if !satisfied {
                return false;
            }
        }

        true
    }
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

/// 操作系统条件。各字段均为可选，缺省表示不限制。
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

impl OsRule {
    /// 判断本系统条件是否满足给定环境。
    #[must_use]
    pub fn matches(&self, context: &LaunchContext) -> bool {
        if let Some(expected) = &self.name
            && context.os_name.as_deref() != Some(expected.as_str())
        {
            return false;
        }

        if let Some(expected) = &self.arch
            && context.arch.as_deref() != Some(expected.as_str())
        {
            return false;
        }

        // 无法确定系统版本时，任何依赖版本的条件都视为不满足——
        // 宁可少加一个库，也不要错误地加上不兼容的库。
        if let Some(pattern) = &self.version {
            let matched = context
                .os_version
                .as_deref()
                .is_some_and(|version| regex_matches(pattern, version));
            if !matched {
                return false;
            }
        }

        if let Some(range) = &self.version_range {
            let matched = context
                .os_version
                .as_deref()
                .is_some_and(|version| range.contains(version));
            if !matched {
                return false;
            }
        }

        true
    }
}

/// 系统版本区间，如 Windows 的 `{"min": "10.0.17134"}`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionRange {
    /// 下界，**含**该值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// 上界，**不含**该值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}

impl VersionRange {
    /// 判断版本号是否落在区间内。
    ///
    /// 边界语义为 `[min, max)`：官方用 `min: 10.0.17134` 与 `max: 10.0.17134`
    /// 两条规则互斥地划分版本，只有左闭右开才不会重叠或遗漏。
    #[must_use]
    pub fn contains(&self, version: &str) -> bool {
        if let Some(min) = &self.min
            && compare_versions(version, min).is_lt()
        {
            return false;
        }
        if let Some(max) = &self.max
            && !compare_versions(version, max).is_lt()
        {
            return false;
        }
        true
    }
}

/// 规则求值的运行环境。
///
/// 刻意设计为「由调用方填充」而非内部探测：求值逻辑因此保持纯函数，
/// 可以在任意平台上完整测试，不需要真的跑在 macOS 上才能验证 macOS 分支。
#[derive(Debug, Clone, Default)]
pub struct LaunchContext {
    /// 系统名：`windows` / `osx` / `linux`。取值与官方 JSON 一致，非 Rust 的 `std::env::consts::OS`。
    pub os_name: Option<String>,
    /// 架构：`x86` / `x86_64` / `arm64`。
    pub arch: Option<String>,
    /// 系统版本，如 `10.0.22631`。未提供时，带版本条件的规则一律视为不满足。
    pub os_version: Option<String>,
    /// 功能开关，如 `has_custom_resolution`。未声明按 `false` 处理。
    pub features: BTreeMap<String, bool>,
}

impl LaunchContext {
    /// 按当前进程所在平台构造。
    ///
    /// 系统版本留空：取真实版本号需要执行系统命令或读系统文件，属 IO，
    /// 不能放进本 crate。需要完整上下文（含系统版本）请用 `qcl-platform` 的
    /// `launch_context()`——它会影响 GC 选择这类按系统版本分流的参数。
    #[must_use]
    pub fn current() -> Self {
        Self {
            os_name: Some(current_os_name().to_owned()),
            arch: Some(current_arch().to_owned()),
            os_version: None,
            features: BTreeMap::new(),
        }
    }

    /// 设置系统版本。
    #[must_use]
    pub fn with_os_version(mut self, version: impl Into<String>) -> Self {
        self.os_version = Some(version.into());
        self
    }

    /// 设置单个功能开关。
    #[must_use]
    pub fn with_feature(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.features.insert(name.into(), enabled);
        self
    }
}

/// 当前系统的官方命名（`windows` / `osx` / `linux`）。
///
/// Rust 把 macOS 报成 `macos`，而 Mojang 的 JSON 里一律是 `osx`，必须转换。
#[must_use]
pub fn current_os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

/// 当前架构的官方命名（`x86` / `x86_64` / `arm64`）。
///
/// Rust 把 ARM64 报成 `aarch64`，Mojang 的 JSON 里是 `arm64`。
#[must_use]
pub fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    }
}

/// 按官方语义求值一组规则。
///
/// 语义见模块文档：无规则即允许，否则默认拒绝并按顺序取第一条命中项的动作。
#[must_use]
pub fn evaluate_rules(rules: Option<&[Rule]>, context: &LaunchContext) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }

    rules
        .iter()
        .find(|rule| rule.matches(context))
        .is_some_and(|rule| rule.action == RuleAction::Allow)
}

/// 用正则匹配系统版本。正则非法时视为不匹配，而不是 panic。
///
/// 用 `regex-lite` 而非完整 `regex`：官方 `os.version` 只出现 `^10\.`、`^6\.1\.`
/// 这类极简模式，用不上回溯、环视等能力，而完整 `regex` 会带来约 1 MB 的固定体积开销——
/// 对一个安装包预算 12 MB 的启动器而言不值得。
fn regex_matches(pattern: &str, value: &str) -> bool {
    Regex::new(pattern).is_ok_and(|regex| regex.is_match(value))
}

/// 逐段数值比较两个版本号，缺失的段按 0 处理。
///
/// 仅用于 `10.0.17134` 这类纯数字点分版本（系统构建号）。非数字段按 0 处理，
/// 因为官方 `versionRange` 不会出现语义化版本后缀。
fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts: Vec<u64> = left.split('.').map(parse_segment).collect();
    let right_parts: Vec<u64> = right.split('.').map(parse_segment).collect();

    let length = left_parts.len().max(right_parts.len());
    for index in 0..length {
        let left_value = left_parts.get(index).copied().unwrap_or(0);
        let right_value = right_parts.get(index).copied().unwrap_or(0);
        match left_value.cmp(&right_value) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

fn parse_segment(segment: &str) -> u64 {
    segment.trim().parse().unwrap_or(0)
}
