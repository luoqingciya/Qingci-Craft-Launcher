// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Modrinth 搜索、文件解析与安装。
//!
//! 夹具是 2026-10-26 从 `api.modrinth.com` 拉到的真实响应，字段名照抄。

use std::sync::Arc;

use qcl_mods::{ModError, dependencies_from, install, resolve_file_from, search_from};
use qcl_net::{HttpDownloader, hex_sha1};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SEARCH_BODY: &str = r#"{
  "hits": [
    {
      "project_id": "AANobbMI",
      "project_type": "mod",
      "slug": "sodium",
      "author": "jellysquid3",
      "title": "Sodium",
      "description": "A high-performance rendering engine replacement for Minecraft.",
      "downloads": 12345678
    }
  ],
  "total_hits": 32
}"#;

const VERSIONS_BODY: &str = r#"[
  {
    "id": "abc123",
    "version_number": "mc26.3-0.9.3-beta.1-fabric",
    "files": [
      {
        "url": "https://cdn.modrinth.com/sources.jar",
        "filename": "sodium-sources.jar",
        "primary": false,
        "size": 100,
        "hashes": { "sha1": "0000000000000000000000000000000000000000" }
      },
      {
        "url": "https://cdn.modrinth.com/sodium.jar",
        "filename": "sodium-fabric-0.9.3-beta.1+mc26.3.jar",
        "primary": true,
        "size": 1912496,
        "hashes": { "sha1": "1e6aec741b4134258ba1d7d458bae96a2e8e187e" }
      }
    ]
  }
]"#;

fn downloader(dir: &TempDir) -> HttpDownloader {
    HttpDownloader::new(dir.path()).expect("应能创建下载器")
}

#[tokio::test]
async fn search_sends_version_and_loader_facets() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .mount(&server)
        .await;

    let results = search_from(
        &downloader(&dir),
        &server.uri(),
        "sodium",
        "26.3",
        "fabric",
        10,
    )
    .await
    .expect("应能搜索");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Sodium");
    assert_eq!(results[0].project_id, "AANobbMI");

    // facets 必须带上游戏版本与加载器，否则会搜到不兼容的模组。
    let requests = server.received_requests().await.expect("应能读到请求");
    let url = requests[0].url.as_str().to_string();
    // 冒号在查询值里合法，不转义；方括号与引号必须转义，否则 facets 的 JSON 会破坏 URL。
    assert!(
        url.contains("%5B%5B%22project_type:mod%22%5D"),
        "实际 URL：{url}"
    );
    assert!(url.contains("%22versions:26.3%22"), "实际 URL：{url}");
    assert!(
        url.contains("%22categories:fabric%22%5D"),
        "实际 URL：{url}"
    );
}

#[tokio::test]
async fn resolve_file_prefers_the_primary_file() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSIONS_BODY))
        .mount(&server)
        .await;

    let file = resolve_file_from(
        &downloader(&dir),
        &server.uri(),
        "AANobbMI",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能解析");

    // 一个版本可能带多个文件（源码包、dev 包），必须挑 primary 那个。
    assert_eq!(file.filename, "sodium-fabric-0.9.3-beta.1+mc26.3.jar");
    assert_eq!(file.size, 1_912_496);
    assert_eq!(file.sha1.len(), 40);
}

#[tokio::test]
async fn resolve_file_reports_missing_compatibility() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/X/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let error = resolve_file_from(&downloader(&dir), &server.uri(), "X", "26.3", "fabric")
        .await
        .expect_err("空列表应报错");

    assert!(
        matches!(error, ModError::NoCompatibleFile(_)),
        "实际：{error:?}"
    );
}

