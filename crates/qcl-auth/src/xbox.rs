// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Xbox 认证链：Xbox Live → XSTS → Minecraft。
//!
//! 微软账号的令牌不能直接用于 Minecraft，必须走三跳：
//!
//! ```text
//! MS access token
//!   → user.auth.xboxlive.com   → XBL token
//!   → xsts.auth.xboxlive.com   → XSTS token + user hash
//!   → api.minecraftservices.com/authentication/login_with_xbox → Minecraft token
//!   → api.minecraftservices.com/minecraft/profile              → UUID + 名字
//! ```
//!
//! 每一跳的失败原因都不一样（没有 Xbox 账号、未成年账号、没买游戏），
//! 因此错误必须按跳归类，而不是笼统报「登录失败」——用户看到
//! 「该账号没有 Xbox 档案」才知道下一步该做什么。

use qcl_core::launch::AccountInfo;
use qcl_net::{HttpDownloader, HttpMethod, RequestBody};
use serde::Deserialize;
use serde_json::json;

use crate::AuthError;

/// Xbox Live 用户认证端点。
pub const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";

/// XSTS 授权端点。
pub const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";

/// Minecraft 登录端点。
pub const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";

/// Minecraft 档案端点。
pub const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

/// XSTS 的依赖方标识，必须是这个值 Minecraft 才认。
const XSTS_RELYING_PARTY: &str = "rp://api.minecraftservices.com/";

/// Xbox Live 令牌。
#[derive(Debug, Clone, Deserialize)]
pub struct XboxToken {
    /// 令牌本体。
    #[serde(rename = "Token")]
    pub token: String,
}

/// XSTS 授权结果。
#[derive(Debug, Clone)]
pub struct XstsToken {
    /// 令牌本体。
    pub token: String,
    /// 用户哈希，拼 `XBL3.0 x=<uhs>;<token>` 时要用。
    pub user_hash: String,
}

/// Minecraft 令牌。
#[derive(Debug, Clone, Deserialize)]
pub struct MinecraftToken {
    /// 访问令牌。
    pub access_token: String,
    /// 有效期（秒）。
    #[serde(default)]
    pub expires_in: u64,
}

/// Minecraft 档案。
#[derive(Debug, Clone, Deserialize)]
pub struct MinecraftProfile {
    /// 账号 UUID（无连字符）。
    pub id: String,
    /// 玩家名。
    pub name: String,
}

/// 用微软访问令牌换 Xbox Live 令牌。
pub async fn authenticate_xbox(
    downloader: &HttpDownloader,
    ms_access_token: &str,
) -> Result<XboxToken, AuthError> {
    authenticate_xbox_from(downloader, XBL_URL, ms_access_token).await
}

/// 同 [`authenticate_xbox`]，但可指定端点。
pub async fn authenticate_xbox_from(
    downloader: &HttpDownloader,
    url: &str,
    ms_access_token: &str,
) -> Result<XboxToken, AuthError> {
    let body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            // 前缀 `d=` 是规范要求，漏了会得到 400 而不是明确提示。
            "RpsTicket": format!("d={ms_access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });

    let response = downloader
        .request(HttpMethod::Post, url, None, RequestBody::Json(&body))
        .await?;

    if !(200..300).contains(&response.status) {
        return Err(AuthError::XboxLive(format!(
            "HTTP {}：{}",
            response.status,
            truncate(&response.body)
        )));
    }

    serde_json::from_str(&response.body).map_err(AuthError::Parse)
}

/// 用 Xbox Live 令牌换 XSTS 令牌。
pub async fn authorize_xsts(
    downloader: &HttpDownloader,
    xbl_token: &str,
) -> Result<XstsToken, AuthError> {
    authorize_xsts_from(downloader, XSTS_URL, xbl_token).await
}

/// 同 [`authorize_xsts`]，但可指定端点。
pub async fn authorize_xsts_from(
    downloader: &HttpDownloader,
    url: &str,
    xbl_token: &str,
) -> Result<XstsToken, AuthError> {
    let body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": XSTS_RELYING_PARTY,
        "TokenType": "JWT",
    });

    let response = downloader
        .request(HttpMethod::Post, url, None, RequestBody::Json(&body))
        .await?;

    if !(200..300).contains(&response.status) {
        // XSTS 的错误码在 `XErr` 里，直接给出可读解释比原样抛 HTTP 401 有用得多。
        if let Some(message) = describe_xsts_error(&response.body) {
            return Err(AuthError::Xsts(message));
        }
        return Err(AuthError::Xsts(format!(
            "HTTP {}：{}",
            response.status,
            truncate(&response.body)
        )));
    }

    let payload: XstsResponse = serde_json::from_str(&response.body).map_err(AuthError::Parse)?;
    let user_hash = payload
        .display_claims
        .and_then(|claims| claims.xui.into_iter().next())
        .and_then(|claim| {
            claim
                .get("uhs")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
        .ok_or_else(|| AuthError::Xsts("响应里没有用户哈希（uhs）".to_owned()))?;

    Ok(XstsToken {
        token: payload.token,
        user_hash,
    })
}

/// 用 XSTS 令牌登录 Minecraft。
pub async fn login_minecraft(
    downloader: &HttpDownloader,
    xsts: &XstsToken,
) -> Result<MinecraftToken, AuthError> {
    login_minecraft_from(downloader, MC_LOGIN_URL, xsts).await
}

/// 同 [`login_minecraft`]，但可指定端点。
pub async fn login_minecraft_from(
    downloader: &HttpDownloader,
    url: &str,
    xsts: &XstsToken,
) -> Result<MinecraftToken, AuthError> {
    let body = json!({
        "identityToken": format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token),
    });

    let response = downloader
        .request(HttpMethod::Post, url, None, RequestBody::Json(&body))
        .await?;

    if !(200..300).contains(&response.status) {
        return Err(AuthError::Minecraft(format!(
            "HTTP {}：{}",
            response.status,
            truncate(&response.body)
        )));
    }

    serde_json::from_str(&response.body).map_err(AuthError::Parse)
}

