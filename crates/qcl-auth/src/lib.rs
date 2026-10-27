// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Qingci Craft Launcher 的微软账号认证。
//!
//! 两条链路：
//!
//! 1. [`device`]：设备码登录，拿到微软访问令牌。用户在自己浏览器里输入短码完成登录，
//!    启动器全程看不到密码。
//! 2. [`xbox`]：把微软令牌换成 Xbox → XSTS → Minecraft 令牌，最后读出 UUID 与玩家名。
//!
//! 设计约束：**不落盘任何令牌**。持久化属于上层（桌面端用系统凭据库），
//! 这样这个 crate 能在无 GUI 的测试里完整跑通，也不会在日志里意外泄露凭据。

pub mod device;
pub mod xbox;

pub use device::{
    DeviceCode, PollOutcome, TOKEN_URL, TokenSet, interpret_token_response,
    is_trusted_verification_uri, poll_once, poll_once_from, poll_until_granted,
    poll_until_granted_from, request_device_code, request_device_code_from,
};
pub use xbox::{
    MinecraftProfile, MinecraftToken, XboxToken, XstsToken, authenticate_xbox,
    authenticate_xbox_from, authorize_xsts, authorize_xsts_from, describe_xsts_error,
    fetch_profile, fetch_profile_from, login_chain, login_minecraft, login_minecraft_from,
};

/// 认证过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// 网络失败。
    #[error("网络错误：{0}")]
    Network(String),

    /// 响应无法解析。
    #[error("响应解析失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// 协议层面的失败（端点返回了非预期状态）。
    #[error("{0}")]
    Protocol(String),

    /// 用户拒绝授权。
    #[error("已取消授权")]
    Declined,

    /// 设备码过期。
    #[error("登录超时，请重新开始")]
    Expired,

    /// Xbox Live 认证失败。
    #[error("Xbox Live 认证失败：{0}")]
    XboxLive(String),

    /// XSTS 授权失败，通常是账号状态问题。
    #[error("{0}")]
    Xsts(String),

    /// Minecraft 服务拒绝。
    #[error("Minecraft 服务拒绝：{0}")]
    Minecraft(String),

    /// 该账号没有购买 Minecraft。
    #[error("该微软账号没有购买 Minecraft Java 版")]
    NoGameOwnership,
}
