//! 完整版本安装：走真实 HTTP 与真实磁盘。
//!
//! 服务端同时充当官方 CDN、资源仓库与 assets 对象仓库——`objects_base` 可配置正是
//! 为此。

use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;

use qcl_core::version::{LaunchContext, VersionMeta};
use qcl_install::{
    AssetInstallOptions, InstallPhase, InstallProgress, VersionError, VersionInstallOptions,
    install_version,
};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT_JAR: &[u8] = b"client-jar-bytes";
const LOG_CONFIG: &[u8] = b"<Configuration/>";
const LIBRARY_JAR: &[u8] = b"library-bytes";
const ASSET_OBJECT: &[u8] = b"asset-bytes";
const NATIVE_DLL: &[u8] = b"native-dll";

/// 造一个含 `META-INF/` 与一个 dll 的 natives 归档。
fn natives_archive() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer
            .start_file("META-INF/MANIFEST.MF", options)
            .expect("应能写入条目");
        writer.write_all(b"manifest").expect("应能写入内容");
        writer
            .start_file("lwjgl64.dll", options)
            .expect("应能写入条目");
        writer.write_all(NATIVE_DLL).expect("应能写入内容");
        writer.finish().expect("应能完成归档");
    }
    buffer.into_inner()
}

fn object_path(hash: &str) -> String {
    format!("/{}/{}", &hash[..2], hash)
}

/// 起服务并返回与该服务配套的版本元数据。
async fn serve() -> (MockServer, VersionMeta, Vec<u8>) {
    let server = MockServer::start().await;
    let natives = natives_archive();
    let asset_hash = hex_sha1(ASSET_OBJECT);

    let index_body = format!(
        r#"{{"objects":{{"minecraft/sounds/a.ogg":{{"hash":"{asset_hash}","size":{}}}}}}}"#,
        ASSET_OBJECT.len()
    );

    let object_route = object_path(&asset_hash);
    let payloads: Vec<(String, &[u8])> = vec![
        ("/client.jar".to_owned(), CLIENT_JAR),
        ("/client.xml".to_owned(), LOG_CONFIG),
        ("/lib.jar".to_owned(), LIBRARY_JAR),
        ("/natives.jar".to_owned(), natives.as_slice()),
        ("/indexes/34.json".to_owned(), index_body.as_bytes()),
        (object_route, ASSET_OBJECT),
    ];

    for (route, body) in payloads {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
            .mount(&server)
            .await;
    }

    let raw = format!(
        r#"{{
            "id": "test",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "releaseTime": "2026-01-01T00:00:00+00:00",
            "time": "2026-01-01T00:00:00+00:00",
            "assets": "34",
            "assetIndex": {{
                "id": "34",
                "sha1": "{index_sha}",
                "size": {index_size},
                "totalSize": {asset_size},
                "url": "{base}/indexes/34.json"
            }},
            "downloads": {{
                "client": {{
                    "url": "{base}/client.jar",
                    "sha1": "{client_sha}",
                    "size": {client_size}
                }}
            }},
            "logging": {{
                "client": {{
                    "argument": "-Dlog4j.configurationFile=${{path}}",
                    "file": {{
                        "id": "client.xml",
                        "url": "{base}/client.xml",
                        "sha1": "{log_sha}",
                        "size": {log_size}
                    }},
                    "type": "log4j2-xml"
                }}
            }},
            "libraries": [
                {{
                    "name": "g:a:1",
                    "downloads": {{
                        "artifact": {{
                            "path": "g/a/1/a-1.jar",
                            "url": "{base}/lib.jar",
                            "sha1": "{lib_sha}",
                            "size": {lib_size}
                        }},
                        "classifiers": {{
                            "natives-windows": {{
                                "path": "g/a/1/a-1-natives-windows.jar",
                                "url": "{base}/natives.jar",
                                "sha1": "{natives_sha}",
                                "size": {natives_size}
                            }}
                        }}
                    }},
                    "natives": {{ "windows": "natives-windows" }},
                    "extract": {{ "exclude": ["META-INF/"] }}
                }}
            ]
        }}"#,
        base = server.uri(),
        index_sha = hex_sha1(index_body.as_bytes()),
        index_size = index_body.len(),
        asset_size = ASSET_OBJECT.len(),
        client_sha = hex_sha1(CLIENT_JAR),
        client_size = CLIENT_JAR.len(),
        log_sha = hex_sha1(LOG_CONFIG),
        log_size = LOG_CONFIG.len(),
        lib_sha = hex_sha1(LIBRARY_JAR),
        lib_size = LIBRARY_JAR.len(),
        natives_sha = hex_sha1(&natives),
        natives_size = natives.len(),
    );

    let meta: VersionMeta = serde_json::from_str(&raw).expect("测试元数据应可解析");
    (server, meta, natives)
}

fn windows_context() -> LaunchContext {
    LaunchContext {
        os_name: Some("windows".to_owned()),
        arch: Some("x86_64".to_owned()),
        os_version: None,
        features: std::collections::BTreeMap::new(),
    }
}

