// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 平台规则求值的测试。
//!
//! 期望值不是凭直觉写的：`26.3` 的库数量与参数数量先由一份独立的 JS 实现
//! 对同一份官方 JSON 求值得到（Windows 74 / macOS 80 / Linux 66），
//! 再作为断言基准。两份独立实现得到一致结果，才说明语义理解正确。

use std::collections::BTreeMap;

use qcl_core::version::{
    LaunchContext, OsRule, Rule, RuleAction, VersionMeta, VersionRange, current_arch,
    current_os_name, evaluate_rules,
};

const META_26_3: &str = include_str!("fixtures/26.3.json");

fn context(os: &str, arch: &str) -> LaunchContext {
    LaunchContext {
        os_name: Some(os.to_owned()),
        arch: Some(arch.to_owned()),
        os_version: None,
        features: BTreeMap::new(),
    }
}

fn windows_11() -> LaunchContext {
    context("windows", "x86_64").with_os_version("10.0.22631")
}

fn allow(os: Option<OsRule>) -> Rule {
    Rule {
        action: RuleAction::Allow,
        os,
        features: None,
    }
}

fn deny(os: Option<OsRule>) -> Rule {
    Rule {
        action: RuleAction::Deny,
        os,
        features: None,
    }
}

fn os_named(name: &str) -> Option<OsRule> {
    Some(OsRule {
        name: Some(name.to_owned()),
        ..OsRule::default()
    })
}

fn meta_26_3() -> VersionMeta {
    serde_json::from_str(META_26_3).expect("26.3 夹具应可解析")
}

#[test]
fn absent_or_empty_rules_allow_everything() {
    let ctx = windows_11();
    assert!(evaluate_rules(None, &ctx), "无 rules 字段应允许");
    assert!(evaluate_rules(Some(&[]), &ctx), "空 rules 数组应允许");
}

#[test]
fn rules_deny_by_default_when_nothing_matches() {
    let ctx = windows_11();
    let rules = [allow(os_named("osx"))];
    assert!(
        !evaluate_rules(Some(&rules), &ctx),
        "没有任何规则命中时应拒绝，而不是允许"
    );
}

#[test]
fn first_matching_rule_wins() {
    let ctx = windows_11();

    let allow_then_deny = [allow(os_named("windows")), deny(None)];
    assert!(
        evaluate_rules(Some(&allow_then_deny), &ctx),
        "先命中的 allow 应立即生效"
    );

    let deny_then_allow = [deny(os_named("windows")), allow(None)];
    assert!(
        !evaluate_rules(Some(&deny_then_allow), &ctx),
        "先命中的 deny 应立即生效，顺序不可忽略"
    );
}

#[test]
fn os_name_and_arch_must_match_exactly() {
    let ctx = windows_11();

    let wrong_os = [allow(os_named("linux"))];
    assert!(!evaluate_rules(Some(&wrong_os), &ctx));

    let wrong_arch = [allow(Some(OsRule {
        arch: Some("x86".to_owned()),
        ..OsRule::default()
    }))];
    assert!(
        !evaluate_rules(Some(&wrong_arch), &ctx),
        "x86 的规则不应命中 x86_64 环境"
    );

    let right_arch = [allow(Some(OsRule {
        arch: Some("x86_64".to_owned()),
        ..OsRule::default()
    }))];
    assert!(evaluate_rules(Some(&right_arch), &ctx));
}

#[test]
fn version_range_is_left_closed_right_open() {
    let boundary = "10.0.17134";

    let at_least = VersionRange {
        min: Some(boundary.to_owned()),
        max: None,
    };
    assert!(at_least.contains(boundary), "min 应包含边界值");
    assert!(at_least.contains("10.0.22631"));
    assert!(!at_least.contains("10.0.17133"));

    let below = VersionRange {
        min: None,
        max: Some(boundary.to_owned()),
    };
    assert!(!below.contains(boundary), "max 不应包含边界值");
    assert!(below.contains("10.0.17133"));

    // 缺失的段按 0 处理，因此 10.0 与 10.0.0 等价。
    assert!(at_least.contains("10.0") == at_least.contains("10.0.0"));
}

#[test]
fn legacy_version_regex_is_supported() {
    let ctx = windows_11();
    let rule = allow(Some(OsRule {
        version: Some(r"^10\.".to_owned()),
        ..OsRule::default()
    }));
    assert!(
        evaluate_rules(Some(&[rule]), &ctx),
        "1.15 时代的 os.version 正则应生效"
    );

    let rule = allow(Some(OsRule {
        version: Some(r"^6\.1\.".to_owned()),
        ..OsRule::default()
    }));
    assert!(!evaluate_rules(Some(&[rule]), &ctx));
}

