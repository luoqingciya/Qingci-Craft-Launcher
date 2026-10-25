// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Java 探测与版本解析的测试。

use std::path::{Path, PathBuf};

use qcl_java::{
    JavaInstallation, JavaSource, JavaVersion, detect, discover_in, inspect, is_java_home,
    java_executable_name, parse_java_version, parse_java_version_output, parse_release_file,
    select,
};
use tempfile::TempDir;

/// 造一个假的 JDK 根目录：`bin/java` + `release` 文件。
///
/// 只写 `release` 而不准备真实可执行文件，探测就会走「读文件」快路径，
/// 不会真的去执行 `java -version`——测试因此既快又不依赖本机装了什么。
fn fake_jdk(root: &Path, name: &str, version: &str) -> PathBuf {
    let home = root.join(name);
    std::fs::create_dir_all(home.join("bin")).expect("应能创建 bin 目录");
    std::fs::write(home.join("bin").join(java_executable_name()), b"").expect("应能创建 java 文件");
    std::fs::write(
        home.join("release"),
        format!("JAVA_VERSION=\"{version}\"\nJAVA_VERSION_DATE=\"2026-01-20\"\n"),
    )
    .expect("应能写入 release 文件");
    home
}

fn installation(major: u32) -> JavaInstallation {
    JavaInstallation {
        home: PathBuf::from(format!("/fake/{major}")),
        executable: PathBuf::from(format!("/fake/{major}/bin/java")),
        version: JavaVersion {
            major,
            raw: format!("{major}.0.0"),
        },
        source: JavaSource::PlatformDirectory,
    }
}

#[test]
fn java_versions_are_parsed_across_eras() {
    // 1.x 时代：主版本在第二段。
    assert_eq!(parse_java_version("1.8.0_392").map(|v| v.major), Some(8));
    assert_eq!(parse_java_version("1.8").map(|v| v.major), Some(8));

    // 9 及以后：主版本就是第一段。
    assert_eq!(parse_java_version("21.0.12").map(|v| v.major), Some(21));
    assert_eq!(parse_java_version("25.0.4+101").map(|v| v.major), Some(25));
    assert_eq!(
        parse_java_version("17.0.9+9-LTS").map(|v| v.major),
        Some(17)
    );
    assert_eq!(parse_java_version("21").map(|v| v.major), Some(21));

    // 带引号的形态来自 `java -version` 输出。
    assert_eq!(
        parse_java_version("\"1.8.0_392\"").map(|v| v.major),
        Some(8)
    );

    assert!(parse_java_version("").is_none());
    assert!(parse_java_version("not-a-version").is_none());
}

#[test]
fn release_file_version_is_extracted() {
    let content = "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"25.0.4\"\nOS_ARCH=\"x86_64\"\n";
    assert_eq!(parse_release_file(content).as_deref(), Some("25.0.4"));

    // 不带引号的写法也要支持。
    assert_eq!(
        parse_release_file("JAVA_VERSION=17.0.9\n").as_deref(),
        Some("17.0.9")
    );

    assert!(parse_release_file("IMPLEMENTOR=\"X\"\n").is_none());
    assert!(parse_release_file("").is_none());
}

#[test]
fn java_version_output_is_parsed() {
    let openjdk21 =
        "openjdk version \"21.0.12\" 2025-07-15\nOpenJDK Runtime Environment (build 21.0.12+7)\n";
    assert_eq!(
        parse_java_version_output(openjdk21).map(|v| v.major),
        Some(21)
    );

    let java8 =
        "java version \"1.8.0_392\"\nJava(TM) SE Runtime Environment (build 1.8.0_392-b08)\n";
    assert_eq!(parse_java_version_output(java8).map(|v| v.major), Some(8));

    let temurin25 = "openjdk version \"25.0.4\" 2026-01-20\nOpenJDK Runtime Environment Temurin-25.0.4+101 (build 25.0.4+101-LTS)\n";
    assert_eq!(
        parse_java_version_output(temurin25).map(|v| v.major),
        Some(25)
    );

    assert!(parse_java_version_output("garbage output").is_none());
}

