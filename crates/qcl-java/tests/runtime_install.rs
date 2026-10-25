//! 运行时安装的集成测试。
//!
//! 用 wiremock 同时扮演「清单服务器」与「文件服务器」，走真实的 HTTP 与磁盘路径，
//! 因此覆盖的是生产代码而不是假实现。

use std::path::PathBuf;
use std::sync::Arc;

use qcl_java::{InstallError, JavaSource, RuntimeIndex, install_runtime, java_executable_name};
use qcl_net::{HttpDownloader, hex_sha1};
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COMPONENT: &str = "java-runtime-epsilon";
const PLATFORM: &str = "windows-x64";

struct Fixture {
    /// 保持 mock 服务存活，否则其挂载的响应会失效。
    _server: MockServer,
    _dir: TempDir,
    downloader: Arc<HttpDownloader>,
    index: RuntimeIndex,
    game_dir: PathBuf,
}

/// 构造一份最小但形态完整的运行时清单。
async fn setup(include_java: bool, corrupt_digest: bool) -> Fixture {
    let server = MockServer::start().await;
    let dir = TempDir::new().expect("应能创建临时目录");
    let game_dir = dir.path().to_path_buf();

    let exe = java_executable_name();
    let java_path = format!("bin/{exe}");
    let java_body = b"fake-java-binary".to_vec();
    let release_body = b"JAVA_VERSION=\"25.0.4\"\n".to_vec();
    let modules_body = b"fake-modules".to_vec();

    for (relative, body) in [
        (java_path.as_str(), java_body.clone()),
        ("release", release_body.clone()),
        ("lib/modules", modules_body.clone()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/{relative}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
    }

    let mut files = Map::new();
    files.insert("bin".to_owned(), json!({ "type": "directory" }));

    if include_java {
        files.insert(
            java_path.clone(),
            json!({
                "type": "file",
                "executable": true,
                "downloads": { "raw": {
                    "sha1": hex_sha1(&java_body),
                    "size": java_body.len(),
                    "url": format!("{}/{java_path}", server.uri()),
                }}
            }),
        );
        files.insert(
            "release".to_owned(),
            json!({
                "type": "file",
                "downloads": { "raw": {
                    "sha1": hex_sha1(&release_body),
                    "size": release_body.len(),
                    "url": format!("{}/release", server.uri()),
                }}
            }),
        );
        files.insert(
            "lib/modules".to_owned(),
            json!({
                "type": "file",
                "downloads": { "raw": {
                    "sha1": hex_sha1(&modules_body),
                    "size": modules_body.len(),
                    "url": format!("{}/lib/modules", server.uri()),
                }}
            }),
        );
        files.insert(
            "lib/link.jar".to_owned(),
            json!({ "type": "link", "target": "lib/modules" }),
        );
    }

    let manifest_text = serde_json::to_string(&json!({ "files": files })).expect("应能序列化清单");

    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(manifest_text.clone()))
        .mount(&server)
        .await;

    let declared_sha1 = if corrupt_digest {
        "0".repeat(40)
    } else {
        hex_sha1(manifest_text.as_bytes())
    };

    let index_value = json!({
        PLATFORM: {
            COMPONENT: [{
                "version": { "name": "25.0.4", "released": "2026-01-20T00:00:00+00:00" },
                "manifest": {
                    "sha1": declared_sha1,
                    "size": manifest_text.len(),
                    "url": format!("{}/manifest.json", server.uri()),
                }
            }]
        }
    });
    let index: RuntimeIndex = serde_json::from_value(index_value).expect("索引应可解析");

    let downloader = Arc::new(HttpDownloader::new(&game_dir).expect("应能创建下载器"));

    Fixture {
        _server: server,
        _dir: dir,
        downloader,
        index,
        game_dir,
    }
}

