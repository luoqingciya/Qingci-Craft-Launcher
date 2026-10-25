// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 版本清单与元数据获取：走真实 HTTP（wiremock）。
//!
//! 元数据用仓库里的官方 26.3 夹具，因此校验与解析都对着真实数据跑。

use std::path::PathBuf;

use qcl_core::version::{VersionManifest, VersionType};
use qcl_install::{ManifestError, fetch_manifest_from, fetch_meta, resolve_version};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

/// 一份最小清单：两项，`latest.release` 指向**第二项**。
///
/// 这个顺序是刻意的——按 `latest` 指针取而不是取第一项，才能区分两种实现。
fn manifest_json(version_url: &str) -> String {
    format!(
        r#"{{
            "latest": {{ "release": "1.20.1", "snapshot": "26.3" }},
            "versions": [
                {{ "id": "26.3", "type": "snapshot", "url": "{version_url}", "time": "2026-09-15T11:23:02+00:00", "releaseTime": "2026-09-15T11:23:02+00:00" }},
                {{ "id": "1.20.1", "type": "release", "url": "{version_url}", "time": "2023-06-12T00:00:00+00:00", "releaseTime": "2023-06-12T00:00:00+00:00" }}
            ]
        }}"#
    )
}

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

#[tokio::test]
async fn fetches_and_parses_the_manifest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let body = manifest_json("https://example.invalid/v.json");

    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;

    let manifest = fetch_manifest_from(
        &downloader(&dir),
        &format!("{}/manifest.json", server.uri()),
    )
    .await
    .expect("应能获取并解析");

    assert_eq!(manifest.versions.len(), 2);
    assert_eq!(manifest.latest.release, "1.20.1");
    assert_eq!(manifest.versions[0].version_type, VersionType::Snapshot);
}

#[tokio::test]
async fn malformed_manifest_is_reported() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
        .mount(&server)
        .await;

    let error = fetch_manifest_from(
        &downloader(&dir),
        &format!("{}/manifest.json", server.uri()),
    )
    .await
    .expect_err("非 JSON 应失败");

    assert!(matches!(error, ManifestError::Parse(_)), "实际：{error:?}");
}

#[tokio::test]
async fn latest_follows_the_pointer_not_the_list_order() {
    let manifest: VersionManifest =
        serde_json::from_str(&manifest_json("https://example.invalid/v.json")).expect("应可解析");

    let latest = resolve_version(&manifest, "latest").expect("应能找到");
    assert_eq!(latest.id, "1.20.1");
    assert_eq!(latest.version_type, VersionType::Release);
    assert_ne!(
        latest.id, manifest.versions[0].id,
        "若实现取列表首项，这里会失败"
    );
}

#[tokio::test]
async fn explicit_ids_are_resolved_exactly() {
    let manifest: VersionManifest =
        serde_json::from_str(&manifest_json("https://example.invalid/v.json")).expect("应可解析");

    assert_eq!(
        resolve_version(&manifest, "26.3").expect("应能找到").id,
        "26.3"
    );

    let error = resolve_version(&manifest, "9999").expect_err("不存在的版本应报错");
    assert!(matches!(error, ManifestError::NotFound(id) if id == "9999"));
}

#[tokio::test]
async fn fetches_meta_and_verifies_the_digest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let body = fixture("26.3.json");

    Mock::given(method("GET"))
        .and(path("/v/26.3.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;

    let manifest: VersionManifest = serde_json::from_str(&format!(
        r#"{{
            "latest": {{ "release": "26.3", "snapshot": "26.3" }},
            "versions": [{{
                "id": "26.3", "type": "release",
                "url": "{}/v/26.3.json",
                "sha1": "{}",
                "time": "2026-09-15T11:23:02+00:00",
                "releaseTime": "2026-09-15T11:23:02+00:00"
            }}]
        }}"#,
        server.uri(),
        hex_sha1(body.as_bytes())
    ))
    .expect("应可解析");

    let entry = resolve_version(&manifest, "latest").expect("应能找到");
    let meta = fetch_meta(&downloader(&dir), entry)
        .await
        .expect("应能获取");

    assert_eq!(meta.id, "26.3");
    assert_eq!(meta.required_java_major(), Some(25));
    assert_eq!(meta.libraries.len(), 114);
}

#[tokio::test]
async fn digest_mismatch_is_rejected() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let body = fixture("26.3.json");

    Mock::given(method("GET"))
        .and(path("/v/26.3.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let manifest: VersionManifest = serde_json::from_str(&format!(
        r#"{{
            "latest": {{ "release": "26.3", "snapshot": "26.3" }},
            "versions": [{{
                "id": "26.3", "type": "release",
                "url": "{}/v/26.3.json",
                "sha1": "{}",
                "time": "t", "releaseTime": "t"
            }}]
        }}"#,
        server.uri(),
        "0".repeat(40)
    ))
    .expect("应可解析");

    let entry = resolve_version(&manifest, "latest").expect("应能找到");
    let error = fetch_meta(&downloader(&dir), entry)
        .await
        .expect_err("摘要不符必须失败");

    assert!(
        matches!(error, ManifestError::Checksum(_)),
        "实际：{error:?}"
    );
}

#[tokio::test]
async fn meta_without_declared_digest_is_accepted() {
    // 清单 v1 没有 sha1 字段，此时只能信任传输层（HTTPS）。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let body = fixture("26.3.json");

    Mock::given(method("GET"))
        .and(path("/v/26.3.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let manifest: VersionManifest = serde_json::from_str(&format!(
        r#"{{
            "latest": {{ "release": "26.3", "snapshot": "26.3" }},
            "versions": [{{
                "id": "26.3", "type": "release",
                "url": "{}/v/26.3.json",
                "time": "t", "releaseTime": "t"
            }}]
        }}"#,
        server.uri()
    ))
    .expect("应可解析");

    let entry = resolve_version(&manifest, "latest").expect("应能找到");
    assert!(entry.sha1.is_none());
    assert!(fetch_meta(&downloader(&dir), entry).await.is_ok());
}

#[tokio::test]
async fn network_failure_is_reported() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let manifest: VersionManifest = serde_json::from_str(&format!(
        r#"{{
            "latest": {{ "release": "26.3", "snapshot": "26.3" }},
            "versions": [{{
                "id": "26.3", "type": "release",
                "url": "{}/v/26.3.json",
                "time": "t", "releaseTime": "t"
            }}]
        }}"#,
        server.uri()
    ))
    .expect("应可解析");

    let entry = resolve_version(&manifest, "latest").expect("应能找到");
    let error = fetch_meta(&downloader(&dir), entry)
        .await
        .expect_err("500 应失败");

    assert!(
        matches!(error, ManifestError::Download(_)),
        "实际：{error:?}"
    );
}