fn options(server: &MockServer) -> VersionInstallOptions {
    VersionInstallOptions {
        version_dir: "versions/test".to_owned(),
        assets: AssetInstallOptions {
            assets_dir: "assets".to_owned(),
            objects_base: server.uri(),
        },
    }
}

#[tokio::test]
async fn installs_every_file_a_version_needs() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta, _) = serve().await;
    let downloader = Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器"));

    let report = install_version(
        downloader,
        &meta,
        &windows_context(),
        &options(&server),
        |_| {},
    )
    .await
    .expect("安装应成功");

    // 客户端 jar + 日志配置 + 库 + natives 归档
    assert_eq!(report.planned_files, 4);
    assert_eq!(report.downloaded, 4);
    assert_eq!(report.skipped, 0);

    assert_eq!(
        std::fs::read(dir.path().join("versions/test/test.jar")).expect("客户端 jar"),
        CLIENT_JAR
    );
    assert_eq!(
        std::fs::read(dir.path().join("assets/log_configs/client.xml")).expect("日志配置"),
        LOG_CONFIG
    );
    assert_eq!(
        std::fs::read(dir.path().join("libraries/g/a/1/a-1.jar")).expect("库"),
        LIBRARY_JAR
    );

    // natives 解压：dll 落盘，META-INF 被排除
    assert_eq!(report.natives.archives, 1);
    assert_eq!(report.natives.extracted, 1);
    assert_eq!(report.natives.excluded, 1);
    assert_eq!(
        std::fs::read(dir.path().join("versions/test/natives/lwjgl64.dll")).expect("dll"),
        NATIVE_DLL
    );
    assert!(!dir.path().join("versions/test/natives/META-INF").exists());

    // 资产
    assert_eq!(report.assets.objects, 1);
    assert_eq!(report.assets.downloaded, 1);
    let hash = hex_sha1(ASSET_OBJECT);
    assert_eq!(
        std::fs::read(
            dir.path()
                .join("assets/objects")
                .join(&hash[..2])
                .join(&hash)
        )
        .expect("资产对象"),
        ASSET_OBJECT
    );
}

#[tokio::test]
async fn progress_reports_both_phases_in_order() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta, _) = serve().await;
    let downloader = Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器"));

    let seen: Arc<Mutex<Vec<InstallPhase>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);

    install_version(
        downloader,
        &meta,
        &windows_context(),
        &options(&server),
        move |InstallProgress { phase, .. }| {
            let mut phases = recorder.lock().expect("锁未中毒");
            if phases.last() != Some(&phase) {
                phases.push(phase);
            }
        },
    )
    .await
    .expect("安装应成功");

    assert_eq!(
        *seen.lock().expect("锁未中毒"),
        vec![InstallPhase::VersionFiles, InstallPhase::Assets],
        "两个阶段的进度不能混在一起，否则界面进度条会突然回退"
    );
}

#[tokio::test]
async fn second_run_reuses_everything() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta, _) = serve().await;

    install_version(
        Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器")),
        &meta,
        &windows_context(),
        &options(&server),
        |_| {},
    )
    .await
    .expect("首次安装应成功");

    let second = install_version(
        Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器")),
        &meta,
        &windows_context(),
        &options(&server),
        |_| {},
    )
    .await
    .expect("二次安装应成功");

    assert_eq!(second.downloaded, 0, "已就绪的文件不应重复下载");
    assert_eq!(second.skipped, 4);
    assert_eq!(second.assets.downloaded, 0);
    assert_eq!(second.assets.skipped, 1);
}

#[tokio::test]
async fn version_file_failure_stops_before_assets() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 只挂客户端 jar，且返回 500：版本文件阶段就该失败。
    Mock::given(method("GET"))
        .and(path("/client.jar"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let meta: VersionMeta = serde_json::from_str(&format!(
        r#"{{
            "id": "test",
            "type": "release",
            "mainClass": "a.B",
            "releaseTime": "t",
            "time": "t",
            "downloads": {{
                "client": {{ "url": "{}/client.jar", "sha1": "{}", "size": {} }}
            }},
            "assetIndex": {{
                "id": "34",
                "sha1": "{}",
                "size": 2,
                "totalSize": 1,
                "url": "{}/indexes/34.json"
            }}
        }}"#,
        server.uri(),
        hex_sha1(CLIENT_JAR),
        CLIENT_JAR.len(),
        hex_sha1(b"{}"),
        server.uri()
    ))
    .expect("应可解析");

    let error = install_version(
        Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器")),
        &meta,
        &windows_context(),
        &options(&server),
        |_| {},
    )
    .await
    .expect_err("版本文件失败必须中止");

    assert!(matches!(error, VersionError::Files(_)), "实际：{error:?}");
    assert!(
        !dir.path().join("assets").join("objects").exists(),
        "版本文件失败时不应继续下载资产"
    );
}