#[tokio::test]
async fn resolve_file_rejects_missing_digest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 没有摘要就没法校验，宁可失败也不装一个来路不明的 jar。
    let body = VERSIONS_BODY.replace("1e6aec741b4134258ba1d7d458bae96a2e8e187e", "");
    Mock::given(method("GET"))
        .and(path("/project/X/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let error = resolve_file_from(&downloader(&dir), &server.uri(), "X", "26.3", "fabric")
        .await
        .expect_err("缺摘要应报错");

    assert!(matches!(error, ModError::Download(_)), "实际：{error:?}");
}

#[tokio::test]
async fn install_writes_into_mods_and_verifies_digest() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    let body = b"fake-jar-bytes".to_vec();
    let digest = hex_sha1(&body);

    Mock::given(method("GET"))
        .and(path("/sodium.jar"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let file = qcl_mods::ModFile {
        project_id: "AANobbMI".to_owned(),
        version_number: "1.0".to_owned(),
        filename: "sodium.jar".to_owned(),
        url: format!("{}/sodium.jar", server.uri()),
        sha1: digest,
        size: body.len() as u64,
        dependencies: Vec::new(),
    };

    let downloader = Arc::new(downloader(&dir));
    let path = install(Arc::clone(&downloader), &file)
        .await
        .expect("应能安装");

    assert_eq!(path, "mods/sodium.jar");
    assert!(dir.path().join("mods/sodium.jar").is_file());
}

#[tokio::test]
async fn install_rejects_unsafe_filenames() {
    let dir = TempDir::new().expect("临时目录");
    let downloader = Arc::new(downloader(&dir));

    // 远端文件名不能假设规矩：接受 `../` 就会写到游戏目录之外。
    for name in [
        "../evil.jar",
        "a/b.jar",
        "a\\b.jar",
        "..",
        ".hidden.jar",
        "  ",
    ] {
        let file = qcl_mods::ModFile {
            project_id: "X".to_owned(),
            version_number: "1".to_owned(),
            filename: name.to_owned(),
            url: "http://127.0.0.1:1/x.jar".to_owned(),
            sha1: "0".repeat(40),
            size: 1,
            dependencies: Vec::new(),
        };

        let error = install(Arc::clone(&downloader), &file)
            .await
            .expect_err("不安全文件名应被拒绝");
        assert!(
            matches!(error, ModError::UnsafeFilename(_)),
            "{name} 应被拒绝，实际：{error:?}"
        );
    }
}

#[tokio::test]
#[ignore = "需要访问 api.modrinth.com"]
async fn live_search_and_resolve() {
    // 对着真实上游跑一遍：搜索 → 解析到具体文件。
    // 只验证「能拿到适配 26.3 + Fabric 的文件」，不下载（几 MB，没必要）。
    let dir = TempDir::new().expect("临时目录");
    let downloader = downloader(&dir);

    let hits = qcl_mods::search(&downloader, "sodium", "26.3", "fabric", 5)
        .await
        .expect("应能搜索");
    assert!(!hits.is_empty(), "26.3 + Fabric 下应有 sodium 相关结果");

    let sodium = hits
        .iter()
        .find(|hit| hit.title.eq_ignore_ascii_case("sodium"))
        .expect("应能找到 Sodium");

    let file = qcl_mods::resolve_file(&downloader, &sodium.project_id, "26.3", "fabric")
        .await
        .expect("应能解析出文件");

    assert!(file.filename.ends_with(".jar"), "实际：{}", file.filename);
    assert_eq!(file.sha1.len(), 40, "摘要应是完整 SHA-1");
    assert!(file.size > 0, "大小应大于 0");
    assert!(file.url.starts_with("https://"), "下载地址应走 HTTPS");
}

#[test]
fn remove_deletes_only_inside_mods() {
    let dir = TempDir::new().expect("临时目录");
    let mods = dir.path().join("mods");
    std::fs::create_dir_all(&mods).expect("应能建目录");
    std::fs::write(mods.join("sodium.jar"), b"x").expect("应能写入");
    std::fs::write(dir.path().join("outside.jar"), b"x").expect("应能写入");

    qcl_mods::remove(dir.path(), "sodium.jar").expect("应能删除");
    assert!(!mods.join("sodium.jar").exists());
    // mods 目录之外的文件不该被碰到。
    assert!(dir.path().join("outside.jar").exists());
}

#[test]
fn remove_rejects_escaping_names_and_non_jars() {
    let dir = TempDir::new().expect("临时目录");
    std::fs::create_dir_all(dir.path().join("mods")).expect("应能建目录");
    std::fs::write(dir.path().join("outside.jar"), b"x").expect("应能写入");

    for name in ["../outside.jar", "a/b.jar", r"a\b.jar", "..", ".hidden.jar"] {
        let error = qcl_mods::remove(dir.path(), name).expect_err("应被拒绝");
        assert!(
            matches!(error, ModError::UnsafeFilename(_)),
            "{name} 应被拒绝，实际：{error:?}"
        );
    }

    // 不是 .jar 的一律不删：这个接口只服务模组，不该变成通用删除器。
    let error = qcl_mods::remove(dir.path(), "notes.txt").expect_err("非 jar 应被拒绝");
    assert!(
        matches!(error, ModError::UnsafeFilename(_)),
        "实际：{error:?}"
    );

    assert!(dir.path().join("outside.jar").exists());
}

#[test]
fn remove_reports_missing_file() {
    let dir = TempDir::new().expect("临时目录");
    std::fs::create_dir_all(dir.path().join("mods")).expect("应能建目录");

    let error = qcl_mods::remove(dir.path(), "ghost.jar").expect_err("不存在应报错");
    assert!(matches!(error, ModError::Download(_)), "实际：{error:?}");
}

#[tokio::test]
async fn search_parses_icon_url() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    let body = r#"{
      "hits": [
        {
          "project_id": "AANobbMI",
          "slug": "sodium",
          "title": "Sodium",
          "description": "d",
          "downloads": 1,
          "author": "a",
          "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let results = search_from(&downloader(&dir), &server.uri(), "x", "26.3", "fabric", 1)
        .await
        .expect("应能搜索");

    assert_eq!(
        results[0].icon_url.as_deref(),
        Some("https://cdn.modrinth.com/data/AANobbMI/icon.png")
    );
}

/// 造一个「项目 → 版本」的 mock：`deps` 是该项目声明的必需依赖。
async fn mount_project(server: &MockServer, id: &str, filename: &str, body: &[u8], deps: &[&str]) {
    let dependencies: Vec<String> = deps
        .iter()
        .map(|dep| format!(r#"{{"project_id":"{dep}","dependency_type":"required"}}"#))
        .collect();
    // 顺带混入一个可选依赖，确认它不会被装。
    let payload = format!(
        r#"[{{"version_number":"1.0","files":[{{"url":"{}/files/{filename}","filename":"{filename}","primary":true,"size":{},"hashes":{{"sha1":"{}"}}}}],"dependencies":[{}]}}]"#,
        server.uri(),
        body.len(),
        hex_sha1(body),
        {
            let mut all = dependencies;
            all.push(r#"{"project_id":"optional-dep","dependency_type":"optional"}"#.to_owned());
            all.join(",")
        }
    );

    Mock::given(method("GET"))
        .and(path(format!("/project/{id}/version")))
        .respond_with(ResponseTemplate::new(200).set_body_string(payload))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

#[tokio::test]
async fn install_pulls_required_dependencies() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 主模组依赖 fabric-api；fabric-api 自身没有依赖。
    mount_project(&server, "main", "main.jar", b"main", &["fabric-api"]).await;
    mount_project(&server, "fabric-api", "fabric-api.jar", b"api", &[]).await;

    let installed = qcl_mods::install_with_dependencies_from(
        Arc::new(downloader(&dir)),
        &server.uri(),
        "main",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能安装");

    assert!(
        installed.contains(&"mods/main.jar".to_owned()),
        "实际：{installed:?}"
    );
    assert!(
        installed.contains(&"mods/fabric-api.jar".to_owned()),
        "依赖必须一起装，否则游戏会报 requires fabric-api：{installed:?}"
    );
    assert_eq!(installed.len(), 2, "可选依赖不该被安装：{installed:?}");
    assert!(dir.path().join("mods/fabric-api.jar").is_file());
}

#[tokio::test]
async fn dependency_installation_dedupes_shared_dependencies() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    // 主模组同时依赖 A 与 B，而 A、B 都依赖 shared —— shared 只该装一次。
    mount_project(&server, "main", "main.jar", b"main", &["a", "b"]).await;
    mount_project(&server, "a", "a.jar", b"a", &["shared"]).await;
    mount_project(&server, "b", "b.jar", b"b", &["shared"]).await;
    mount_project(&server, "shared", "shared.jar", b"shared", &[]).await;

    let installed = qcl_mods::install_with_dependencies_from(
        Arc::new(downloader(&dir)),
        &server.uri(),
        "main",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能安装");

    let shared_count = installed
        .iter()
        .filter(|path| path.ends_with("shared.jar"))
        .count();
    assert_eq!(shared_count, 1, "共享依赖只应装一次：{installed:?}");
    assert_eq!(installed.len(), 4, "实际：{installed:?}");
}

#[tokio::test]
async fn installing_again_skips_existing_files() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    mount_project(&server, "main", "main.jar", b"main", &[]).await;

    let downloader = Arc::new(downloader(&dir));
    let first = qcl_mods::install_with_dependencies_from(
        Arc::clone(&downloader),
        &server.uri(),
        "main",
        "26.3",
        "fabric",
    )
    .await
    .expect("首次应能安装");
    assert_eq!(first.len(), 1);

    // 再装一次不该重复下载。
    let second = qcl_mods::install_with_dependencies_from(
        Arc::clone(&downloader),
        &server.uri(),
        "main",
        "26.3",
        "fabric",
    )
    .await
    .expect("重复安装应成功");
    assert!(second.is_empty(), "已存在的文件应跳过：{second:?}");
}

#[tokio::test]
async fn resolve_file_ignores_optional_dependencies() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    mount_project(&server, "main", "main.jar", b"main", &["fabric-api"]).await;

    let file = resolve_file_from(&downloader(&dir), &server.uri(), "main", "26.3", "fabric")
        .await
        .expect("应能解析");

    assert_eq!(file.dependencies, vec!["fabric-api".to_owned()]);
}

#[tokio::test]
async fn search_puts_selected_categories_in_one_or_group() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_BODY))
        .mount(&server)
        .await;

    let categories = vec!["optimization".to_owned(), "technology".to_owned()];
    qcl_mods::search_with_filters_from(
        &downloader(&dir),
        &server.uri(),
        "x",
        "26.3",
        "fabric",
        &categories,
        10,
    )
    .await
    .expect("应能搜索");

    let requests = server.received_requests().await.expect("应能读到请求");
    let url = requests[0].url.as_str().to_string();

    // 多个分类必须在同一个 facets 组里（组内是「或」），否则会变成取交集。
    assert!(
        url.contains("%5B%22categories:optimization%22,%22categories:technology%22%5D"),
        "实际 URL：{url}"
    );
}

#[tokio::test]
async fn check_updates_reports_newer_files_only() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let mods = dir.path().join("mods");
    std::fs::create_dir_all(&mods).expect("应能建目录");

    // 两个已装模组：sodium 有新版本，lithium 没有。
    std::fs::write(mods.join("sodium-old.jar"), b"x").expect("应能写入");
    std::fs::write(mods.join("lithium.jar"), b"x").expect("应能写入");
    qcl_mods::manifest::remember(&mods, "sodium-old.jar", "AANobbMI", "0.9.0").expect("应能记下");
    qcl_mods::manifest::remember(&mods, "lithium.jar", "gvQqBUqZ", "0.26.0").expect("应能记下");

    // sodium 最新文件换了名字（版本号变了）；lithium 还是同名。
    mount_project(&server, "AANobbMI", "sodium-new.jar", b"new", &[]).await;
    mount_project(&server, "gvQqBUqZ", "lithium.jar", b"same", &[]).await;

    let report = qcl_mods::check_updates_from(&downloader(&dir), &server.uri(), "26.3", "fabric")
        .await
        .expect("应能检查");

    assert_eq!(
        report.updates.len(),
        1,
        "只有文件名变了才算更新：{report:?}"
    );
    assert_eq!(report.updates[0].file_name, "sodium-old.jar");
    assert_eq!(report.updates[0].latest_file_name, "sodium-new.jar");
}

#[tokio::test]
async fn check_updates_ignores_mods_without_manifest_entry() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let mods = dir.path().join("mods");
    std::fs::create_dir_all(&mods).expect("应能建目录");

    // 用户手动丢进来的 jar：没有清单条目，无从得知项目 id。
    std::fs::write(mods.join("manual.jar"), b"x").expect("应能写入");

    let report = qcl_mods::check_updates_from(&downloader(&dir), &server.uri(), "26.3", "fabric")
        .await
        .expect("应能检查");

    assert!(report.updates.is_empty() && report.incompatible.is_empty());
}

