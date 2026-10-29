// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 下载源。
//!
//! Mojang 的下载端点在部分网络下不可达，[BMCLAPI](https://bmclapidoc.bangbang93.com/)
//! 提供了同结构的镜像。这里只做一件事：**把官方地址改写成镜像地址**。
//!
//! 用地址改写而不是在每个调用点各写一套 URL，是因为 URL 的来源太分散——版本清单、
//! 版本 JSON 里的 client jar、libraries 的 artifact、资产对象、Fabric 元数据，
//! 任何一处漏改，用户看到的就是「其他都下好了，卡在某一个文件」。

use std::borrow::Cow;

/// 官方前缀 → 镜像前缀。
///
/// 两处不是简单换主机：
///
/// - `libraries.minecraft.net/<path>` 在镜像上是 `/maven/<path>`
/// - `resources.download.minecraft.net/<xx>/<hash>` 在镜像上是 `/assets/<xx>/<hash>`
///
/// 其余（`piston-meta`、`piston-data`、`launchermeta`、`launcher`）保持路径不变。
/// `piston-data` 是 1.19 之后客户端 jar 所在的主机，文档没写，**实测可用**
/// （`/v1/objects/<sha>/client.jar` 返回 206）。
const RULES: [(&str, &str); 11] = [
    (
        "https://piston-meta.mojang.com/",
        "https://bmclapi2.bangbang93.com/",
    ),
    (
        "https://piston-data.mojang.com/",
        "https://bmclapi2.bangbang93.com/",
    ),
    (
        "https://launchermeta.mojang.com/",
        "https://bmclapi2.bangbang93.com/",
    ),
    (
        "https://launcher.mojang.com/",
        "https://bmclapi2.bangbang93.com/",
    ),
    (
        "https://libraries.minecraft.net/",
        "https://bmclapi2.bangbang93.com/maven/",
    ),
    (
        "https://resources.download.minecraft.net/",
        "https://bmclapi2.bangbang93.com/assets/",
    ),
    (
        "http://resources.download.minecraft.net/",
        "https://bmclapi2.bangbang93.com/assets/",
    ),
    (
        "https://meta.fabricmc.net/",
        "https://bmclapi2.bangbang93.com/fabric-meta/",
    ),
    (
        "https://maven.fabricmc.net/",
        "https://bmclapi2.bangbang93.com/maven/",
    ),
    // Forge 与 NeoForge 的 maven 是**真镜像**（实测：pom 内容与官方 SHA-1 一致，
    // installer jar 经镜像 2.4 MB/s，官方只有约 500 B/s）。
    // 注意 `maven.neoforged.net` 的仓库路径带 `/releases/`，镜像上统一落在 `/maven/`。
    (
        "https://maven.minecraftforge.net/",
        "https://bmclapi2.bangbang93.com/maven/",
    ),
    (
        "https://maven.neoforged.net/releases/",
        "https://bmclapi2.bangbang93.com/maven/",
    ),
];

/// 官方地址 → 镜像地址；不是已知官方主机、或镜像确实不提供的返回 `None`。
///
/// `.sha1` 单独排除：maven 的 `.sha1` 纯文本镜像不提供（实测 `maven.fabricmc.net`
/// 有、`/maven` 下 404），改写过去只会多一次必然失败的请求。影响面很小——
/// 只有 Fabric 的 `fabric-loader` 条目缺摘要时会取它，其余库条目自带摘要。
fn mirror_of(url: &str) -> Option<String> {
    if url.ends_with(".sha1") {
        return None;
    }
    for (official, mirror) in RULES {
        if let Some(rest) = url.strip_prefix(official) {
            return Some(format!("{mirror}{rest}"));
        }
    }
    None
}

/// 下载源。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DownloadSource {
    /// Mojang 官方端点。
    #[default]
    Official,
    /// BMCLAPI 镜像，国内可达性更好。
    Bmclapi,
}

impl DownloadSource {
    /// 把官方地址改写成该源的地址。
    ///
    /// 不认识的主机原样返回（认证、Modrinth 等都不该被改写）。官方源直接返回原值，
    /// 不产生任何分配。
    ///
    /// 前缀比对带上结尾的 `/`：`https://piston-meta.mojang.com.evil.com/` 这种
    /// 相似主机不会被误判成官方域名。
    pub fn rewrite<'a>(self, url: &'a str) -> Cow<'a, str> {
        if self == Self::Official {
            return Cow::Borrowed(url);
        }

        match mirror_of(url) {
            Some(mirror) => Cow::Owned(mirror),
            None => Cow::Borrowed(url),
        }
    }

    /// 依次尝试的地址：优先该源，失败再换另一个。
    ///
    /// **两个方向都回落**：
    /// - 选了镜像：先镜像、再官方——BMCLAPI 的元数据端点会 302 到 CDN 节点
    ///   （`bmclapi*.sakura.sld.tw`），那些节点在某些网络下直接 reset。
    /// - 选了官方：先官方、再镜像——`meta.fabricmc.net` 这类主机在部分网络下
    ///   根本连不通，只给官方地址等于把加载器安装卡死在那里（用户实际遇到过）。
    ///
    /// 不是已知官方主机的地址只返回它自己——认证、Modrinth 这些不该被重试。
    #[must_use]
    pub fn candidates<'a>(self, url: &'a str) -> Vec<Cow<'a, str>> {
        match (self, mirror_of(url)) {
            (Self::Official, Some(mirror)) => vec![Cow::Borrowed(url), Cow::Owned(mirror)],
            (Self::Bmclapi, Some(mirror)) => vec![Cow::Owned(mirror), Cow::Borrowed(url)],
            (_, None) => vec![Cow::Borrowed(url)],
        }
    }

    /// 配置里的字符串形式。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Bmclapi => "bmclapi",
        }
    }

    /// 从配置里的字符串解析，无法识别时回落到官方源。
    ///
    /// 回落而不是报错：配置文件是用户能改的，写错一个值不该让启动器起不来。
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "bmclapi" => Self::Bmclapi,
            _ => Self::Official,
        }
    }
}
