// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Modrinth 搜索、文件解析与安装。
//!
//! 夹具是 2026-10-26 从 `api.modrinth.com` 拉到的真实响应，字段名照抄。

use std::sync::Arc;

use qcl_mods::{ModError, install, resolve_file_from, search_from};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SEARCH_BODY: &str = r#"{
  "hits": [
    {
      "project_id": "AANobbMI",
      "project_type": "mod",
      "slug": "sodium",
      "author": "jellysquid3",
      "title": "Sodium",
      "description": "A high-performance rendering engine replacement for Minecraft.",
      "downloads": 12345678
    }
  ],
  "total_hits": 32
}"#;

const VERSIONS_BODY: &str = r#"[
  {
    "id": "abc123",
    "version_number": "mc26.3-0.9.3-beta.1-fabric",
    "files": [
      {
        "url": "https://cdn.modrinth.com/sources.jar",
        "filename": "sodium-sources.jar",
        "primary": false,
        "size": 100,
        "hashes": { "sha1": "0000000000000000000000000000000000000000" }
      },
      {
        "url": "https://cdn.modrinth.com/sodium.jar",
        "filename": "sodium-fabric-0.9.3-beta.1+mc26.3.jar",
        "primary": true,
        "size": 1912496,
        "hashes": { "sha1": "1e6aec741b4134258ba1d7d458bae96a2e8e187e" }
      }
    ]
  }
]"#;

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

#[tokio::test]
async fn search_sends_version_and_loader_facets() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .mount(&server)
        .await;

    let results = search_from(
        &downloader(&dir),
        &server.uri(),
        "sodium",
        "26.3",
        "fabric",
        10,
    )
    .await
    .expect("应能搜索");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Sodium");
    assert_eq!(results[0].project_id, "AANobbMI");

    // facets 必须带上游戏版本与加载器，否则会搜到不兼容的模组。
    let requests = server.received_requests().await.expect("应能读到请求");
    let url = requests[0].url.as_str().to_string();
    // 冒号在查询值里合法，不转义；方括号与引号必须转义，否则 facets 的 JSON 会破坏 URL。
    assert!(
        url.contains("%5B%5B%22project_type:mod%22%5D"),
        "实际 URL：{url}"
    );
    assert!(url.contains("%22versions:26.3%22"), "实际 URL：{url}");
    assert!(
        url.contains("%22categories:fabric%22%5D"),
        "实际 URL：{url}"
    );
}

#[tokio::test]
async fn resolve_file_prefers_the_primary_file() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSIONS_BODY))
        .mount(&server)
        .await;

    let file = resolve_file_from(
        &downloader(&dir),
        &server.uri(),
        "AANobbMI",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能解析");

    // 一个版本可能带多个文件（源码包、dev 包），必须挑 primary 那个。
    assert_eq!(file.filename, "sodium-fabric-0.9.3-beta.1+mc26.3.jar");
    assert_eq!(file.size, 1_912_496);
    assert_eq!(file.sha1.len(), 40);
}

#[tokio::test]
async fn resolve_file_reports_missing_compatibility() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/X/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let error = resolve_file_from(&downloader(&dir), &server.uri(), "X", "26.3", "fabric")
        .await
        .expect_err("空列表应报错");

    assert!(
        matches!(error, ModError::NoCompatibleFile(_)),
        "实际：{error:?}"
    );
}

#[tokio::test]
async fn resolve_file_rejects_missing_digest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 没有摘要就没法校验，宁可失败也不装一个来路不明的 jar。
    let body = VERSIONS_BODY.replace("1e6aec741b4134258ba1d7d458bae96a2e8e187e", "");
    Mock::given(method("GET"))
        .and(path("/project/X/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let error = resolve_file_from(&downloader(&dir), &server.uri(), "X", "26.3", "fabric")
        .await
        .expect_err("缺摘要应报错");

    assert!(matches!(error, ModError::Download(_)), "实际：{error:?}");
}

#[tokio::test]
async fn install_writes_into_mods_and_verifies_digest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    let body = b"fake-jar-bytes".to_vec();
    let digest = hex_sha1(&body);

    Mock::given(method("GET"))
        .and(path("/sodium.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let file = qcl_mods::ModFile {
        project_id: "AANobbMI".to_owned(),
        version_number: "1.0".to_owned(),
        filename: "sodium.jar".to_owned(),
        url: format!("{}/sodium.jar", server.uri()),
        sha1: digest,
        size: body.len() as u64,
    };

    let downloader = Arc::new(downloader(&dir));
    let path = install(Arc::clone(&downloader), &file)
        .await
        .expect("应能安装");

    assert_eq!(path, "mods/sodium.jar");
    assert!(dir.path().join("mods/sodium.jar").is_file());
}

#[tokio::test]
async fn install_rejects_unsafe_filenames() {
    let dir = TempDir::new().expect("临时目录");
    let downloader = Arc::new(downloader(&dir));

    // 远端文件名不能假设规矩：接受 `../` 就会写到游戏目录之外。
    for name in [
        "../evil.jar",
        "a/b.jar",
        "a\\b.jar",
        "..",
        ".hidden.jar",
        "  ",
    ] {
        let file = qcl_mods::ModFile {
            project_id: "X".to_owned(),
            version_number: "1".to_owned(),
            filename: name.to_owned(),
            url: "http://127.0.0.1:1/x.jar".to_owned(),
            sha1: "0".repeat(40),
            size: 1,
        };

        let error = install(Arc::clone(&downloader), &file)
            .await
            .expect_err("不安全文件名应被拒绝");
        assert!(
            matches!(error, ModError::UnsafeFilename(_)),
            "{name} 应被拒绝，实际：{error:?}"
        );
    }
}

#[tokio::test]
#[ignore = "需要访问 api.modrinth.com"]
async fn live_search_and_resolve() {
    // 对着真实上游跑一遍：搜索 → 解析到具体文件。
    // 只验证「能拿到适配 26.3 + Fabric 的文件」，不下载（几 MB，没必要）。
    let dir = TempDir::new().expect("临时目录");
    let downloader = downloader(&dir);

    let hits = qcl_mods::search(&downloader, "sodium", "26.3", "fabric", 5)
        .await
        .expect("应能搜索");
    assert!(!hits.is_empty(), "26.3 + Fabric 下应有 sodium 相关结果");

    let sodium = hits
        .iter()
        .find(|hit| hit.title.eq_ignore_ascii_case("sodium"))
        .expect("应能找到 Sodium");

    let file = qcl_mods::resolve_file(&downloader, &sodium.project_id, "26.3", "fabric")
        .await
        .expect("应能解析出文件");

    assert!(file.filename.ends_with(".jar"), "实际：{}", file.filename);
    assert_eq!(file.sha1.len(), 40, "摘要应是完整 SHA-1");
    assert!(file.size > 0, "大小应大于 0");
    assert!(file.url.starts_with("https://"), "下载地址应走 HTTPS");
}
