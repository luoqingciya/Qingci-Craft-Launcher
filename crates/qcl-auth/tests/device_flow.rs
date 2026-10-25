// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 设备码登录：响应解释、轮询节奏与错误分类。
//!
//! 轮询逻辑最容易出错的地方是「把 `authorization_pending` 当成失败」——
//! 那样用户还没来得及打开浏览器就报错了。所以解释函数是纯函数，逐条覆盖。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use qcl_auth::{
    AuthError, DeviceCode, PollOutcome, interpret_token_response, poll_until_granted_from,
    request_device_code_from,
};
use qcl_net::HttpDownloader;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

fn device_code(interval: u64) -> DeviceCode {
    DeviceCode {
        device_code: "device-abc".to_owned(),
        user_code: "83CBFPQY".to_owned(),
        verification_uri: "https://www.microsoft.com/link".to_owned(),
        expires_in: 60,
        interval,
        message: "打开 https://www.microsoft.com/link 并输入 83CBFPQY".to_owned(),
    }
}

#[test]
fn success_response_yields_tokens() {
    let body = r#"{"access_token":"ms-token","refresh_token":"refresh","expires_in":3600}"#;

    let outcome = interpret_token_response(200, body);

    match outcome {
        PollOutcome::Granted(tokens) => {
            assert_eq!(tokens.access_token, "ms-token");
            assert_eq!(tokens.refresh_token.as_deref(), Some("refresh"));
        }
        other => panic!("应为 Granted，实际：{other:?}"),
    }
}

#[test]
fn pending_is_not_an_error() {
    // 这条如果写成失败，用户根本没机会完成登录。
    let body = r#"{"error":"authorization_pending","error_description":"waiting"}"#;
    assert_eq!(interpret_token_response(400, body), PollOutcome::Pending);
}

#[test]
fn slow_down_is_reported_separately() {
    // 服务端限流要求放慢，与「还没完成」是两回事：前者要加间隔，后者按原间隔继续。
    let body = r#"{"error":"slow_down"}"#;
    assert_eq!(interpret_token_response(400, body), PollOutcome::SlowDown);
}

#[test]
fn declined_and_expired_are_distinguished() {
    assert_eq!(
        interpret_token_response(400, r#"{"error":"authorization_declined"}"#),
        PollOutcome::Declined
    );
    assert_eq!(
        interpret_token_response(400, r#"{"error":"access_denied"}"#),
        PollOutcome::Declined
    );
    assert_eq!(
        interpret_token_response(400, r#"{"error":"expired_token"}"#),
        PollOutcome::Expired
    );
}

#[test]
fn unknown_errors_keep_the_server_description() {
    let body = r#"{"error":"invalid_client","error_description":"AADSTS7000218"}"#;

    match interpret_token_response(401, body) {
        PollOutcome::Failed(message) => {
            assert!(message.contains("invalid_client"), "{message}");
            assert!(message.contains("AADSTS7000218"), "{message}");
        }
        other => panic!("应为 Failed，实际：{other:?}"),
    }
}

#[test]
fn non_json_error_body_still_reports_status() {
    match interpret_token_response(502, "<html>bad gateway</html>") {
        PollOutcome::Failed(message) => assert!(message.contains("502"), "{message}"),
        other => panic!("应为 Failed，实际：{other:?}"),
    }
}

#[tokio::test]
async fn request_device_code_parses_official_shape() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 与真实响应同形（字段名与嵌套都照抄）。
    let body = r#"{
        "user_code":"83CBFPQY",
        "device_code":"device-abc",
        "verification_uri":"https://www.microsoft.com/link",
        "expires_in":900,
        "interval":5,
        "message":"To sign in, use a web browser to open the page https://www.microsoft.com/link and enter the code 83CBFPQY to authenticate."
    }"#;

    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let device = request_device_code_from(
        &downloader(&dir),
        &format!("{}/devicecode", server.uri()),
        "client-id",
    )
    .await
    .expect("应能申请设备码");

    assert_eq!(device.user_code, "83CBFPQY");
    assert_eq!(device.device_code, "device-abc");
    assert_eq!(device.interval, 5);
    assert_eq!(device.expires_in, 900);
}

#[tokio::test]
async fn request_device_code_reports_protocol_errors() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/devicecode"))
        .respond_with(ResponseTemplate::new(400).set_body_string(
            r#"{"error":"invalid_client","error_description":"client not found"}"#,
        ))
        .mount(&server)
        .await;

    let error = request_device_code_from(
        &downloader(&dir),
        &format!("{}/devicecode", server.uri()),
        "bad-client",
    )
    .await
    .expect_err("应报错");

    match error {
        AuthError::Protocol(message) => {
            assert!(message.contains("invalid_client"), "{message}");
            assert!(message.contains("client not found"), "{message}");
        }
        other => panic!("应为 Protocol，实际：{other:?}"),
    }
}

#[tokio::test]
async fn polling_waits_through_pending_until_granted() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 前两次「还在等」，第三次成功。优先级数字越小越先匹配。
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string(r#"{"error":"authorization_pending"}"#),
        )
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"ms-token","expires_in":3600}"#),
        )
        .with_priority(2)
        .mount(&server)
        .await;

    let pending_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&pending_calls);

    let tokens = poll_until_granted_from(
        &downloader(&dir),
        &format!("{}/token", server.uri()),
        "client-id",
        &device_code(1),
        &mut || {
            counter.fetch_add(1, Ordering::Relaxed);
        },
    )
    .await
    .expect("最终应拿到令牌");

    assert_eq!(tokens.access_token, "ms-token");
    assert_eq!(
        pending_calls.load(Ordering::Relaxed),
        2,
        "两次 pending 都应回调，而不是直接报错"
    );
}

#[tokio::test]
async fn polling_stops_when_the_user_declines() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string(r#"{"error":"authorization_declined"}"#),
        )
        .mount(&server)
        .await;

    let error = poll_until_granted_from(
        &downloader(&dir),
        &format!("{}/token", server.uri()),
        "client-id",
        &device_code(1),
        &mut || {},
    )
    .await
    .expect_err("拒绝应终止轮询");

    assert!(matches!(error, AuthError::Declined), "实际：{error:?}");
}
