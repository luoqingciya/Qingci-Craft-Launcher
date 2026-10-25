//! natives 解压：真实 zip 与真实磁盘。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use qcl_core::version::{LaunchContext, VersionMeta};
use qcl_install::{NativeError, extract_natives, install_natives};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "qcl-core",
        "tests",
        "fixtures",
    ]
    .iter()
    .collect::<PathBuf>()
    .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取夹具 {} 失败：{error}", path.display()))
}

/// 造一个 zip，条目按传入顺序写入（名称、内容、是否为目录）。
fn make_zip(path: &Path, entries: &[(&str, &[u8], bool)]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("应能创建目录");
    }
    let file = std::fs::File::create(path).expect("应能创建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for (name, bytes, is_dir) in entries {
        if *is_dir {
            writer
                .add_directory(*name, SimpleFileOptions::default())
                .expect("应能写入目录条目");
        } else {
            writer.start_file(*name, options).expect("应能开始写入条目");
            writer.write_all(bytes).expect("应能写入内容");
        }
    }

    writer.finish().expect("应能完成 zip");
}

fn windows_context() -> LaunchContext {
    LaunchContext {
        os_name: Some("windows".to_owned()),
        arch: Some("x86_64".to_owned()),
        os_version: None,
        features: BTreeMap::new(),
    }
}

#[test]
fn extracts_files_and_honours_exclude_prefixes() {
    let dir = TempDir::new().expect("临时目录");
    let archive = dir.path().join("natives.jar");
    make_zip(
        &archive,
        &[
            ("META-INF/MANIFEST.MF", b"manifest", false),
            ("META-INF/", b"", true),
            ("lwjgl64.dll", b"lwjgl-binary", false),
            ("OpenAL64.dll", b"openal-binary", false),
        ],
    );

    let target = dir.path().join("natives");
    let outcome = extract_natives(&archive, &target, &["META-INF/".to_owned()]).expect("应能解压");

    assert_eq!(outcome.extracted, 2);
    assert_eq!(outcome.excluded, 1, "目录条目不计入排除统计");
    assert_eq!(outcome.unsafe_entries, 0);
    assert_eq!(
        std::fs::read(target.join("lwjgl64.dll")).expect("应已解压"),
        b"lwjgl-binary"
    );
    assert!(!target.join("META-INF").exists());
}

#[test]
fn nested_paths_are_preserved() {
    let dir = TempDir::new().expect("临时目录");
    let archive = dir.path().join("natives.jar");
    make_zip(&archive, &[("linux/x64/liblwjgl.so", b"elf", false)]);

    let target = dir.path().join("natives");
    extract_natives(&archive, &target, &[]).expect("应能解压");

    assert_eq!(
        std::fs::read(target.join("linux").join("x64").join("liblwjgl.so")).expect("应已解压"),
        b"elf"
    );
}

#[test]
fn path_traversal_entries_are_skipped() {
    let dir = TempDir::new().expect("临时目录");
    let archive = dir.path().join("natives.jar");
    make_zip(
        &archive,
        &[
            ("../escaped.dll", b"evil", false),
            ("good.dll", b"good", false),
        ],
    );

    let target = dir.path().join("natives");
    let outcome = extract_natives(&archive, &target, &[]).expect("应能解压");

    assert_eq!(outcome.extracted, 1);
    assert_eq!(outcome.unsafe_entries, 1);
    assert!(
        !dir.path().join("escaped.dll").exists(),
        "文件不得逃出 natives 目录"
    );
    assert_eq!(
        std::fs::read(target.join("good.dll")).expect("合法条目应解压"),
        b"good"
    );
}

#[test]
fn absolute_path_entries_are_skipped() {
    let dir = TempDir::new().expect("临时目录");
    let archive = dir.path().join("natives.jar");
    make_zip(&archive, &[("/tmp/evil.so", b"evil", false)]);

    let target = dir.path().join("natives");
    let outcome = extract_natives(&archive, &target, &[]).expect("应能解压");

    assert_eq!(outcome.extracted, 0);
    assert_eq!(outcome.unsafe_entries, 1);
}

