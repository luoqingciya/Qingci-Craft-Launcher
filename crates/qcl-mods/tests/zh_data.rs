// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 译名库的远端拉取。
//!
//! 三张表是进程级全局状态，所以这些场景写在**同一个测试函数**里按顺序跑：
//! 拆成多个 `#[tokio::test]` 会并行执行、互相覆盖。

use qcl_mods::zh_names;
use qcl_net::{HttpDownloader, hex_sha256};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

/// 挂一个「详情 + 文件」都正常的制品。
async fn mount_package(server: &MockServer, name: &str, file: &str, body: &str) {
    let digest = hex_sha256(body.as_bytes());
    let download_path = format!("/packages/ab/{digest}/{file}");

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/packages/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "files": [{ "download_url": download_path, "sha256": digest }]
        })))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(&download_path))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn refresh_fetches_verifies_and_caches() {
    let dir = TempDir::new().expect("临时目录");

    // ── 场景 1：三份都正常，应写缓存并让表立刻可用 ──
    let server = MockServer::start().await;
    mount_package(&server, "mod-zh", "mod-zh.json", r#"{"sodium":"钠"}"#).await;
    mount_package(
        &server,
        "mod-search-aliases",
        "mod-search-aliases.json",
        r#"{"钠":"sodium"}"#,
    )
    .await;
    mount_package(
        &server,
        "mod-categories-zh",
        "mod-categories-zh.json",
        r#"{"optimization":"优化"}"#,
    )
    .await;

    let outcome = zh_names::refresh(&downloader(&dir), &server.uri(), dir.path())
        .await
        .expect("应能刷新");

    assert_eq!(outcome.names, 1);
    assert_eq!(outcome.aliases, 1);
    assert_eq!(outcome.categories, 1);

    // 落盘了，下次启动不必再联网。
    let cache = zh_names::cache_dir(dir.path());
    assert!(cache.join("mod-zh.json").is_file());
    assert!(zh_names::cache_is_fresh(dir.path()), "刚写完的缓存应算新鲜");

    // 内存表跟着换了，不必重启。
    assert_eq!(zh_names::zh_name("sodium").as_deref(), Some("钠"));
    assert_eq!(zh_names::resolve_query("钠"), "sodium");
    assert_eq!(
        zh_names::category_label("optimization").as_deref(),
        Some("优化")
    );

    // ── 场景 2：摘要不符，应整次失败且**不动**已有缓存 ──
    let bad = MockServer::start().await;
    let digest = hex_sha256(b"real");
    Mock::given(method("GET"))
        .and(path("/api/v1/packages/mod-zh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "files": [{ "download_url": "/packages/ab/deadbeef/mod-zh.json", "sha256": digest }]
        })))
        .mount(&bad)
        .await;
    Mock::given(method("GET"))
        .and(path("/packages/ab/deadbeef/mod-zh.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"sodium":"假"}"#))
        .mount(&bad)
        .await;

    let error = zh_names::refresh(&downloader(&dir), &bad.uri(), dir.path())
        .await
        .expect_err("摘要不符必须失败");
    assert!(error.to_string().contains("SHA-256"), "实际：{error}");
    // 缓存没被污染：还是上一次那份。
    assert_eq!(zh_names::zh_name("sodium").as_deref(), Some("钠"));

    // ── 场景 3：空表（`{}`）应被拒绝，否则等于把译名全删了 ──
    let empty = MockServer::start().await;
    mount_package(&empty, "mod-zh", "mod-zh.json", "{}").await;

    let error = zh_names::refresh(&downloader(&dir), &empty.uri(), dir.path())
        .await
        .expect_err("空表必须拒绝");
    assert!(error.to_string().contains("空"), "实际：{error}");
    assert_eq!(zh_names::zh_name("sodium").as_deref(), Some("钠"));

    // ── 场景 4：详情接口挂了，应报错而不是写半份 ──
    let down = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/packages/mod-zh"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&down)
        .await;

    assert!(
        zh_names::refresh(&downloader(&dir), &down.uri(), dir.path())
            .await
            .is_err()
    );
    assert_eq!(zh_names::zh_name("sodium").as_deref(), Some("钠"));
}
