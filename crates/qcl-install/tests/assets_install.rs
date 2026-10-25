//! 资产安装：走真实 HTTP（wiremock）与真实磁盘。
//!
//! 对象 URL 由 `objects_base` 注入，因此不需要访问官方 CDN——这是把 CDN 根地址
//! 做成配置项的直接理由。

use std::path::Path;
use std::sync::Arc;

use qcl_core::version::VersionMeta;
use qcl_install::{AssetError, AssetInstallOptions, install_assets};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 两个对象：`minecraft/sounds/stone.ogg` 与 `icons/icon.png`。
const STONE: &[u8] = b"stone-break-sound";
const ICON: &[u8] = b"icon-bytes";

fn stone_hash() -> String {
    hex_sha1(STONE)
}

fn icon_hash() -> String {
    hex_sha1(ICON)
}

fn index_body() -> String {
    format!(
        r#"{{"objects":{{
            "icons/icon.png":{{"hash":"{}","size":{}}},
            "minecraft/sounds/stone.ogg":{{"hash":"{}","size":{}}}
        }}}}"#,
        icon_hash(),
        ICON.len(),
        stone_hash(),
        STONE.len()
    )
}

/// 构造只含资产索引的版本元数据。必填字段之外一律省略，避免编造无关数据。
fn meta_with_index(id: &str, url: &str, sha1: &str, size: u64) -> VersionMeta {
    let raw = format!(
        r#"{{
            "id": "test-version",
            "type": "release",
            "mainClass": "net.minecraft.client.main.Main",
            "releaseTime": "2026-01-01T00:00:00+00:00",
            "time": "2026-01-01T00:00:00+00:00",
            "assets": "{id}",
            "assetIndex": {{
                "id": "{id}",
                "sha1": "{sha1}",
                "size": {size},
                "totalSize": 0,
                "url": "{url}"
            }}
        }}"#
    );
    serde_json::from_str(&raw).expect("测试元数据应可解析")
}

/// 起一个服务：索引在 `/indexes/<id>.json`，两个对象在 `/<xx>/<hash>`。
async fn serve(id: &str) -> (MockServer, VersionMeta) {
    let server = MockServer::start().await;
    let body = index_body();

    Mock::given(method("GET"))
        .and(path(format!("/indexes/{id}.json")))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;

    for (hash, bytes) in [(stone_hash(), STONE), (icon_hash(), ICON)] {
        Mock::given(method("GET"))
            .and(path(format!("/{}/{}", &hash[..2], hash)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&server)
            .await;
    }

    let meta = meta_with_index(
        id,
        &format!("{}/indexes/{id}.json", server.uri()),
        &hex_sha1(body.as_bytes()),
        body.len() as u64,
    );

    (server, meta)
}

fn downloader(dir: &TempDir) -> Arc<HttpDownloader> {
    Arc::new(HttpDownloader::new(dir.path()).expect("应能创建下载器"))
}

fn options(server: &MockServer) -> AssetInstallOptions {
    AssetInstallOptions {
        assets_dir: "assets".to_owned(),
        objects_base: server.uri(),
    }
}

fn object_path(dir: &Path, hash: &str) -> std::path::PathBuf {
    dir.join("assets")
        .join("objects")
        .join(&hash[..2])
        .join(hash)
}

#[tokio::test]
async fn modern_index_downloads_objects_without_virtual_tree() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("34").await;

    let report = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("安装应成功");

    assert_eq!(report.index_id, "34");
    assert_eq!(report.objects, 2);
    assert_eq!(report.downloaded, 2);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.total_bytes, (STONE.len() + ICON.len()) as u64);
    assert_eq!(report.virtual_files, 0, "现代索引不需要虚拟目录");

    assert_eq!(
        std::fs::read(object_path(dir.path(), &stone_hash())).expect("对象应已落盘"),
        STONE
    );
    assert!(!dir.path().join("assets").join("virtual").exists());

    // 索引本身也要落盘，供离线排查与重建校验使用。
    let index_file = dir.path().join("assets/indexes/34.json");
    assert_eq!(
        std::fs::read_to_string(&index_file).expect("索引应已落盘"),
        index_body()
    );
}

#[tokio::test]
async fn second_run_reuses_existing_objects() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("34").await;

    let first = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("首次安装应成功");
    assert_eq!(first.downloaded, 2);

    let second = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("二次安装应成功");

    assert_eq!(second.downloaded, 0, "已存在的对象不应重复下载");
    assert_eq!(second.skipped, 2);
}

#[tokio::test]
async fn index_checksum_mismatch_writes_nothing() {
    let dir = TempDir::new().expect("临时目录");
    let (server, _) = serve("34").await;

    // 故意声明错误的摘要。
    let meta = meta_with_index(
        "34",
        &format!("{}/indexes/34.json", server.uri()),
        &"0".repeat(40),
        1,
    );

    let error = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect_err("摘要不符必须失败");

    assert!(matches!(error, AssetError::IndexChecksum));
    assert!(
        !dir.path().join("assets").join("objects").exists(),
        "校验失败时不应写盘"
    );
    assert!(
        !dir.path().join("assets").join("indexes").exists(),
        "校验失败时索引也不应落盘"
    );
}

