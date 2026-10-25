// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Fabric 加载器：profile 解析、构件补齐与合并。
//!
//! 夹具是 2026-10-25 从 `meta.fabricmc.net` 原样拉取的官方响应，未作修改。

use std::path::PathBuf;

use qcl_core::version::VersionMeta;
use qcl_install::{
    FabricProfile, LoaderError, fetch_loader_list_from, fetch_profile_from, inherits_from, merge,
    resolve_libraries,
};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PROFILE: &str = include_str!("fixtures/fabric-profile-26.3-0.20.1.json");
const ENTRY: &str = include_str!("fixtures/fabric-entry-26.3-0.20.1.json");

fn fixture(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "qcl-core",
        "tests",
        "fixtures",
    ]
    .iter()
    .collect::<PathBuf>()
    .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取夹具 {} 失败：{error}", path.display()))
}

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

fn profile() -> FabricProfile {
    serde_json::from_str(PROFILE).expect("官方 profile 应可解析")
}

#[test]
fn official_profile_parses() {
    let profile = profile();

    assert_eq!(profile.id, "fabric-loader-0.20.1-26.3");
    assert_eq!(profile.inherits_from, "26.3");
    assert_eq!(
        profile.main_class,
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    );
    assert_eq!(profile.libraries.len(), 7);

    // Fabric 只追加 JVM 参数，游戏参数为空——这条决定了合并时不能覆盖父版本的游戏参数。
    let arguments = profile.arguments.as_ref().expect("应有 arguments");
    assert!(arguments.game.is_empty());
    assert_eq!(arguments.jvm.len(), 1);
}

#[test]
fn inherits_from_matches_the_requested_version() {
    let profile = profile();

    assert!(inherits_from(&profile, "26.3"));
    assert!(!inherits_from(&profile, "1.20.1"));
}

#[test]
fn loader_entry_parses_without_launcher_meta() {
    // 单条目响应里还有 launcherMeta（体积大且与合并无关），只取 loader 字段。
    let entries: Vec<qcl_install::LoaderInfo> =
        serde_json::from_str::<Vec<serde_json::Value>>(&format!("[{ENTRY}]"))
            .expect("应是合法 JSON")
            .into_iter()
            .map(|value| serde_json::from_value(value["loader"].clone()).expect("应有 loader"))
            .collect();

    assert_eq!(entries[0].version, "0.20.1");
    assert!(entries[0].stable);
}

#[test]
fn merge_replaces_main_class_and_appends_libraries() {
    let parent: VersionMeta = serde_json::from_str(&fixture("26.3.json")).expect("应可解析");
    let profile = profile();
    let parent_libraries = parent.libraries.len();

    let merged = merge(&parent, &profile, Vec::new());

    assert_eq!(merged.id, "fabric-loader-0.20.1-26.3");
    assert_eq!(
        merged.main_class,
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    );
    assert_eq!(
        merged.libraries.len(),
        parent_libraries,
        "空库列表时不增不减"
    );

    // 原版的关键信息必须原样保留，否则装不上也跑不起来。
    assert_eq!(
        merged.asset_index.as_ref().map(|i| i.id.as_str()),
        Some("34")
    );
    assert_eq!(merged.required_java_major(), Some(25));
    assert!(
        merged
            .downloads
            .as_ref()
            .and_then(|d| d.client.as_ref())
            .is_some()
    );
    assert!(
        merged
            .arguments
            .as_ref()
            .and_then(|a| a.default_user_jvm.as_ref())
            .is_some()
    );
}

#[test]
fn merge_keeps_parent_game_arguments_before_loader_arguments() {
    let parent: VersionMeta = serde_json::from_str(&fixture("26.3.json")).expect("应可解析");
    let profile = profile();

    let merged = merge(&parent, &profile, Vec::new());

    let parent_game = parent
        .arguments
        .as_ref()
        .expect("父版本应有参数")
        .game
        .len();
    let merged_arguments = merged.arguments.as_ref().expect("合并后应有参数");
    assert_eq!(
        merged_arguments.game.len(),
        parent_game,
        "加载器没有追加游戏参数，数量应与父版本一致"
    );

    let parent_jvm = parent.arguments.as_ref().expect("父版本应有参数").jvm.len();
    assert_eq!(
        merged_arguments.jvm.len(),
        parent_jvm + 1,
        "应追加一条 -DFabricMcEmu"
    );
    assert!(
        merged_arguments.jvm.iter().any(|argument| {
            matches!(argument, qcl_core::version::Argument::Plain(value) if value.contains("FabricMcEmu"))
        }),
        "追加的参数里应有 FabricMcEmu"
    );
}

