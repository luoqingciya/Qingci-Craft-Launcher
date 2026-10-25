//! 下载层核心逻辑的测试。

use std::collections::BTreeMap;
use std::time::Duration;

use qcl_core::version::{LaunchContext, VersionMeta};
use qcl_net::{
    RetryPolicy, Sha1Hasher, hex_sha1, is_retryable_status, plan_version_files, verify_sha1,
};

// 夹具与 qcl-core 共用同一份官方数据，避免两处各存一份而逐渐走样。
const META_26_3: &str = include_str!("../../qcl-core/tests/fixtures/26.3.json");
const META_LEGACY: &str = include_str!("../../qcl-core/tests/fixtures/legacy-1.7.10.json");

fn windows_11() -> LaunchContext {
    LaunchContext {
        os_name: Some("windows".to_owned()),
        arch: Some("x86_64".to_owned()),
        os_version: Some("10.0.22631".to_owned()),
        features: BTreeMap::new(),
    }
}

fn parse(raw: &str) -> VersionMeta {
    serde_json::from_str(raw).expect("夹具应可解析")
}

#[test]
fn plan_covers_client_jar_logging_and_libraries() {
    let meta = parse(META_26_3);
    let plan = plan_version_files(&meta, &windows_11(), "versions/26.3");

    // 74 个适用库 + 客户端 jar + 日志配置。
    assert_eq!(plan.len(), 76);
    assert_eq!(
        plan.tasks.first().map(|task| task.path.as_str()),
        Some("versions/26.3/26.3.jar"),
        "客户端 jar 应排在最前，便于优先展示进度"
    );
    assert!(plan.contains_path("assets/log_configs/client-1.21.2.xml"));
    assert!(plan.contains_path("at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"));

    assert!(
        plan.tasks.iter().all(|task| task.sha1.len() == 40),
        "每个任务都必须带 40 位 SHA-1，否则无法校验"
    );
    assert!(
        plan.tasks
            .iter()
            .all(|task| task.url.starts_with("https://")),
        "官方下载地址均为 HTTPS"
    );

    let expected_total: u64 = plan.tasks.iter().map(|task| task.size).sum();
    assert_eq!(plan.total_bytes(), expected_total);

    let unique: std::collections::BTreeSet<&String> =
        plan.tasks.iter().map(|task| &task.path).collect();
    assert_eq!(unique.len(), plan.len(), "落地路径不应重复");
}

#[test]
fn plan_for_legacy_version_includes_natives() {
    let meta = parse(META_LEGACY);
    let plan = plan_version_files(&meta, &windows_11(), "versions/1.7.10");

    // 客户端 jar + 2 个库 + 1 个 natives；旧版本没有日志配置。
    assert_eq!(plan.len(), 4);
    assert!(
        plan.contains_path("org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1-natives-windows.jar"),
        "natives 构件必须纳入下载计划"
    );
    assert!(
        !plan
            .tasks
            .iter()
            .any(|task| task.path.contains("log_configs")),
        "1.7.10 没有 log4j2 配置"
    );
}

#[test]
fn plan_deduplicates_repeated_libraries() {
    let raw = r#"
    {
      "id": "test",
      "type": "release",
      "mainClass": "a.B",
      "releaseTime": "t",
      "time": "t",
      "libraries": [
        { "name": "g:a:1", "downloads": { "artifact": { "path": "g/a/1/a-1.jar", "sha1": "aaaa", "size": 10, "url": "https://example.invalid/a.jar" } } },
        { "name": "g:a:1", "downloads": { "artifact": { "path": "g/a/1/a-1.jar", "sha1": "aaaa", "size": 10, "url": "https://example.invalid/a.jar" } } }
      ]
    }"#;
    let meta = parse(raw);
    let plan = plan_version_files(&meta, &windows_11(), "versions/test");

    assert_eq!(plan.len(), 1);
    assert_eq!(plan.total_bytes(), 10);
}

#[test]
fn tasks_without_sha1_are_dropped() {
    // 没有 SHA-1 就无法校验，宁可不下载也不能引入未校验的文件。
    let raw = r#"
    {
      "id": "test",
      "type": "release",
      "mainClass": "a.B",
      "releaseTime": "t",
      "time": "t",
      "libraries": [
        { "name": "g:a:1", "downloads": { "artifact": { "path": "g/a/1/a-1.jar", "sha1": "", "size": 10, "url": "https://example.invalid/a.jar" } } }
      ]
    }"#;
    let meta = parse(raw);
    assert!(plan_version_files(&meta, &windows_11(), "versions/test").is_empty());
}

#[test]
fn sha1_matches_known_vectors() {
    assert_eq!(hex_sha1(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(hex_sha1(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");

    assert!(verify_sha1(
        "a9993e364706816aba3e25717850c26c9cd0d89d",
        b"abc"
    ));
    assert!(
        verify_sha1("A9993E364706816ABA3E25717850C26C9CD0D89D", b"abc"),
        "官方摘要大小写不固定，比较必须忽略大小写"
    );
    assert!(!verify_sha1(
        "da39a3ee5e6b4b0d3255bfef95601890afd80709",
        b"abc"
    ));
}

#[test]
fn empty_expected_digest_never_passes() {
    assert!(
        !verify_sha1("", b"anything"),
        "空摘要不构成校验，必须判为失败而不是通过"
    );
}

#[test]
fn chunked_hashing_matches_single_shot() {
    let data = b"chunked hashing must agree with single shot";

    let mut hasher = Sha1Hasher::new();
    for chunk in data.chunks(7) {
        hasher.update(chunk);
    }
    assert_eq!(hasher.finish(), hex_sha1(data));

    let mut verifier = Sha1Hasher::new();
    for chunk in data.chunks(5) {
        verifier.update(chunk);
    }
    assert!(verifier.finish_and_verify(&hex_sha1(data)));
}

#[test]
fn retry_policy_backs_off_exponentially() {
    let policy = RetryPolicy::new(4, Duration::from_millis(100), Duration::from_secs(1));

    assert_eq!(policy.max_attempts(), 4);
    assert_eq!(policy.delay_for(1), Some(Duration::from_millis(100)));
    assert_eq!(policy.delay_for(2), Some(Duration::from_millis(200)));
    assert_eq!(policy.delay_for(3), Some(Duration::from_millis(400)));
    assert_eq!(policy.delay_for(4), None, "达到上限后不再重试");
}

#[test]
fn retry_delay_is_capped() {
    let policy = RetryPolicy::new(10, Duration::from_secs(1), Duration::from_secs(3));

    assert_eq!(policy.delay_for(1), Some(Duration::from_secs(1)));
    assert_eq!(policy.delay_for(2), Some(Duration::from_secs(2)));
    assert_eq!(
        policy.delay_for(3),
        Some(Duration::from_secs(3)),
        "超过上限应被封顶，而不是无限增长"
    );
    assert_eq!(policy.delay_for(9), Some(Duration::from_secs(3)));
}

#[test]
fn zero_attempts_is_clamped_to_one() {
    let policy = RetryPolicy::new(0, Duration::from_millis(10), Duration::from_millis(10));
    assert_eq!(policy.max_attempts(), 1);
    assert!(!policy.should_retry(1));
    assert_eq!(policy.delay_for(1), None);
}

#[test]
fn retryable_statuses_exclude_client_errors() {
    for status in [500, 502, 503, 504, 408, 429] {
        assert!(is_retryable_status(status), "{status} 应可重试");
    }
    for status in [200, 301, 400, 403, 404, 410] {
        assert!(!is_retryable_status(status), "{status} 不应重试");
    }
}
