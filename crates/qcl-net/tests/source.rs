// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 下载源改写。
//!
//! 每一条映射都对应一个真实 URL——写错一个，用户看到的是「其他都下好了，
//! 卡在某一个文件」，而这类问题在日志里只表现为一次 404。

use qcl_net::DownloadSource;

#[test]
fn 官方源不改写也不分配() {
    let url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    assert_eq!(DownloadSource::Official.rewrite(url), url);
    // 官方源必须是零成本路径，否则每次请求都多一次分配。
    assert!(matches!(
        DownloadSource::Official.rewrite(url),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn 元数据与客户端_jar_保持路径只换主机() {
    let cases = [
        (
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json",
        ),
        (
            "https://piston-data.mojang.com/v1/objects/e877b6a07acd633fb3bb475002175cec036e7b87/client.jar",
            "https://bmclapi2.bangbang93.com/v1/objects/e877b6a07acd633fb3bb475002175cec036e7b87/client.jar",
        ),
        (
            "https://launchermeta.mojang.com/mc/game/version_manifest.json",
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json",
        ),
        (
            "https://launcher.mojang.com/v1/objects/abc/client.jar",
            "https://bmclapi2.bangbang93.com/v1/objects/abc/client.jar",
        ),
    ];
    for (official, mirror) in cases {
        assert_eq!(DownloadSource::Bmclapi.rewrite(official), mirror);
    }
}

#[test]
fn 库与资源要加路径前缀() {
    // 这两条不是简单换主机：libraries 走 /maven，assets 走 /assets。
    assert_eq!(
        DownloadSource::Bmclapi.rewrite(
            "https://libraries.minecraft.net/at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
        ),
        "https://bmclapi2.bangbang93.com/maven/at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"
    );
    assert_eq!(
        DownloadSource::Bmclapi.rewrite("https://resources.download.minecraft.net/ab/abcdef"),
        "https://bmclapi2.bangbang93.com/assets/ab/abcdef"
    );
    // 1.7.10 的资源地址是 http，也要认。
    assert_eq!(
        DownloadSource::Bmclapi.rewrite("http://resources.download.minecraft.net/ab/abcdef"),
        "https://bmclapi2.bangbang93.com/assets/ab/abcdef"
    );
}

#[test]
fn fabric_的元数据与_maven() {
    assert_eq!(
        DownloadSource::Bmclapi.rewrite("https://meta.fabricmc.net/v2/versions/loader/26.3"),
        "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/26.3"
    );
    assert_eq!(
        DownloadSource::Bmclapi.rewrite("https://maven.fabricmc.net/net/fabricmc/x/y.jar"),
        "https://bmclapi2.bangbang93.com/maven/net/fabricmc/x/y.jar"
    );
}

#[test]
fn 其他主机原样返回() {
    // 认证、Modrinth 都不该被改写。
    for url in [
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
        "https://api.modrinth.com/v2/search",
        "https://qcl.luoqingciya.cn/releases/latest.json",
    ] {
        assert_eq!(DownloadSource::Bmclapi.rewrite(url), url);
    }
}

#[test]
fn 相似主机不会被误判() {
    // 前缀比对带上结尾的 `/`，所以这些都不算官方域名。
    for url in [
        "https://piston-meta.mojang.com.evil.com/x",
        "https://evil.com/https://piston-meta.mojang.com/x",
        "https://notlibraries.minecraft.net/x",
    ] {
        assert_eq!(DownloadSource::Bmclapi.rewrite(url), url, "{url}");
    }
}

#[test]
fn 配置值可往返且无法识别时回落官方() {
    assert_eq!(DownloadSource::parse("bmclapi"), DownloadSource::Bmclapi);
    assert_eq!(
        DownloadSource::parse(" official "),
        DownloadSource::Official
    );
    // 配置文件是用户能改的，写错一个值不该让启动器起不来。
    assert_eq!(
        DownloadSource::parse("mirror-please"),
        DownloadSource::Official
    );
    assert_eq!(DownloadSource::parse(""), DownloadSource::Official);

    assert_eq!(DownloadSource::Bmclapi.as_str(), "bmclapi");
    assert_eq!(DownloadSource::Official.as_str(), "official");
}

#[test]
fn 镜像启用时官方地址作为兜底排在后面() {
    // 实测 BMCLAPI 的元数据端点会 302 到 CDN，那些节点在某些网络下直接 reset。
    // 只给一个地址的话，用户切到镜像反而连版本列表都拿不到。
    let url = "https://piston-meta.mojang.com/v1/packages/abc/34.json";
    let candidates = DownloadSource::Bmclapi.candidates(url);

    assert_eq!(candidates.len(), 2, "镜像 + 官方兜底");
    assert_eq!(
        candidates[0],
        "https://bmclapi2.bangbang93.com/v1/packages/abc/34.json"
    );
    assert_eq!(candidates[1], url, "兜底必须是原始官方地址");
}

#[test]
fn 官方源只有一个候选() {
    let url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    assert_eq!(DownloadSource::Official.candidates(url).len(), 1);
}

#[test]
fn 非官方主机只有一个候选不会重试() {
    // 认证与 Modrinth 不该因为下载源是镜像就多试一次。
    for url in [
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
        "https://api.modrinth.com/v2/search",
    ] {
        let candidates = DownloadSource::Bmclapi.candidates(url);
        assert_eq!(candidates.len(), 1, "{url}");
        assert_eq!(candidates[0], url);
    }
}

#[test]
fn sha1_文本始终走官方() {
    // 镜像的 maven 下没有 `.sha1`（实测 404），改写过去只会多一次必然失败的请求。
    // 只有 fabric-loader 缺 sha1 时会取它，所以省下的是每次装 Fabric 的一个往返。
    let url = "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar.sha1";

    assert_eq!(DownloadSource::Bmclapi.rewrite(url), url);
    // 只有一个候选，意味着不会先打镜像再回落。
    assert_eq!(DownloadSource::Bmclapi.candidates(url).len(), 1);
}

#[test]
fn sha1_旁边的_jar_仍然走镜像() {
    // 排除的是 `.sha1` 这一种后缀，不是整个 maven 路径。
    let jar =
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar";
    assert_eq!(
        DownloadSource::Bmclapi.rewrite(jar),
        "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
    );
    assert_eq!(DownloadSource::Bmclapi.candidates(jar).len(), 2);
}