#[tokio::test]
async fn resolve_libraries_fills_missing_digest_and_size() {
    // 夹具里 fabric-loader 只有 name 与 url，必须回 maven 补摘要与大小。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    let loader_path = "net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar";
    let digest = hex_sha1(b"fabric-loader-bytes");

    Mock::given(method("GET"))
        .and(path(format!("/{loader_path}.sha1")))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("{digest}  fabric-loader.jar")),
        )
        .mount(&server)
        .await;
    // HEAD 在这里拿不到有效长度（mock 对 HEAD 返回 0，真实 CDN 分块传输也不给长度），
    // 因此实际走的是 `Range: bytes=0-0` 兜底，长度从 Content-Range 里读。
    Mock::given(method("GET"))
        .and(path(format!("/{loader_path}")))
        .and(wiremock::matchers::header("range", "bytes=0-0"))
        .respond_with(ResponseTemplate::new(206).append_header("content-range", "bytes 0-0/19"))
        .mount(&server)
        .await;

    // 其余六个库自带 sha1 与大小，不会触发额外请求。
    let mut only_loader = profile();
    only_loader
        .libraries
        .retain(|library| library.name.contains("fabric-loader"));
    for library in &mut only_loader.libraries {
        library.url = Some(format!("{}/", server.uri()));
        library.sha1 = None;
        library.size = None;
    }

    let libraries = resolve_libraries(&downloader(&dir), &only_loader)
        .await
        .expect("应能补齐");

    assert_eq!(libraries.len(), 1);
    let artifact = libraries[0]
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("应有主构件");
    assert_eq!(artifact.sha1, digest);
    assert_eq!(artifact.size, 19);
    assert_eq!(artifact.path.as_deref(), Some(loader_path));
}

#[tokio::test]
async fn resolve_libraries_uses_given_values_without_extra_requests() {
    // 自带 sha1/size 的条目不应再发请求：mock 服务是空的，发了就会失败。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    let mut only_asm = profile();
    only_asm
        .libraries
        .retain(|library| library.name == "org.ow2.asm:asm:9.10.1");
    for library in &mut only_asm.libraries {
        library.url = Some(format!("{}/", server.uri()));
    }

    let libraries = resolve_libraries(&downloader(&dir), &only_asm)
        .await
        .expect("自带信息应直接可用");

    assert_eq!(libraries.len(), 1);
    let artifact = libraries[0]
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .expect("应有主构件");
    assert_eq!(artifact.size, 126_151);
    assert_eq!(artifact.sha1, "ada2141c0cc52ee8f5c48cd5fa4ce0e794f22236");
}

#[tokio::test]
async fn fetch_profile_hits_the_expected_endpoint() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/versions/loader/26.3/0.20.1/profile/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PROFILE))
        .mount(&server)
        .await;

    let profile = fetch_profile_from(&downloader(&dir), &server.uri(), "26.3", "0.20.1")
        .await
        .expect("应能获取并解析");

    assert_eq!(profile.id, "fabric-loader-0.20.1-26.3");
    assert_eq!(profile.inherits_from, "26.3");
}

#[tokio::test]
async fn loader_list_parses_the_official_shape() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/versions/loader/26.3"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("[{ENTRY}]")))
        .mount(&server)
        .await;

    let loaders = fetch_loader_list_from(&downloader(&dir), &server.uri(), "26.3")
        .await
        .expect("应能获取并解析");

    assert_eq!(loaders.len(), 1);
    assert_eq!(loaders[0].version, "0.20.1");
    assert!(loaders[0].stable);
}

#[tokio::test]
async fn malformed_responses_are_reported_as_parse_errors() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/versions/loader/26.3"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
        .mount(&server)
        .await;

    let error = fetch_loader_list_from(&downloader(&dir), &server.uri(), "26.3")
        .await
        .expect_err("非 JSON 应失败");

    assert!(matches!(error, LoaderError::Parse(_)), "实际：{error:?}");
}

#[test]
fn invalid_coordinate_is_reported() {
    let profile = FabricProfile {
        id: "x".to_owned(),
        inherits_from: "26.3".to_owned(),
        main_class: "a.B".to_owned(),
        release_time: "t".to_owned(),
        time: "t".to_owned(),
        version_type: qcl_core::version::VersionType::Release,
        libraries: vec![qcl_install::FabricLibrary {
            name: "broken".to_owned(),
            url: None,
            sha1: None,
            size: None,
        }],
        arguments: None,
    };

    let dir = TempDir::new().expect("临时目录");
    let downloader = downloader(&dir);

    let error = tokio::runtime::Runtime::new()
        .expect("应能创建运行时")
        .block_on(resolve_libraries(&downloader, &profile))
        .expect_err("非法坐标应报错");

    assert!(
        matches!(error, LoaderError::InvalidCoordinate(_)),
        "实际：{error:?}"
    );
}
