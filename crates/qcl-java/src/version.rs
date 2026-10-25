// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Java 版本号解析。
//!
//! Java 的版本字符串形态混乱：8 及以前是 `1.8.0_392`，9 及以后是 `21.0.12`，
//! 还常见 `25.0.4+101`、`17.0.9+9-LTS` 这类带构建号与后缀的写法。
//! 启动器只关心**主版本号**——它决定能否运行某个游戏版本。

use std::fmt;

/// Java 版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaVersion {
    /// 主版本号，如 `8` / `17` / `21` / `25`。
    pub major: u32,
    /// 原始版本字符串，保留以便排查问题。
    pub raw: String,
}

impl fmt::Display for JavaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// 解析 Java 版本字符串。
///
/// 规则：以 `1.` 开头时取第二段作为主版本（`1.8.0_392` → 8），
/// 否则取第一段（`25.0.4+101` → 25）。
#[must_use]
pub fn parse_java_version(raw: &str) -> Option<JavaVersion> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split(['.', '_', '+', '-']);
    let first = parts.next()?.parse::<u32>().ok()?;

    let major = if first == 1 {
        // 1.x 时代：主版本在第二段。
        parts.next()?.parse::<u32>().ok()?
    } else {
        first
    };

    Some(JavaVersion {
        major,
        raw: trimmed.to_owned(),
    })
}

/// 从 JDK 根目录下的 `release` 文件中提取 `JAVA_VERSION`。
///
/// 优先读这个文件而不是执行 `java -version`：一次文件读取比启动一个进程快两个数量级，
/// 而探测阶段往往要检查十几个候选目录。
#[must_use]
pub fn parse_release_file(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("JAVA_VERSION=") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// 解析 `java -version` 的输出。
///
/// 该命令把版本写到 **stderr** 而不是 stdout，这是长期存在的历史行为。
/// 输出形如 `openjdk version "21.0.12" 2025-07-15` 或 `java version "1.8.0_392"`。
#[must_use]
pub fn parse_java_version_output(output: &str) -> Option<JavaVersion> {
    const MARKER: &str = "version \"";

    for line in output.lines() {
        let Some(start) = line.find(MARKER) else {
            continue;
        };
        let rest = &line[start + MARKER.len()..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        if let Some(version) = parse_java_version(&rest[..end]) {
            return Some(version);
        }
    }

    None
}