#[tokio::test]
async fn installs_runtime_and_detects_it() {
    let fixture = setup(true, false).await;

    let installation = install_runtime(
        Arc::clone(&fixture.downloader),
        &fixture.index,
        COMPONENT,
        PLATFORM,
        |_| {},
    )
    .await
    .expect("安装应成功");

    assert_eq!(installation.source, JavaSource::Managed);
    assert_eq!(installation.version.major, 25);
    assert!(installation.executable.is_file());

    let home = fixture
        .game_dir
        .join("runtime")
        .join(COMPONENT)
        .join(PLATFORM);
    assert!(home.join("release").is_file());
    assert!(home.join("lib/modules").is_file());
    assert!(
        !home.join("lib/link.jar").exists(),
        "符号链接条目不参与下载"
    );
    assert!(
        !home.join("bin").is_file(),
        "directory 条目不应生成同名文件"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sets_executable_bit_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = setup(true, false).await;
    install_runtime(
        Arc::clone(&fixture.downloader),
        &fixture.index,
        COMPONENT,
        PLATFORM,
        |_| {},
    )
    .await
    .expect("安装应成功");

    let exe = fixture
        .game_dir
        .join("runtime")
        .join(COMPONENT)
        .join(PLATFORM)
        .join("bin")
        .join(java_executable_name());

    let mode = std::fs::metadata(&exe)
        .expect("应存在")
        .permissions()
        .mode();
    assert!(
        mode & 0o111 != 0,
        "bin/java 必须可执行，否则装完也启动不了（mode={mode:o}）"
    );
}

#[tokio::test]
async fn unavailable_component_is_reported() {
    let fixture = setup(true, false).await;

    let error = install_runtime(
        Arc::clone(&fixture.downloader),
        &fixture.index,
        "java-runtime-unknown",
        PLATFORM,
        |_| {},
    )
    .await
    .expect_err("未知组件应失败");

    assert!(matches!(error, InstallError::Unavailable { .. }));
    assert!(!fixture.game_dir.join("runtime").exists());
}

#[tokio::test]
async fn corrupt_manifest_digest_aborts_before_any_download() {
    let fixture = setup(true, true).await;

    let error = install_runtime(
        Arc::clone(&fixture.downloader),
        &fixture.index,
        COMPONENT,
        PLATFORM,
        |_| {},
    )
    .await
    .expect_err("清单摘要不符应失败");

    assert!(matches!(error, InstallError::ManifestChecksum));
    assert!(
        !fixture.game_dir.join("runtime").exists(),
        "清单不可信时不应写入任何文件"
    );
}

#[tokio::test]
async fn missing_java_after_install_is_reported() {
    // 清单里没有 bin/java：流程能走完，但探测不到可用的 Java。
    let fixture = setup(false, false).await;

    let error = install_runtime(
        Arc::clone(&fixture.downloader),
        &fixture.index,
        COMPONENT,
        PLATFORM,
        |_| {},
    )
    .await
    .expect_err("探测不到 Java 应失败");

    assert!(matches!(error, InstallError::NotDetected(_)));
}

#[tokio::test]
async fn http_failure_is_reported_as_download_error() {
    let fixture = setup(true, false).await;

    // 让清单地址指向一个必定 404 的服务，验证错误会被归类为下载层失败。
    let broken = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/manifest.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&broken)
        .await;

    let mut index_value = serde_json::to_value(&fixture.index).expect("索引应可序列化");
    let entry = index_value
        .get_mut(PLATFORM)
        .and_then(|platform| platform.get_mut(COMPONENT))
        .and_then(|releases| releases.get_mut(0))
        .and_then(|release| release.get_mut("manifest"))
        .expect("索引结构应与构造时一致");
    entry["url"] = Value::String(format!("{}/manifest.json", broken.uri()));

    let index: RuntimeIndex = serde_json::from_value(index_value).expect("索引应可解析");

    let error = install_runtime(
        Arc::clone(&fixture.downloader),
        &index,
        COMPONENT,
        PLATFORM,
        |_| {},
    )
    .await
    .expect_err("404 应失败");

    assert!(
        matches!(error, InstallError::Download(_)),
        "应把下载层错误原样上抛，便于区分网络问题与数据问题"
    );
}
