// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 运行时清单解析与安装计划的测试。

use qcl_java::runtime::{
    RuntimeIndex, RuntimeManifest, plan_runtime_files, platform_key_for, runtime_install_path,
    runtime_platform_key, select_release,
};

/// 2026-10-25 从官方 `all.json` 原样拉取的索引（14 KB）。
const INDEX: &str = include_str!("fixtures/runtime-index.json");
/// 手工构造：覆盖 directory / file / link / 只有 lzma 四种情形。
const MANIFEST: &str = include_str!("fixtures/runtime-manifest-trimmed.json");

fn index() -> RuntimeIndex {
    serde_json::from_str(INDEX).expect("官方索引应可解析")
}

fn manifest() -> RuntimeManifest {
    serde_json::from_str(MANIFEST).expect("清单应可解析")
}

#[test]
fn official_index_contains_the_runtime_26_3_needs() {
    let index = index();

    // 26.3 声明 javaVersion.component = java-runtime-epsilon
    let release = select_release(&index, "java-runtime-epsilon", "windows-x64")
        .expect("windows-x64 应有 epsilon 运行时");

    assert_eq!(release.version.name, "25.0.1");
    assert!(release.version.released.starts_with("2025-12-10"));
    assert!(
        release.manifest.url.ends_with("/manifest.json"),
        "条目应指向文件级清单"
    );
    assert!(release.manifest.size > 0);
}

#[test]
fn official_index_covers_64_bit_platforms_only() {
    let index = index();

    // 官方为这六个平台发布了现代运行时。
    for platform in [
        "windows-x64",
        "windows-arm64",
        "mac-os",
        "mac-os-arm64",
        "linux",
    ] {
        assert!(
            select_release(&index, "java-runtime-epsilon", platform).is_some(),
            "{platform} 应能取到 epsilon 运行时"
        );
    }

    // 32 位 x86 的组件键存在，但**条目数组为空**——Mojang 不为 32 位发布现代运行时。
    // 这是真实数据里的坑：只看「键是否存在」会误判为可用，必须检查数组非空。
    // 产品影响：32 位用户无法通过官方渠道安装 Java 25，启动器需提示手动安装。
    for platform in ["windows-x86", "linux-i386"] {
        assert!(
            select_release(&index, "java-runtime-epsilon", platform).is_none(),
            "{platform} 不应有现代运行时（官方未发布）"
        );
    }

    assert!(
        select_release(&index, "java-runtime-epsilon", "plan9").is_none(),
        "未知平台应返回 None 而不是 panic"
    );
    assert!(
        select_release(&index, "java-runtime-nonexistent", "linux").is_none(),
        "未知组件应返回 None"
    );
}

#[test]
fn empty_release_list_yields_none() {
    // 官方索引里确实存在「键在、数组空」的情况，单独锁住这个行为。
    let raw = r#"{ "linux": { "java-runtime-epsilon": [] } }"#;
    let index: RuntimeIndex = serde_json::from_str(raw).expect("应可解析");
    assert!(select_release(&index, "java-runtime-epsilon", "linux").is_none());
}

#[test]
fn platform_keys_follow_official_naming() {
    assert_eq!(platform_key_for("windows", "x86_64"), Some("windows-x64"));
    assert_eq!(platform_key_for("windows", "x86"), Some("windows-x86"));
    assert_eq!(
        platform_key_for("windows", "aarch64"),
        Some("windows-arm64")
    );
    assert_eq!(platform_key_for("macos", "x86_64"), Some("mac-os"));
    assert_eq!(platform_key_for("macos", "aarch64"), Some("mac-os-arm64"));
    assert_eq!(platform_key_for("linux", "x86_64"), Some("linux"));
    assert_eq!(platform_key_for("linux", "x86"), Some("linux-i386"));

    assert_eq!(platform_key_for("freebsd", "x86_64"), None);
    assert_eq!(platform_key_for("linux", "riscv64"), None);
}

#[test]
fn current_platform_is_supported_in_ci() {
    // 三个 CI 平台都在官方支持列表内；返回 None 说明映射写漏了。
    let key = runtime_platform_key();
    assert!(
        key.is_some(),
        "当前平台 {} / {} 未映射到官方运行时平台键",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

#[test]
fn install_path_is_scoped_by_component_and_platform() {
    assert_eq!(
        runtime_install_path("java-runtime-epsilon", "windows-x64", "bin/java.exe"),
        "runtime/java-runtime-epsilon/windows-x64/bin/java.exe"
    );
}

#[test]
fn plan_only_includes_files_with_raw_downloads() {
    let plan = plan_runtime_files(&manifest(), "java-runtime-epsilon", "windows-x64");

    // directory、link、只有 lzma 的条目都应被跳过。
    assert_eq!(plan.len(), 3);
    assert_eq!(plan.total_bytes(), 200 + 300 + 50);

    let paths: Vec<&str> = plan.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "runtime/java-runtime-epsilon/windows-x64/bin/java",
            "runtime/java-runtime-epsilon/windows-x64/lib/modules",
            "runtime/java-runtime-epsilon/windows-x64/release",
        ],
        "顺序应稳定（按路径字典序），便于比对与复现"
    );

    assert!(
        !paths.iter().any(|path| path.contains("only-lzma")),
        "只有 lzma 的条目不能进入计划"
    );
    assert!(
        !paths.iter().any(|path| path.contains("foo.jar")),
        "符号链接不进入计划"
    );
}

#[test]
fn executable_flag_is_preserved() {
    let plan = plan_runtime_files(&manifest(), "java-runtime-epsilon", "linux");

    let java = plan
        .files
        .iter()
        .find(|file| file.path.ends_with("bin/java"))
        .expect("应包含 java 可执行文件");
    assert!(java.executable, "bin/java 必须标记为可执行");

    let modules = plan
        .files
        .iter()
        .find(|file| file.path.ends_with("lib/modules"))
        .expect("应包含 lib/modules");
    assert!(!modules.executable, "普通文件不应被标记为可执行");
}

#[test]
fn raw_download_is_preferred_over_lzma() {
    let plan = plan_runtime_files(&manifest(), "java-runtime-epsilon", "windows-x64");

    let java = plan
        .files
        .iter()
        .find(|file| file.path.ends_with("bin/java"))
        .expect("应包含 java");
    assert_eq!(
        java.url, "https://example.invalid/java",
        "应使用 raw 而非 lzma 的地址"
    );
    assert_eq!(java.size, 200);
}