#[tokio::test]
async fn check_updates_reports_projects_without_compatible_file() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let mods = dir.path().join("mods");
    std::fs::create_dir_all(&mods).expect("应能建目录");
    std::fs::write(mods.join("gone.jar"), b"x").expect("应能写入");
    qcl_mods::manifest::remember(&mods, "gone.jar", "dead-project", "1.0").expect("应能记下");

    // 该版本下没有适配文件（空列表）。
    Mock::given(method("GET"))
        .and(path("/project/dead-project/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;

    let report = qcl_mods::check_updates_from(&downloader(&dir), &server.uri(), "26.3", "fabric")
        .await
        .expect("单个项目失败不该让整次检查失败");

    assert!(report.updates.is_empty(), "没有适配文件不是「有新版本」");
    assert_eq!(report.incompatible.len(), 1);
    assert_eq!(report.incompatible[0].file_name, "gone.jar");
}

#[tokio::test]
#[ignore = "需要访问 api.modrinth.com 并下载真实文件"]
async fn live_install_then_detect_update() {
    let dir = TempDir::new().expect("临时目录");
    let downloader = Arc::new(downloader(&dir));

    // 1) 真装一个 Sodium。
    let installed =
        qcl_mods::install_with_dependencies(Arc::clone(&downloader), "AANobbMI", "26.3", "fabric")
            .await
            .expect("应能安装");
    assert!(!installed.is_empty());

    // 2) 刚装完就是最新，不该报更新。
    let report = qcl_mods::check_updates(&downloader, "26.3", "fabric")
        .await
        .expect("应能检查");
    assert!(
        report.updates.is_empty() && report.incompatible.is_empty(),
        "刚装完不该有更新：{report:?}"
    );

    // 3) 把清单改成一个旧版本的文件名（模拟「装的是老版本」），应能识别出更新。
    let mods_dir = dir.path().join("mods");
    let manifest = qcl_mods::manifest::load(&mods_dir);
    let (file_name, entry) = manifest.iter().next().expect("应有一条记录");
    let fake_old = "sodium-fabric-0.0.1-old.jar".to_owned();
    std::fs::rename(mods_dir.join(file_name), mods_dir.join(&fake_old)).expect("应能改名");
    qcl_mods::manifest::remember(&mods_dir, &fake_old, &entry.project_id, "0.0.1")
        .expect("应能记下");
    qcl_mods::manifest::forget(&mods_dir, file_name).expect("应能忘掉旧名");

    let report = qcl_mods::check_updates(&downloader, "26.3", "fabric")
        .await
        .expect("应能检查");

    assert_eq!(report.updates.len(), 1, "应识别出一个更新：{report:?}");
    assert_eq!(report.updates[0].file_name, fake_old);
    assert_eq!(report.updates[0].current_version, "0.0.1");
    assert_ne!(report.updates[0].latest_file_name, fake_old);
}

/// 带必需依赖与可选依赖的版本响应。
const VERSIONS_WITH_DEPS: &str = r#"[
  {
    "id": "abc123",
    "version_number": "mc26.3-0.9.3-fabric",
    "files": [
      {
        "url": "https://cdn.modrinth.com/sodium.jar",
        "filename": "sodium-fabric-0.9.3+mc26.3.jar",
        "primary": true,
        "size": 1912496,
        "hashes": { "sha1": "1e6aec741b4134258ba1d7d458bae96a2e8e187e" }
      }
    ],
    "dependencies": [
      { "project_id": "P7dR8mSH", "dependency_type": "required" },
      { "project_id": "Ha28R6CL", "dependency_type": "optional" }
    ]
  }
]"#;

