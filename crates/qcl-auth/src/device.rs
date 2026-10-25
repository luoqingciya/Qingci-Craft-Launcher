// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 设备码登录（OAuth 2.0 Device Authorization Grant）。
//!
//! 为什么用设备码而不是浏览器回调：启动器不需要注册重定向 URI，也不需要内嵌
//! WebView 抓取回调，用户在自己熟悉的浏览器里登录，令牌不会被启动器看见。
//! 代价是要轮询，而轮询的每一种失败情形都必须处理正确——这部分逻辑单独抽成
//! 纯函数 [`interpret_token_response`]，可以脱离网络逐条测。

use std::time::Duration;

use qcl_net::{DownloadError, HttpDownloader, HttpMethod, RequestBody};
use serde::Deserialize;

use crate::AuthError;

/// 微软消费者账号的设备码端点。
pub const DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";

/// 微软消费者账号的令牌端点。
pub const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

/// 申请的作用域。
///
/// `XboxLive.signin` 用于换取 Xbox 令牌；`offline_access` 才会返回 refresh token，
/// 否则每次启动都要重新登录。
pub const SCOPE: &str = "XboxLive.signin offline_access";

/// 设备码授权信息。
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    /// 轮询用的长串，**不要展示给用户**。
    pub device_code: String,
    /// 用户在浏览器里输入的短码。
    pub user_code: String,
    /// 用户应打开的地址。
    pub verification_uri: String,
    /// 有效期（秒）。
    pub expires_in: u64,
    /// 轮询间隔（秒）。服务端可能要求放慢，见 [`PollOutcome::SlowDown`]。
    pub interval: u64,
    /// 给用户看的完整提示。
    pub message: String,
}

/// 令牌集合。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TokenSet {
    /// 访问令牌。
    pub access_token: String,
    /// 刷新令牌。`offline_access` 未授权时可能缺失。
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 有效期（秒）。
    #[serde(default)]
    pub expires_in: u64,
}

/// 一次轮询的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// 拿到令牌。
    Granted(TokenSet),
    /// 用户还没完成登录，继续等。
    Pending,
    /// 服务端要求放慢轮询。
    SlowDown,
    /// 用户拒绝授权。
    Declined,
    /// 设备码已过期。
    Expired,
    /// 其它不可恢复的错误，带服务端描述。
    Failed(String),
}

/// 设备码响应体（用于解析失败时的错误信息）。
#[derive(Debug, Deserialize)]
struct OAuthErrorBody {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// 申请设备码。
pub async fn request_device_code(
    downloader: &HttpDownloader,
    client_id: &str,
) -> Result<DeviceCode, AuthError> {
    request_device_code_from(downloader, DEVICE_CODE_URL, client_id).await
}

/// 同 [`request_device_code`]，但可指定端点。
pub async fn request_device_code_from(
    downloader: &HttpDownloader,
    url: &str,
    client_id: &str,
) -> Result<DeviceCode, AuthError> {
    let response = downloader
        .request(
            HttpMethod::Post,
            url,
            None,
            RequestBody::Form(&[("client_id", client_id), ("scope", SCOPE)]),
        )
        .await?;

    if !(200..300).contains(&response.status) {
        return Err(AuthError::Protocol(format!(
            "申请设备码失败（HTTP {}）：{}",
            response.status,
            describe_error(&response.body)
        )));
    }

    serde_json::from_str(&response.body).map_err(AuthError::Parse)
}

/// 轮询一次令牌端点。
pub async fn poll_once(
    downloader: &HttpDownloader,
    client_id: &str,
    device_code: &str,
) -> Result<PollOutcome, AuthError> {
    poll_once_from(downloader, TOKEN_URL, client_id, device_code).await
}

/// 同 [`poll_once`]，但可指定端点。
pub async fn poll_once_from(
    downloader: &HttpDownloader,
    url: &str,
    client_id: &str,
    device_code: &str,
) -> Result<PollOutcome, AuthError> {
    let response = downloader
        .request(
            HttpMethod::Post,
            url,
            None,
            RequestBody::Form(&[
                ("client_id", client_id),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ]),
        )
        .await?;

    Ok(interpret_token_response(response.status, &response.body))
}

/// 把令牌端点的响应解释成轮询结果。
///
/// 纯函数：轮询逻辑的正确性全在这里，而它最容易出错的地方是
/// 「把 `authorization_pending` 当成失败」——那样用户还没来得及打开浏览器就报错了。
#[must_use]
pub fn interpret_token_response(status: u16, body: &str) -> PollOutcome {
    if (200..300).contains(&status) {
        return match serde_json::from_str::<TokenSet>(body) {
            Ok(tokens) => PollOutcome::Granted(tokens),
            Err(error) => PollOutcome::Failed(format!("令牌响应无法解析：{error}")),
        };
    }

    let Ok(payload) = serde_json::from_str::<OAuthErrorBody>(body) else {
        return PollOutcome::Failed(format!("HTTP {status}"));
    };

    match payload.error.as_str() {
        "authorization_pending" => PollOutcome::Pending,
        "slow_down" => PollOutcome::SlowDown,
        "authorization_declined" | "access_denied" => PollOutcome::Declined,
        "expired_token" | "code_expired" => PollOutcome::Expired,
        other => PollOutcome::Failed(if payload.error_description.is_empty() {
            other.to_owned()
        } else {
            format!("{other}：{}", payload.error_description)
        }),
    }
}

/// 轮询直到用户完成登录、拒绝或超时。
///
/// `on_pending` 在每次「还在等」时调用，便于界面显示倒计时或提示。
pub async fn poll_until_granted<F>(
    downloader: &HttpDownloader,
    client_id: &str,
    device: &DeviceCode,
    mut on_pending: F,
) -> Result<TokenSet, AuthError>
where
    F: FnMut(),
{
    poll_until_granted_from(downloader, TOKEN_URL, client_id, device, &mut on_pending).await
}

/// 同 [`poll_until_granted`]，但可指定端点。
pub async fn poll_until_granted_from<F>(
    downloader: &HttpDownloader,
    url: &str,
    client_id: &str,
    device: &DeviceCode,
    on_pending: &mut F,
) -> Result<TokenSet, AuthError>
where
    F: FnMut(),
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in);
    // `slow_down` 会让服务端要求更慢的节奏，按规范每次加 5 秒。
    let mut interval = Duration::from_secs(device.interval.max(1));

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(AuthError::Expired);
        }

        match poll_once_from(downloader, url, client_id, &device.device_code).await? {
            PollOutcome::Granted(tokens) => return Ok(tokens),
            PollOutcome::Pending => on_pending(),
            PollOutcome::SlowDown => {
                interval += Duration::from_secs(5);
                on_pending();
            }
            PollOutcome::Declined => return Err(AuthError::Declined),
            PollOutcome::Expired => return Err(AuthError::Expired),
            PollOutcome::Failed(message) => return Err(AuthError::Protocol(message)),
        }

        // 不要睡过截止时间：否则用户看到的是「等完了才报超时」。
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(interval.min(remaining)).await;
    }
}

/// 从 OAuth 错误体里取可读描述。
fn describe_error(body: &str) -> String {
    match serde_json::from_str::<OAuthErrorBody>(body) {
        Ok(payload) if !payload.error_description.is_empty() => {
            format!("{}：{}", payload.error, payload.error_description)
        }
        Ok(payload) => payload.error,
        Err(_) => body.chars().take(200).collect(),
    }
}

impl From<DownloadError> for AuthError {
    fn from(error: DownloadError) -> Self {
        Self::Network(error.to_string())
    }
}