/// 读取 Minecraft 档案（UUID 与玩家名）。
///
/// 这一步还有一个隐含作用：**没买游戏的账号在这里会 404**，
/// 因此它同时充当「这个账号能不能玩」的检查。
pub async fn fetch_profile(
    downloader: &HttpDownloader,
    access_token: &str,
) -> Result<MinecraftProfile, AuthError> {
    fetch_profile_from(downloader, MC_PROFILE_URL, access_token).await
}

/// 同 [`fetch_profile`]，但可指定端点。
pub async fn fetch_profile_from(
    downloader: &HttpDownloader,
    url: &str,
    access_token: &str,
) -> Result<MinecraftProfile, AuthError> {
    let response = downloader
        .request(HttpMethod::Get, url, Some(access_token), RequestBody::Empty)
        .await?;

    match response.status {
        200..=299 => serde_json::from_str(&response.body).map_err(AuthError::Parse),
        404 => Err(AuthError::NoGameOwnership),
        status => Err(AuthError::Minecraft(format!(
            "HTTP {status}：{}",
            truncate(&response.body)
        ))),
    }
}

/// 走完整条认证链，返回可直接用于启动的账号信息。
pub async fn login_chain(
    downloader: &HttpDownloader,
    ms_access_token: &str,
) -> Result<AccountInfo, AuthError> {
    let xbl = authenticate_xbox(downloader, ms_access_token).await?;
    let xsts = authorize_xsts(downloader, &xbl.token).await?;
    let mc = login_minecraft(downloader, &xsts).await?;
    let profile = fetch_profile(downloader, &mc.access_token).await?;

    Ok(AccountInfo {
        player_name: profile.name,
        uuid: profile.id,
        access_token: mc.access_token,
        user_type: "msa".to_owned(),
        client_id: None,
        xuid: Some(xsts.user_hash),
    })
}

#[derive(Debug, Deserialize)]
struct XstsResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims", default)]
    display_claims: Option<XstsClaims>,
}

#[derive(Debug, Deserialize)]
struct XstsClaims {
    #[serde(default)]
    xui: Vec<serde_json::Value>,
}

/// 把 XSTS 的 `XErr` 翻译成可操作的提示。
///
/// 这几个错误码对应的是「账号状态问题」，用户必须自己处理（注册 Xbox、加家庭成员、
/// 购买游戏）。原样抛 HTTP 401 的话，用户完全不知道下一步做什么。
#[must_use]
pub fn describe_xsts_error(body: &str) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(body).ok()?;
    let code = payload.get("XErr")?.as_i64()?;

    let message = match code {
        2_148_916_233 => "该微软账号还没有 Xbox 档案，请先在 xbox.com 创建",
        2_148_916_238 => "未成年账号需要先加入家庭组，由家长在 account.microsoft.com 设置",
        2_148_916_243 => "该账号所在国家/地区不受 Xbox Live 支持",
        2_148_916_247 => "该账号的 Xbox 档案不完整，请到 xbox.com 补全",
        _ => return Some(format!("Xbox 授权失败（XErr {code}）")),
    };

    Some(message.to_owned())
}

fn truncate(body: &str) -> String {
    body.chars().take(200).collect()
}
