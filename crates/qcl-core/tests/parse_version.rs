//! 版本元数据解析的集成测试。
//!
//! 夹具说明：
//!
//! - `fixtures/26.3.json`：2026-10-24 从 `piston-meta` 拉取的官方文件，未作任何修改。
//!   用于验证解析器与**真实数据**的兼容性，而不是与自己的假设兼容。
//! - `fixtures/legacy-1.7.10.json`：手工构造，覆盖 1.13 之前的字段形态
//!   （`minecraftArguments`、`natives`、`classifiers`、`extract`）。

use qcl_core::version::{Argument, ArgumentValue, VersionManifest, VersionMeta, VersionType};

const META_26_3: &str = include_str!("fixtures/26.3.json");
const META_LEGACY: &str = include_str!("fixtures/legacy-1.7.10.json");

#[test]
fn parses_official_26_3_metadata() {
    let meta: VersionMeta = serde_json::from_str(META_26_3).expect("26.3 应可解析");

    assert_eq!(meta.id, "26.3");
    assert_eq!(meta.version_type, VersionType::Release);
    assert_eq!(meta.main_class, "net.minecraft.client.main.Main");
    assert_eq!(meta.minimum_launcher_version, Some(21));
    assert_eq!(meta.compliance_level, Some(1));
    assert!(!meta.uses_legacy_arguments());

    assert_eq!(meta.required_java_major(), Some(25));
    let java = meta.java_version.as_ref().expect("26.3 应有 javaVersion");
    assert_eq!(java.component.as_deref(), Some("java-runtime-epsilon"));

    let index = meta.asset_index.as_ref().expect("26.3 应有 assetIndex");
    assert_eq!(index.id, "34");
    assert_eq!(index.total_size, 484_894_038);

    let client = meta
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
        .expect("26.3 应有 client jar");
    assert_eq!(client.size, 41_483_720);
    assert!(client.path.is_none(), "客户端 jar 不应带 path 字段");

    assert_eq!(meta.libraries.len(), 114);
    let with_rules = meta
        .libraries
        .iter()
        .filter(|library| library.rules.is_some())
        .count();
    assert_eq!(with_rules, 61);

    let arguments = meta.arguments.as_ref().expect("26.3 应有 arguments");
    assert_eq!(arguments.game.len(), 26);
    assert_eq!(arguments.jvm.len(), 15);
    let conditional = arguments
        .game
        .iter()
        .filter(|arg| matches!(arg, Argument::Conditional { .. }))
        .count();
    assert_eq!(conditional, 6);

    let logging = meta.logging.as_ref().expect("26.3 应有 logging");
    let logging_client = logging.client.as_ref().expect("应有客户端日志配置");
    assert_eq!(logging_client.file.id.as_deref(), Some("client-1.21.2.xml"));
    assert_eq!(logging_client.kind, "log4j2-xml");

    // 26.3 起官方自带默认 JVM 参数，其中包含默认堆内存。
    // 启动器在用户未显式设置内存时应采用它，而不是自行硬编码一个默认值。
    let defaults = arguments
        .default_user_jvm
        .as_ref()
        .expect("26.3 应有 default-user-jvm");
    assert_eq!(defaults.len(), 3);

    let Argument::Conditional {
        rules: first_rules,
        value: first_value,
    } = &defaults[0]
    else {
        panic!("default-user-jvm 首项应为 value 形态");
    };
    assert!(first_rules.is_none(), "首项不带规则，表示无条件生效");
    assert_eq!(
        &first_value.as_slice()[..2],
        ["-Xms2G".to_owned(), "-Xmx4G".to_owned()].as_slice()
    );

    // 新版本用 versionRange 取代旧的正则表达式描述系统版本。
    let uses_version_range = defaults.iter().any(|arg| match arg {
        Argument::Conditional {
            rules: Some(rules), ..
        } => rules.iter().any(|rule| {
            rule.os
                .as_ref()
                .and_then(|os| os.version_range.as_ref())
                .is_some()
        }),
        _ => false,
    });
    assert!(
        uses_version_range,
        "default-user-jvm 应使用 versionRange 形式"
    );
}

#[test]
fn parses_legacy_1_7_10_metadata() {
    let meta: VersionMeta = serde_json::from_str(META_LEGACY).expect("1.7.10 应可解析");

    assert_eq!(meta.id, "1.7.10");
    assert!(
        meta.uses_legacy_arguments(),
        "1.7.10 应走 minecraftArguments 路径"
    );
    assert!(meta.arguments.is_none());
    assert!(meta.java_version.is_none(), "旧版本没有 javaVersion 字段");

    let lwjgl = meta
        .libraries
        .iter()
        .find(|library| library.name.contains("lwjgl"))
        .expect("应有 lwjgl 依赖");

    let natives = lwjgl.natives.as_ref().expect("lwjgl 应有 natives 映射");
    assert_eq!(
        natives.get("windows").map(String::as_str),
        Some("natives-windows")
    );

    let classifiers = lwjgl
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.classifiers.as_ref())
        .expect("lwjgl 应有 classifiers");
    assert_eq!(classifiers.len(), 2);

    let exclude = &lwjgl.extract.as_ref().expect("lwjgl 应有 extract").exclude;
    assert_eq!(exclude.len(), 1);
    assert_eq!(exclude[0], "META-INF/");

    insta::assert_json_snapshot!(meta);
}