#[test]
fn version_conditions_fail_when_os_version_unknown() {
    let ctx = context("windows", "x86_64");
    assert!(ctx.os_version.is_none());

    let range_rule = allow(Some(OsRule {
        version_range: Some(VersionRange {
            min: Some("10.0.17134".to_owned()),
            max: None,
        }),
        ..OsRule::default()
    }));
    assert!(
        !evaluate_rules(Some(&[range_rule]), &ctx),
        "无法确定系统版本时应视为不满足，避免误加不兼容的库"
    );
}

#[test]
fn features_are_compared_against_declared_flags() {
    let mut rule = allow(None);
    rule.features = Some(BTreeMap::from([("is_demo_user".to_owned(), true)]));

    let without = windows_11();
    assert!(
        !evaluate_rules(Some(&[rule.clone()]), &without),
        "未声明即按 false"
    );

    let with = windows_11().with_feature("is_demo_user", true);
    assert!(evaluate_rules(Some(&[rule]), &with));

    // 要求开关为 false 时，未声明应当满足。
    let mut expects_false = allow(None);
    expects_false.features = Some(BTreeMap::from([("is_demo_user".to_owned(), false)]));
    assert!(evaluate_rules(Some(&[expects_false]), &without));
}

#[test]
fn library_selection_matches_reference_counts() {
    let meta = meta_26_3();
    let count = |ctx: &LaunchContext| {
        meta.libraries
            .iter()
            .filter(|library| library.applies_to(ctx))
            .count()
    };

    assert_eq!(count(&windows_11()), 74, "Windows x86_64 应选中 74 个库");
    assert_eq!(
        count(&context("osx", "arm64")),
        80,
        "macOS arm64 应选中 80 个库"
    );
    assert_eq!(
        count(&context("linux", "x86_64")),
        66,
        "Linux x86_64 应选中 66 个库"
    );
}

#[test]
fn specific_platform_libraries_are_included_and_excluded_correctly() {
    let meta = meta_26_3();
    let find = |needle: &str, ctx: &LaunchContext| {
        meta.libraries
            .iter()
            .find(|library| library.name.contains(needle))
            .map(|library| library.applies_to(ctx))
    };

    // java-objc-bridge 只在 macOS 上需要。
    assert_eq!(
        find("java-objc-bridge", &context("osx", "arm64")),
        Some(true)
    );
    assert_eq!(find("java-objc-bridge", &windows_11()), Some(false));
}

#[test]
fn game_arguments_are_resolved_according_to_features() {
    let meta = meta_26_3();
    let arguments = meta.arguments.as_ref().expect("26.3 应有 arguments");

    let plain = windows_11();
    assert_eq!(
        arguments.resolve_game(&plain).len(),
        20,
        "无功能开关时应为 20 个参数"
    );

    let with_resolution = windows_11().with_feature("has_custom_resolution", true);
    let resolved = arguments.resolve_game(&with_resolution);
    assert_eq!(resolved.len(), 24);
    assert!(resolved.contains(&"--width"));
    assert!(resolved.contains(&"--height"));

    let demo = windows_11().with_feature("is_demo_user", true);
    let resolved = arguments.resolve_game(&demo);
    assert_eq!(resolved.len(), 21);
    assert!(resolved.contains(&"--demo"));

    let quick_play = windows_11().with_feature("is_quick_play_singleplayer", true);
    assert_eq!(arguments.resolve_game(&quick_play).len(), 22);
}

#[test]
fn jvm_arguments_exclude_other_platform_flags() {
    let meta = meta_26_3();
    let arguments = meta.arguments.as_ref().expect("26.3 应有 arguments");

    let resolved = arguments.resolve_jvm(&windows_11());
    assert_eq!(resolved.len(), 13, "Windows 应得到 13 个 JVM 参数");
    assert!(
        !resolved.contains(&"-XstartOnFirstThread"),
        "macOS 专属参数不应出现在 Windows 上"
    );
}

#[test]
fn current_platform_uses_official_naming() {
    let os = current_os_name();
    assert!(
        ["windows", "osx", "linux"].contains(&os),
        "系统名应为官方取值，实际为 {os}"
    );
    assert_ne!(os, "macos", "macOS 必须映射为 osx");

    let arch = current_arch();
    assert_ne!(arch, "aarch64", "ARM64 必须映射为 arm64");
}