const PROJECTS_BODY: &str = r#"[
  { "id": "P7dR8mSH", "title": "Fabric API" }
]"#;

#[tokio::test]
async fn dependencies_keep_required_only_and_add_titles() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSIONS_WITH_DEPS))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PROJECTS_BODY))
        .mount(&server)
        .await;

    let deps = dependencies_from(
        &downloader(&dir),
        &server.uri(),
        "AANobbMI",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能查到依赖");

    // 可选依赖不能出现：装上去反而可能冲突，安装流程也是这么筛的。
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].project_id, "P7dR8mSH");
    assert_eq!(deps[0].title.as_deref(), Some("Fabric API"));
}

#[tokio::test]
async fn dependencies_survive_a_failing_title_lookup() {
    // 标题只是锦上添花。查不到也必须告诉用户「有依赖、会被装」。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSIONS_WITH_DEPS))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/projects"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let deps = dependencies_from(
        &downloader(&dir),
        &server.uri(),
        "AANobbMI",
        "26.3",
        "fabric",
    )
    .await
    .expect("标题失败不该让整个查询失败");

    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].project_id, "P7dR8mSH");
    assert_eq!(deps[0].title, None);
}

#[tokio::test]
async fn dependencies_are_empty_for_a_standalone_mod() {
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(VERSIONS_BODY))
        .mount(&server)
        .await;

    let deps = dependencies_from(
        &downloader(&dir),
        &server.uri(),
        "AANobbMI",
        "26.3",
        "fabric",
    )
    .await
    .expect("应能查到依赖");

    assert!(deps.is_empty());
}

#[tokio::test]
async fn check_updates_does_not_call_network_errors_incompatible() {
    // 一次断网就把用户的模组全标成「不兼容」，比不报还糟。
    let dir = TempDir::new().expect("临时目录");
    let server = MockServer::start().await;
    let mods = dir.path().join("mods");
    std::fs::create_dir_all(&mods).expect("应能建目录");
    std::fs::write(mods.join("sodium.jar"), b"x").expect("应能写入");
    qcl_mods::manifest::remember(&mods, "sodium.jar", "AANobbMI", "1.0").expect("应能记下");

    Mock::given(method("GET"))
        .and(path("/project/AANobbMI/version"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let report = qcl_mods::check_updates_from(&downloader(&dir), &server.uri(), "26.3", "fabric")
        .await
        .expect("应能检查");

    assert!(report.updates.is_empty());
    assert!(report.incompatible.is_empty(), "网络错误不是不兼容");
}