#[test]
fn selection_prefers_exact_match() {
    let installations = vec![installation(17), installation(21), installation(25)];

    let selected = select(&installations, 21).expect("应选中 21");
    assert_eq!(selected.version.major, 21);
}

#[test]
fn selection_falls_back_to_the_lowest_higher_version() {
    let installations = vec![installation(17), installation(21), installation(25)];

    let selected = select(&installations, 19).expect("应选中 21");
    assert_eq!(
        selected.version.major, 21,
        "没有 19 时应选最近的更高版本，而不是 25"
    );
}

#[test]
fn selection_never_downgrades() {
    let installations = vec![installation(8), installation(17)];

    assert!(
        select(&installations, 21).is_none(),
        "只有更低版本时必须返回 None，绝不能向下兼容"
    );
}

#[test]
fn discover_finds_direct_children_and_macos_bundles() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let root = dir.path();

    fake_jdk(root, "jdk-17", "17.0.9");
    fake_jdk(root, "jdk-21", "21.0.12");

    // macOS 的 .jdk 包结构
    let bundle = root.join("temurin-25.jdk");
    fake_jdk(&bundle.join("Contents"), "Home", "25.0.4");

    // 干扰项：不是 JDK 的目录
    std::fs::create_dir_all(root.join("not-a-jdk")).expect("应能创建目录");

    let homes = discover_in(root);
    assert_eq!(homes.len(), 3, "应找到两个普通目录 + 一个 macOS 包");

    let majors: Vec<u32> = homes
        .iter()
        .filter_map(|home| inspect(home, JavaSource::PlatformDirectory))
        .map(|found| found.version.major)
        .collect();
    assert!(majors.contains(&17));
    assert!(majors.contains(&21));
    assert!(majors.contains(&25));
}

#[test]
fn discover_returns_root_when_it_is_already_a_java_home() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let home = fake_jdk(dir.path(), "jdk-21", "21.0.12");

    assert!(is_java_home(&home));
    assert_eq!(discover_in(&home), vec![home]);
}

#[test]
fn inspect_rejects_directories_without_executable() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let home = dir.path().join("broken");
    std::fs::create_dir_all(home.join("bin")).expect("应能创建目录");

    assert!(!is_java_home(&home));
    assert!(inspect(&home, JavaSource::Explicit).is_none());
}

#[test]
fn detect_honours_explicit_paths_first() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let home = fake_jdk(dir.path(), "jdk-21", "21.0.12");

    let installations = detect(std::slice::from_ref(&home), None);
    assert!(!installations.is_empty(), "至少应找到显式指定的那个");

    let first = &installations[0];
    assert_eq!(first.source, JavaSource::Explicit);
    assert_eq!(first.version.major, 21);
    assert_eq!(first.home, home);
}

#[test]
fn detect_deduplicates_the_same_installation() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let home = fake_jdk(dir.path(), "jdk-21", "21.0.12");

    // 同一个路径传两次，模拟「显式指定」与「PATH 反推」指向同一份安装。
    let installations = detect(&[home.clone(), home.clone()], None);
    let matches = installations
        .iter()
        .filter(|found| found.home == home)
        .count();

    assert_eq!(matches, 1, "同一份安装不应重复出现");
}

#[test]
fn detect_scans_managed_root() {
    let dir = TempDir::new().expect("应能创建临时目录");
    let managed = dir.path().join("runtime");
    fake_jdk(&managed, "java-runtime-epsilon", "25.0.4");

    let installations = detect(&[], Some(&managed));
    let managed_hits: Vec<&JavaInstallation> = installations
        .iter()
        .filter(|found| found.source == JavaSource::Managed)
        .collect();

    assert_eq!(managed_hits.len(), 1);
    assert_eq!(managed_hits[0].version.major, 25);
}