#[test]
fn unknown_version_type_is_preserved_instead_of_failing() {
    let json = r#"
    {
      "latest": { "release": "26.3", "snapshot": "26.4-snapshot-5" },
      "versions": [
        {
          "id": "26.3",
          "type": "release",
          "url": "https://example.invalid/26.3.json",
          "time": "2026-09-15T00:00:00+00:00",
          "releaseTime": "2026-09-15T00:00:00+00:00",
          "sha1": "abc",
          "complianceLevel": 1
        },
        {
          "id": "99.0",
          "type": "experimental",
          "url": "https://example.invalid/99.0.json",
          "time": "2027-01-01T00:00:00+00:00",
          "releaseTime": "2027-01-01T00:00:00+00:00"
        }
      ]
    }"#;

    let manifest: VersionManifest =
        serde_json::from_str(json).expect("未知的 type 取值不应导致整份清单解析失败");

    assert_eq!(manifest.versions.len(), 2);
    assert_eq!(
        manifest.versions[1].version_type,
        VersionType::Other("experimental".to_owned())
    );
    assert!(
        manifest.versions[1].sha1.is_none(),
        "v2 专属字段缺失时应为 None"
    );
    assert!(manifest.versions[1].compliance_level.is_none());
}

#[test]
fn manifest_lookup_follows_latest_pointer_not_list_order() {
    let json = r#"
    {
      "latest": { "release": "26.2", "snapshot": "26.4-snapshot-5" },
      "versions": [
        { "id": "26.4-snapshot-5", "type": "snapshot", "url": "u1", "time": "t1", "releaseTime": "t1" },
        { "id": "26.2", "type": "release", "url": "u2", "time": "t2", "releaseTime": "t2" },
        { "id": "26.3", "type": "release", "url": "u3", "time": "t3", "releaseTime": "t3" }
      ]
    }"#;

    let manifest: VersionManifest = serde_json::from_str(json).expect("应可解析");

    // 清单首项是快照，若按顺序取第一个 release 会错拿到 26.2 之前/之后的版本，
    // 因此必须以 latest.release 指针为准。
    assert_eq!(
        manifest.latest_release().map(|entry| entry.id.as_str()),
        Some("26.2")
    );
    assert!(manifest.find("26.3").is_some());
    assert!(manifest.find("1.0").is_none());
}

#[test]
fn version_type_roundtrips_to_original_string() {
    for raw in [
        "release",
        "snapshot",
        "old_beta",
        "old_alpha",
        "experimental",
    ] {
        let json = format!("\"{raw}\"");
        let parsed: VersionType = serde_json::from_str(&json).expect("应可解析");
        assert_eq!(parsed.as_str(), raw);
        assert_eq!(
            serde_json::to_string(&parsed).expect("应可序列化"),
            json,
            "序列化应还原原始字符串"
        );
    }
}

#[test]
fn argument_value_exposes_both_shapes() {
    let single = ArgumentValue::Single("--demo".to_owned());
    assert_eq!(single.as_slice(), ["--demo".to_owned()].as_slice());

    let many = ArgumentValue::Many(vec!["--width".to_owned(), "854".to_owned()]);
    assert_eq!(
        many.as_slice(),
        ["--width".to_owned(), "854".to_owned()].as_slice()
    );
}

/// 往返一致性：解析后再序列化，必须与官方原始 JSON 完全等价。
///
/// 这个测试专门防「serde 字段名写错」这一类缺陷——`complianceLevel`、`javaVersion`
/// 这类驼峰字段一旦漏写 `rename`，反序列化会静默得到 `None`，常规断言很容易漏掉。
/// 它同时充当上游 schema 漂移的探测器：Mojang 新增字段时该测试会失败，提醒开发者做决策。
#[test]
fn official_fixture_roundtrips_without_field_loss() {
    for (name, raw) in [("26.3", META_26_3), ("1.7.10", META_LEGACY)] {
        let original: serde_json::Value = serde_json::from_str(raw).expect("夹具应是合法 JSON");
        let meta: VersionMeta = serde_json::from_str(raw).expect("应可解析");
        let roundtripped = serde_json::to_value(&meta).expect("应可序列化");

        assert_json_equivalent(&roundtripped, &original, name);
    }
}

/// 逐字段比对，失败时只输出不一致的字段，避免整份 JSON 淹没真正的问题。
fn assert_json_equivalent(actual: &serde_json::Value, expected: &serde_json::Value, name: &str) {
    let (serde_json::Value::Object(actual_map), serde_json::Value::Object(expected_map)) =
        (actual, expected)
    else {
        panic!("夹具 {name} 顶层应为对象");
    };

    let mut keys: Vec<&String> = actual_map.keys().chain(expected_map.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut mismatches = Vec::new();
    for key in keys {
        let actual_value = actual_map.get(key);
        let expected_value = expected_map.get(key);
        if actual_value != expected_value {
            mismatches.push(format!(
                "  - `{key}`\n      实际: {}\n      期望: {}",
                brief(actual_value),
                brief(expected_value)
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "夹具 {name} 往返后以下字段不一致：\n{}",
        mismatches.join("\n")
    );
}

/// 截断过长的 JSON 片段，保证失败信息可读。
fn brief(value: Option<&serde_json::Value>) -> String {
    let text = match value {
        Some(value) => value.to_string(),
        None => "<缺失>".to_owned(),
    };
    if text.chars().count() <= 160 {
        text
    } else {
        let truncated: String = text.chars().take(160).collect();
        format!("{truncated}…")
    }
}
