// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Xbox 认证链：请求体拼装、响应解析与错误归类。
//!
//! 每一跳的失败原因都不同（没有 Xbox 档案、未成年、没买游戏），
//! 因此断言的重点不只是「成功路径能跑」，还有「失败被归到正确的类别」。

use qcl_auth::{
    AuthError, XstsToken, authenticate_xbox_from, authorize_xsts_from, describe_xsts_error,
    fetch_profile_from, login_minecraft_from,
};
use qcl_net::HttpDownloader;
use tempfile::TempDir;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

#[tokio::test]
async fn xbox_authentication_sends_the_required_prefix() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // `RpsTicket` 必须带 `d=` 前缀，漏了会得到 400 而不是明确提示。
    Mock::given(method("POST"))
        .and(path("/user/authenticate"))
        .and(body_string_contains("d=ms-token"))
        .and(body_string_contains("RPS"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"Token":"xbl-token","DisplayClaims":{"xui":[{"uhs":"1"}]}}"#),
        )
        .mount(&server)
        .await;

    let token = authenticate_xbox_from(
        &downloader(&dir),
        &format!("{}/user/authenticate", server.uri()),
        "ms-token",
    )
    .await
    .expect("应能认证");

    assert_eq!(token.token, "xbl-token");
}

#[tokio::test]
async fn xbox_authentication_failure_is_categorised() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/user/authenticate"))
        .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"error":"bad ticket"}"#))
        .mount(&server)
        .await;

    let error = authenticate_xbox_from(
        &downloader(&dir),
        &format!("{}/user/authenticate", server.uri()),
        "bad",
    )
    .await
    .expect_err("应报错");

    assert!(matches!(error, AuthError::XboxLive(_)), "实际：{error:?}");
}

#[tokio::test]
async fn xsts_extracts_token_and_user_hash() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/xsts/authorize"))
        .and(body_string_contains("rp://api.minecraftservices.com/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"Token":"xsts-token","DisplayClaims":{"xui":[{"uhs":"2535410000000000"}]}}"#,
        ))
        .mount(&server)
        .await;

    let xsts = authorize_xsts_from(
        &downloader(&dir),
        &format!("{}/xsts/authorize", server.uri()),
        "xbl-token",
    )
    .await
    .expect("应能授权");

    assert_eq!(xsts.token, "xsts-token");
    assert_eq!(xsts.user_hash, "2535410000000000");
}

#[tokio::test]
async fn xsts_without_user_hash_is_an_error() {
    // 没有 uhs 就拼不出 identityToken，早失败好过带着空串去登录。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/xsts/authorize"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"Token":"xsts-token","DisplayClaims":{"xui":[]}}"#),
        )
        .mount(&server)
        .await;

    let error = authorize_xsts_from(
        &downloader(&dir),
        &format!("{}/xsts/authorize", server.uri()),
        "xbl-token",
    )
    .await
    .expect_err("缺少 uhs 应报错");

    assert!(matches!(error, AuthError::Xsts(_)), "实际：{error:?}");
}

#[test]
fn known_xsts_errors_get_actionable_messages() {
    // 这几个错误码都是「账号状态问题」，用户必须自己处理，报错要能指导下一步。
    let no_profile =
        describe_xsts_error(r#"{"Identity":"0","XErr":2148916233}"#).expect("应能识别");
    assert!(no_profile.contains("xbox.com"), "{no_profile}");

    let child = describe_xsts_error(r#"{"XErr":2148916238}"#).expect("应能识别");
    assert!(child.contains("家庭组"), "{child}");

    let unknown = describe_xsts_error(r#"{"XErr":12345}"#).expect("应能识别");
    assert!(unknown.contains("12345"), "{unknown}");

    assert_eq!(describe_xsts_error("not json"), None);
    assert_eq!(describe_xsts_error(r#"{"Identity":"0"}"#), None);
}

#[tokio::test]
async fn minecraft_login_builds_the_identity_token() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login_with_xbox"))
        .and(body_string_contains("XBL3.0 x=uhs-123;xsts-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"access_token":"mc-token","expires_in":86400}"#),
        )
        .mount(&server)
        .await;

    let xsts = XstsToken {
        token: "xsts-token".to_owned(),
        user_hash: "uhs-123".to_owned(),
    };

    let token = login_minecraft_from(
        &downloader(&dir),
        &format!("{}/login_with_xbox", server.uri()),
        &xsts,
    )
    .await
    .expect("应能登录");

    assert_eq!(token.access_token, "mc-token");
    assert_eq!(token.expires_in, 86_400);
}

#[tokio::test]
async fn profile_requires_a_bearer_token() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/profile"))
        .and(header("authorization", "Bearer mc-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"id":"069a79f4","name":"Notch"}"#),
        )
        .mount(&server)
        .await;

    let profile = fetch_profile_from(
        &downloader(&dir),
        &format!("{}/profile", server.uri()),
        "mc-token",
    )
    .await
    .expect("应能取到档案");

    assert_eq!(profile.name, "Notch");
    assert_eq!(profile.id, "069a79f4");
}

#[tokio::test]
async fn missing_game_is_reported_as_ownership_problem() {
    // 没买游戏的账号在 profile 端点返回 404——这是「不能玩」的信号，
    // 不能笼统报成网络错误。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/profile"))
        .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"error":"NOT_FOUND"}"#))
        .mount(&server)
        .await;

    let error = fetch_profile_from(
        &downloader(&dir),
        &format!("{}/profile", server.uri()),
        "mc-token",
    )
    .await
    .expect_err("应报错");

    assert!(
        matches!(error, AuthError::NoGameOwnership),
        "实际：{error:?}"
    );
}
