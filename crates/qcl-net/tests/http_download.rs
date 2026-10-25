// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! HTTP 下载实现的测试。
//!
//! 用 wiremock 起本地服务，验证真实网络路径上的行为：截断、错误状态码、哈希不匹配、
//! 以及落地路径越界。这些是假实现无法覆盖的部分。

use std::time::Duration;

use qcl_net::{
    DownloadError, DownloadOps, DownloadOptions, DownloadPlan, DownloadTask, HttpDownloader,
    RetryPolicy, execute_plan, hex_sha1,
};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn task_for(server: &MockServer, relative: &str, body: &[u8]) -> DownloadTask {
    DownloadTask {
        url: format!("{}/{}", server.uri(), relative),
        path: relative.to_owned(),
        sha1: hex_sha1(body),
        size: body.len() as u64,
    }
}

fn downloader() -> (TempDir, HttpDownloader) {
    let dir = TempDir::new().expect("应能创建临时目录");
    let downloader = HttpDownloader::new(dir.path()).expect("应能创建下载器");
    (dir, downloader)
}

#[tokio::test]
async fn downloads_and_verifies_content() {
    let server = MockServer::start().await;
    let body = b"hello minecraft".to_vec();
    Mock::given(method("GET"))
        .and(path("/lib.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let (dir, downloader) = downloader();
    let task = task_for(&server, "lib.jar", &body);

    downloader.fetch_once(&task).await.expect("下载应成功");

    let written = std::fs::read(dir.path().join("lib.jar")).expect("文件应存在");
    assert_eq!(written, body);
    assert!(
        !dir.path().join("lib.jar.part").exists(),
        "临时文件必须被重命名掉，不能残留"
    );
}

#[tokio::test]
async fn rejects_content_that_does_not_match_the_digest() {
    let server = MockServer::start().await;
    let body = b"actual content".to_vec();
    Mock::given(method("GET"))
        .and(path("/lib.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let (dir, downloader) = downloader();
    let mut task = task_for(&server, "lib.jar", &body);
    task.sha1 = hex_sha1(b"expected content");

    let error = downloader
        .fetch_once(&task)
        .await
        .expect_err("哈希不符必须失败");
    assert!(matches!(error, DownloadError::Checksum(_)));
    assert!(error.is_retryable(), "内容损坏应可重试");

    assert!(
        !dir.path().join("lib.jar").exists(),
        "校验失败的文件不得落地"
    );
    assert!(
        !dir.path().join("lib.jar.part").exists(),
        "失败后必须清理临时文件"
    );
}

#[tokio::test]
async fn server_errors_are_marked_retryable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/lib.jar"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let (_dir, downloader) = downloader();
    let task = task_for(&server, "lib.jar", b"x");

    let error = downloader.fetch_once(&task).await.expect_err("503 应失败");
    assert!(error.is_retryable(), "5xx 应可重试");
    assert!(matches!(error, DownloadError::Network(_)));
}

#[tokio::test]
async fn client_errors_are_fatal() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing.jar"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let (_dir, downloader) = downloader();
    let task = task_for(&server, "missing.jar", b"x");

    let error = downloader.fetch_once(&task).await.expect_err("404 应失败");
    assert!(!error.is_retryable(), "404 重试没有意义");
    assert!(matches!(error, DownloadError::Other(_)));
}

#[tokio::test]
async fn existing_valid_file_is_reported_present() {
    let server = MockServer::start().await;
    let body = b"cached content".to_vec();

    let (dir, downloader) = downloader();
    std::fs::write(dir.path().join("lib.jar"), &body).expect("应能预置文件");

    let task = task_for(&server, "lib.jar", &body);
    assert!(downloader.is_present(&task).await);
}

#[tokio::test]
async fn truncated_existing_file_is_reported_missing() {
    let server = MockServer::start().await;
    let body = b"complete content".to_vec();

    let (dir, downloader) = downloader();
    std::fs::write(dir.path().join("lib.jar"), b"complete").expect("应能预置截断文件");

    let task = task_for(&server, "lib.jar", &body);
    assert!(
        !downloader.is_present(&task).await,
        "大小不符的文件必须判为缺失"
    );
}

#[tokio::test]
async fn corrupted_existing_file_is_reported_missing() {
    let server = MockServer::start().await;
    let body = b"good content".to_vec();

    let (dir, downloader) = downloader();
    // 长度相同但内容不同：只有哈希校验能发现。
    std::fs::write(dir.path().join("lib.jar"), b"evil content").expect("应能预置文件");

    let task = task_for(&server, "lib.jar", &body);
    assert!(
        !downloader.is_present(&task).await,
        "同样长度但内容不符，必须靠 SHA-1 拦住"
    );
}

#[tokio::test]
async fn path_traversal_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"x".to_vec()))
        .mount(&server)
        .await;

    let (dir, downloader) = downloader();
    let task = DownloadTask {
        url: format!("{}/evil.jar", server.uri()),
        path: "../escape.jar".to_owned(),
        sha1: hex_sha1(b"x"),
        size: 1,
    };

    let error = downloader
        .fetch_once(&task)
        .await
        .expect_err("越界路径必须被拒绝");
    assert!(matches!(error, DownloadError::UnsafePath(_)));
    assert!(
        !dir.path()
            .parent()
            .expect("临时目录应有父目录")
            .join("escape.jar")
            .exists(),
        "绝不能写到游戏目录之外"
    );
}

#[tokio::test]
async fn absolute_paths_are_rejected() {
    let (_dir, downloader) = downloader();

    // 绝对路径的形态与平台相关：Unix 以 `/` 开头，Windows 带盘符。
    // 注意 `C:/Windows/...` 在 Unix 上只是一个合法的**相对**文件名（冒号是普通字符），
    // 不能拿它做跨平台断言——这正是本测试首轮在三平台 CI 上翻车的原因。
    let absolute = if cfg!(windows) {
        "C:/Windows/system32/evil.dll"
    } else {
        "/etc/passwd"
    };

    let error = downloader
        .resolve(absolute)
        .expect_err("绝对路径必须被拒绝");
    assert!(matches!(error, DownloadError::UnsafePath(_)));

    // 父目录穿越与平台无关。
    assert!(
        downloader.resolve("../escape.jar").is_err(),
        "`..` 穿越必须被拒绝"
    );
}

#[tokio::test]
async fn nested_directories_are_created() {
    let server = MockServer::start().await;
    let body = b"nested".to_vec();
    Mock::given(method("GET"))
        .and(path("/a/b/c.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let (dir, downloader) = downloader();
    let task = task_for(&server, "a/b/c.jar", &body);

    downloader.fetch_once(&task).await.expect("下载应成功");
    assert!(dir.path().join("a/b/c.jar").exists());
}

#[tokio::test]
async fn executor_skips_files_that_are_already_valid() {
    let server = MockServer::start().await;
    let body = b"already here".to_vec();
    // 不挂任何 mock：若执行器真的发起请求会得到 404 并失败。
    let (dir, downloader) = downloader();
    std::fs::write(dir.path().join("lib.jar"), &body).expect("应能预置文件");

    let plan = DownloadPlan {
        tasks: vec![task_for(&server, "lib.jar", &body)],
    };
    let report = execute_plan(
        &plan,
        &DownloadOptions::new(2).with_retry(RetryPolicy::new(
            1,
            Duration::from_millis(1),
            Duration::from_millis(2),
        )),
        std::sync::Arc::new(downloader),
        |_| {},
    )
    .await;

    assert!(report.is_success());
    assert_eq!(report.skipped, 1);
    assert_eq!(report.downloaded, 0);
}
