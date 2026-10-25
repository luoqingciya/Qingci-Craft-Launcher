// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 本机平台探测。
//!
//! `qcl-core` 刻意不碰 IO，因此「系统名与架构」这类静态信息由它提供，
//! 而需要执行命令或读系统文件的**系统版本**放在这里。
//!
//! 系统版本不是可有可无的装饰。26.3 的 `default-user-jvm` 用它区分 GC：
//!
//! | 条件 | 追加参数 |
//! |---|---|
//! | Windows `versionRange: {min: 10.0.17134}` | `-XX:+UseZGC` |
//! | Windows `versionRange: {max: 10.0.17134}` | `-XX:+UnlockExperimentalVMOptions -XX:+UseG1GC ...` |
//!
//! 缺版本号时两条分支都不命中，命令行里一个 GC 参数都没有——游戏能跑（JVM 用默认 GC），
//! 但与官方启动器不一致，性能特征也不同。

use std::collections::BTreeMap;

// Linux 分支读 /proc 而不起进程，所以这个导入只在 Windows 与 macOS 上需要。
// 无条件导入会让 `-D warnings` 在 Linux CI 上直接失败——本机（Windows）编译不出来。
#[cfg(any(windows, target_os = "macos"))]
use std::process::Command;

use qcl_core::version::{LaunchContext, current_arch, current_os_name};

/// 按当前进程所在平台构造启动上下文（含系统版本）。
#[must_use]
pub fn launch_context() -> LaunchContext {
    LaunchContext {
        os_name: Some(current_os_name().to_owned()),
        arch: Some(current_arch().to_owned()),
        os_version: os_version(),
        features: BTreeMap::new(),
    }
}

/// 当前系统的版本号，形如 `10.0.22631` / `14.5` / `6.8.0`。
///
/// 取不到时返回 `None`——调用方应把「版本未知」当成「带版本条件的规则不满足」，
/// 而不是猜一个值。
#[must_use]
pub fn os_version() -> Option<String> {
    #[cfg(windows)]
    {
        windows_version()
    }

    #[cfg(target_os = "macos")]
    {
        macos_version()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_version()
    }

    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_version() -> Option<String> {
    // `ver` 的输出是 `Microsoft Windows [Version 10.0.22631.4460]`，
    // 中文系统则是 `[版本 10.0.22631.4460]`。只提取数字与点，不解析前后缀，
    // 因此与系统语言无关。
    let output = Command::new("cmd").args(["/c", "ver"]).output().ok()?;
    leading_version(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn macos_version() -> Option<String> {
    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    leading_version(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_version() -> Option<String> {
    // 内核 release 形如 `6.8.0-45-generic`，只取前面的纯数字部分：
    // `versionRange` 做的是逐段数值比较，带后缀会让比较失去意义。
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").ok()?;
    leading_version(&release)
}

/// 从任意文本里提取第一段形如 `1.2.3` 的版本号。
///
/// 只认「数字与点」组成的连续片段，且至少两段。这样既能从 `cmd /c ver` 的
/// 本地化输出里取出 `10.0.22631.4460`，也能从 `6.8.0-45-generic` 里取出 `6.8.0`。
#[must_use]
pub fn leading_version(text: &str) -> Option<String> {
    let mut candidate = String::new();

    for character in text.chars() {
        if character.is_ascii_digit() || character == '.' {
            candidate.push(character);
            continue;
        }
        if let Some(version) = normalize(&candidate) {
            return Some(version);
        }
        candidate.clear();
    }

    normalize(&candidate)
}

/// 去掉首尾的点，并要求至少两段非空数字。
fn normalize(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim_matches('.');
    let segments: Vec<&str> = trimmed.split('.').collect();

    if segments.len() >= 2 && segments.iter().all(|segment| !segment.is_empty()) {
        return Some(trimmed.to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{launch_context, leading_version};

    #[test]
    fn parses_english_windows_output() {
        assert_eq!(
            leading_version("Microsoft Windows [Version 10.0.22631.4460]\r\n").as_deref(),
            Some("10.0.22631.4460")
        );
        // Windows 7
        assert_eq!(
            leading_version("Microsoft Windows [Version 6.1.7601]").as_deref(),
            Some("6.1.7601")
        );
    }

    #[test]
    fn parses_localized_windows_output() {
        // 中文系统的 `ver` 输出用「版本」，只提取数字才能不依赖语言。
        assert_eq!(
            leading_version("Microsoft Windows [版本 10.0.19045.5131]").as_deref(),
            Some("10.0.19045.5131")
        );
    }

    #[test]
    fn parses_macos_and_linux_shapes() {
        assert_eq!(leading_version("14.5\n").as_deref(), Some("14.5"));
        assert_eq!(leading_version("15.0.1\n").as_deref(), Some("15.0.1"));
        // 内核 release 带后缀，只取纯数字前缀。
        assert_eq!(
            leading_version("6.8.0-45-generic\n").as_deref(),
            Some("6.8.0")
        );
    }

    #[test]
    fn rejects_text_without_a_version() {
        assert_eq!(leading_version(""), None);
        assert_eq!(leading_version("Microsoft Windows"), None);
        // 单段数字不算版本：`1` 无法参与 `versionRange` 比较。
        assert_eq!(leading_version("build 42"), None);
        assert_eq!(leading_version(".."), None);
    }

    #[test]
    fn context_has_platform_identity() {
        let context = launch_context();
        assert!(context.os_name.is_some());
        assert!(context.arch.is_some());
        assert!(context.features.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn windows_version_is_detected() {
        // 本机必然能取到：拿不到说明 `ver` 调用或解析出了问题。
        let version = launch_context()
            .os_version
            .expect("Windows 上应能取到版本号");
        assert!(
            version.starts_with("10.") || version.starts_with("6."),
            "Windows 版本号形态异常：{version}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_version_is_detected() {
        let version = launch_context().os_version.expect("macOS 上应能取到版本号");
        assert!(version.contains('.'), "版本号形态异常：{version}");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_version_is_detected() {
        let version = launch_context().os_version.expect("Linux 上应能取到版本号");
        assert!(
            version
                .split('.')
                .all(|segment| segment.chars().all(|c| c.is_ascii_digit())),
            "版本号应只含数字与点：{version}"
        );
    }
}