#[tokio::test]
async fn legacy_index_expands_virtual_tree() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("legacy").await;

    let report = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("安装应成功");

    assert_eq!(report.virtual_files, 2);

    let virtual_sound = dir
        .path()
        .join("assets")
        .join("virtual")
        .join("legacy")
        .join("minecraft")
        .join("sounds")
        .join("stone.ogg");
    assert_eq!(
        std::fs::read(&virtual_sound).expect("虚拟目录应已展开"),
        STONE
    );

    assert!(
        !dir.path().join("resources").exists(),
        "legacy 不需要 resources 副本"
    );
}

#[tokio::test]
async fn pre_1_6_also_copies_into_resources() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("pre-1.6").await;

    let report = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("安装应成功");

    // 2 个对象 × 两处落盘（virtual 与 resources）。
    assert_eq!(report.virtual_files, 4);

    let virtual_path = dir
        .path()
        .join("assets")
        .join("virtual")
        .join("pre-1.6")
        .join("icons")
        .join("icon.png");
    let resources = dir.path().join("resources").join("icons").join("icon.png");

    assert_eq!(std::fs::read(&virtual_path).expect("虚拟目录应存在"), ICON);
    assert_eq!(
        std::fs::read(&resources).expect("resources 副本应存在"),
        ICON
    );
}

#[tokio::test]
async fn legacy_tree_is_not_rebuilt_when_already_complete() {
    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("legacy").await;

    install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("首次安装应成功");

    let second = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("二次安装应成功");

    assert_eq!(
        second.virtual_files, 0,
        "已完整的目录树不应重复拷贝——旧版有数千个文件，每次启动重拷代价明显"
    );
}

#[tokio::test]
async fn missing_index_is_reported() {
    let dir = TempDir::new().expect("临时目录");
    let (server, _) = serve("34").await;

    let meta: VersionMeta = serde_json::from_str(
        r#"{
            "id": "custom",
            "type": "release",
            "mainClass": "a.B",
            "releaseTime": "2026-01-01T00:00:00+00:00",
            "time": "2026-01-01T00:00:00+00:00"
        }"#,
    )
    .expect("应可解析");

    let error = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect_err("缺少索引应失败");

    assert!(matches!(error, AssetError::MissingIndex));
}

#[tokio::test]
async fn unsafe_logical_paths_are_not_expanded() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 键里带 `..`：对象本身合法（按哈希落盘），但不能被展开到目录树里。
    let body = format!(
        r#"{{"objects":{{"../../escaped.txt":{{"hash":"{}","size":{}}}}}}}"#,
        stone_hash(),
        STONE.len()
    );
    Mock::given(method("GET"))
        .and(path("/indexes/legacy.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/{}/{}", &stone_hash()[..2], stone_hash())))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(STONE))
        .mount(&server)
        .await;

    let meta = meta_with_index(
        "legacy",
        &format!("{}/indexes/legacy.json", server.uri()),
        &hex_sha1(body.as_bytes()),
        body.len() as u64,
    );

    let report = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect("安装应成功");

    assert_eq!(report.downloaded, 1, "对象本身仍应下载");
    assert_eq!(report.virtual_files, 0, "不安全路径不得展开");
    assert!(
        !dir.path().join("escaped.txt").exists(),
        "文件不得逃出资产目录"
    );
}

#[tokio::test]
async fn object_download_failure_is_reported() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let body = index_body();

    Mock::given(method("GET"))
        .and(path("/indexes/34.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;
    // 对象一律 500，验证失败会被归因为下载错误而不是静默成功。
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let meta = meta_with_index(
        "34",
        &format!("{}/indexes/34.json", server.uri()),
        &hex_sha1(body.as_bytes()),
        body.len() as u64,
    );

    let error = install_assets(downloader(&dir), &meta, &options(&server), |_| {})
        .await
        .expect_err("对象下载失败必须报错");

    assert!(
        matches!(error, AssetError::Download(_)),
        "应归因为下载错误，实际：{error:?}"
    );
}

#[tokio::test]
async fn progress_reaches_completion() {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = TempDir::new().expect("临时目录");
    let (server, meta) = serve("34").await;

    let calls = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(Mutex::new(false));
    let (calls_ref, finished_ref) = (Arc::clone(&calls), Arc::clone(&finished));

    install_assets(
        downloader(&dir),
        &meta,
        &options(&server),
        move |progress| {
            calls_ref.fetch_add(1, Ordering::Relaxed);
            if progress.is_finished() {
                *finished_ref.lock().expect("锁未中毒") = true;
            }
        },
    )
    .await
    .expect("安装应成功");

    assert!(calls.load(Ordering::Relaxed) > 0, "应至少回调一次进度");
    assert!(
        *finished.lock().expect("锁未中毒"),
        "进度应最终到达完成状态"
    );
}