#[test]
fn existing_files_are_overwritten() {
    let dir = TempDir::new().expect("临时目录");
    let archive = dir.path().join("natives.jar");
    make_zip(&archive, &[("lwjgl64.dll", b"new-content", false)]);

    let target = dir.path().join("natives");
    std::fs::create_dir_all(&target).expect("应能创建目录");
    std::fs::write(target.join("lwjgl64.dll"), b"old-content-longer").expect("应能写旧文件");

    extract_natives(&archive, &target, &[]).expect("应能解压");

    assert_eq!(
        std::fs::read(target.join("lwjgl64.dll")).expect("应已覆盖"),
        b"new-content"
    );
}

#[test]
fn modern_version_needs_no_natives() {
    // 26.3 的 natives 由 LWJGL 3 自己在运行时解压，启动器不介入。
    let dir = TempDir::new().expect("临时目录");
    let meta: VersionMeta = serde_json::from_str(&fixture("26.3.json")).expect("应可解析");

    let report = install_natives(dir.path(), &meta, &windows_context(), "versions/26.3")
        .expect("无需解压也应成功");

    assert_eq!(report, Default::default());
    assert!(
        !dir.path().join("versions/26.3/natives").exists(),
        "没有 natives 时不应创建空目录"
    );
}

#[test]
fn legacy_version_extracts_into_natives_dir() {
    let dir = TempDir::new().expect("临时目录");
    let meta: VersionMeta = serde_json::from_str(
        r#"{
            "id": "test",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "releaseTime": "2014-05-14T14:29:00+00:00",
            "time": "2014-05-14T14:29:00+00:00",
            "libraries": [
                {
                    "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                    "natives": { "windows": "natives-windows-${arch}" },
                    "extract": { "exclude": ["META-INF/"] },
                    "downloads": {
                        "classifiers": {
                            "natives-windows-64": {
                                "path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows-64.jar",
                                "sha1": "0000000000000000000000000000000000000000",
                                "size": 100,
                                "url": "https://example.invalid/lwjgl.jar"
                            }
                        }
                    }
                }
            ]
        }"#,
    )
    .expect("应可解析");

    // 下载阶段会把归档放在 artifact.path 指定的位置（含 libraries/ 前缀）。
    let archive = dir.path().join(
        "libraries/org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows-64.jar",
    );
    make_zip(
        &archive,
        &[
            ("META-INF/MANIFEST.MF", b"m", false),
            ("lwjgl64.dll", b"lwjgl", false),
        ],
    );

    let report =
        install_natives(dir.path(), &meta, &windows_context(), "versions/test").expect("应能安装");

    assert_eq!(report.archives, 1);
    assert_eq!(report.extracted, 1);
    assert_eq!(report.excluded, 1);
    assert_eq!(
        std::fs::read(dir.path().join("versions/test/natives/lwjgl64.dll"))
            .expect("应解压到 natives 目录"),
        b"lwjgl"
    );
}

#[test]
fn missing_archive_is_reported() {
    let dir = TempDir::new().expect("临时目录");
    let meta: VersionMeta = serde_json::from_str(
        r#"{
            "id": "test",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "releaseTime": "2014-05-14T14:29:00+00:00",
            "time": "2014-05-14T14:29:00+00:00",
            "libraries": [
                {
                    "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                    "natives": { "windows": "natives-windows" },
                    "downloads": {
                        "classifiers": {
                            "natives-windows": {
                                "path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-windows.jar",
                                "sha1": "0000000000000000000000000000000000000000",
                                "size": 100,
                                "url": "https://example.invalid/lwjgl.jar"
                            }
                        }
                    }
                }
            ]
        }"#,
    )
    .expect("应可解析");

    let error = install_natives(dir.path(), &meta, &windows_context(), "versions/test")
        .expect_err("缺少归档必须失败");

    assert!(
        matches!(error, NativeError::MissingArchive(_)),
        "实际：{error:?}"
    );
}
