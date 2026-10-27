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
const RULES: [(&str, &str); 9] = [
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
];

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

        for (official, mirror) in RULES {
            if let Some(rest) = url.strip_prefix(official) {
                return Cow::Owned(format!("{mirror}{rest}"));
            }
        }

        Cow::Borrowed(url)
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
