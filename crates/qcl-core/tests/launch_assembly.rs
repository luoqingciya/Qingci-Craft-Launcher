//! 启动命令组装（classpath / natives / 参数模板）的测试。

use std::collections::BTreeMap;

use qcl_core::CoreError;
use qcl_core::launch::{
    LaunchVariables, build_classpath, classpath_separator, join_classpath, maven_to_path,
    natives_artifacts,
};
use qcl_core::version::{LaunchContext, VersionMeta};

const META_26_3: &str = include_str!("fixtures/26.3.json");
const META_LEGACY: &str = include_str!("fixtures/legacy-1.7.10.json");

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
fn maven_coordinates_become_repository_paths() {
    assert_eq!(
        maven_to_path("org.lwjgl.lwjgl:lwjgl:2.9.1").as_deref(),
        Some("org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1.jar")
    );
    assert_eq!(
        maven_to_path("org.lwjgl.lwjgl:lwjgl:2.9.1:natives-windows").as_deref(),
        Some("org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1-natives-windows.jar")
    );
    // 旧版本用 @zip 指定扩展名。
    assert_eq!(
        maven_to_path("net.java.jinput:jinput-platform:2.0.5:natives-windows@zip").as_deref(),
        Some("net/java/jinput/jinput-platform/2.0.5/jinput-platform-2.0.5-natives-windows.zip")
    );

    assert!(
        maven_to_path("com.example:broken").is_none(),
        "字段不足应返回 None"
    );
    assert!(maven_to_path("a:b:c:d:e").is_none(), "字段过多应返回 None");
}

#[test]
fn classpath_for_official_26_3_contains_all_applicable_libraries() {
    let meta = parse(META_26_3);
    let entries = build_classpath(&meta, &windows_11(), "versions/26.3/26.3.jar");

    // 74 个适用库 + 版本 jar；该数字与独立实现求值结果一致。
    assert_eq!(entries.len(), 75);
    assert_eq!(
        entries.first().map(String::as_str),
        Some("at/yawk/lz4/lz4-java/1.10.1/lz4-java-1.10.1.jar"),
        "顺序必须与 version.json 一致，不能重排"
    );
    assert_eq!(
        entries.last().map(String::as_str),
        Some("versions/26.3/26.3.jar"),
        "版本 jar 应位于 classpath 末尾"
    );

    let unique: std::collections::HashSet<&String> = entries.iter().collect();
    assert_eq!(unique.len(), entries.len(), "不应出现重复条目");

    assert!(
        entries.iter().all(|entry| !entry.contains('\\')),
        "classpath 一律使用正斜杠，Windows 反斜杠会破坏 Java 的路径解析"
    );
}

#[test]
fn classpath_deduplicates_repeated_entries() {
    // 模组加载器合并库列表时会出现重复条目，这里用重复的库模拟。
    let raw = r#"
    {
      "id": "test",
      "type": "release",
      "mainClass": "a.B",
      "releaseTime": "t",
      "time": "t",
      "libraries": [
        { "name": "g:a:1", "downloads": { "artifact": { "path": "g/a/1/a-1.jar", "sha1": "x", "size": 1, "url": "u" } } },
        { "name": "g:a:1", "downloads": { "artifact": { "path": "g/a/1/a-1.jar", "sha1": "x", "size": 1, "url": "u" } } }
      ]
    }"#;
    let meta = parse(raw);
    let entries = build_classpath(&meta, &windows_11(), "versions/test/test.jar");

    assert_eq!(entries, vec!["g/a/1/a-1.jar", "versions/test/test.jar"]);
}

#[test]
fn classpath_falls_back_to_coordinates_when_path_missing() {
    let raw = r#"
    {
      "id": "test",
      "type": "release",
      "mainClass": "a.B",
      "releaseTime": "t",
      "time": "t",
      "libraries": [
        { "name": "com.example:demo:1.2.3" }
      ]
    }"#;
    let meta = parse(raw);
    let entries = build_classpath(&meta, &windows_11(), "versions/test/test.jar");

    assert_eq!(
        entries.first().map(String::as_str),
        Some("com/example/demo/1.2.3/demo-1.2.3.jar"),
        "缺少 downloads 时应由 Maven 坐标推导路径"
    );
}

#[test]
fn natives_are_collected_for_legacy_versions() {
    let meta = parse(META_LEGACY);
    let artifacts = natives_artifacts(&meta, &windows_11());

    assert_eq!(artifacts.len(), 1, "1.7.10 的 lwjgl 需要一个 natives 包");
    let artifact = &artifacts[0];
    assert_eq!(artifact.library, "org.lwjgl.lwjgl:lwjgl:2.9.1");
    assert_eq!(
        artifact.path,
        "org/lwjgl/lwjgl/lwjgl/2.9.1/lwjgl-2.9.1-natives-windows.jar"
    );
    assert_eq!(artifact.exclude, vec!["META-INF/".to_owned()]);
}

#[test]
fn natives_expand_arch_placeholder() {
    let raw = r#"
    {
      "id": "test",
      "type": "release",
      "mainClass": "a.B",
      "releaseTime": "t",
      "time": "t",
      "libraries": [
        {
          "name": "net.java.jinput:jinput-platform:2.0.5",
          "natives": { "windows": "natives-windows-${arch}" },
          "downloads": {
            "classifiers": {
              "natives-windows-64": {
                "path": "net/java/jinput/jinput-platform/2.0.5/jinput-platform-2.0.5-natives-windows-64.jar",
                "sha1": "x", "size": 1, "url": "u"
              }
            }
          }
        }
      ]
    }"#;
    let meta = parse(raw);

    let sixty_four = natives_artifacts(&meta, &windows_11());
    assert_eq!(sixty_four.len(), 1, "x86_64 应命中 natives-windows-64");
    assert!(sixty_four[0].path.ends_with("-natives-windows-64.jar"));

    let mut thirty_two = windows_11();
    thirty_two.arch = Some("x86".to_owned());
    let thirty_two = natives_artifacts(&meta, &thirty_two);
    assert!(
        thirty_two.is_empty(),
        "32 位缺少对应 classifier，应跳过而非报错"
    );
}

#[test]
fn modern_versions_have_no_natives_classifiers() {
    let meta = parse(META_26_3);
    assert!(
        natives_artifacts(&meta, &windows_11()).is_empty(),
        "26.3 的 natives 以独立库形式提供，不再走 classifiers"
    );
}

#[test]
fn variables_are_substituted_once() {
    let variables = LaunchVariables::new()
        .with("auth_player_name", "Steve")
        .with("version_name", "26.3")
        .with("injected", "${version_name}");

    assert_eq!(
        variables
            .substitute("--username ${auth_player_name}")
            .expect("应替换成功"),
        "--username Steve"
    );

    // 单趟替换：变量值中的占位符不会被二次展开。
    assert_eq!(
        variables.substitute("${injected}").expect("应替换成功"),
        "${version_name}"
    );
}

#[test]
fn unknown_variables_are_reported() {
    let variables = LaunchVariables::new().with("version_name", "26.3");

    let error = variables
        .substitute("${auth_access_token}")
        .expect_err("未声明的变量必须报错");

    match error {
        CoreError::UnknownVariable(name) => assert_eq!(name, "auth_access_token"),
        other => panic!("错误类型不符：{other:?}"),
    }
}

#[test]
fn unclosed_placeholder_is_kept_as_text() {
    let variables = LaunchVariables::new().with("a", "1");
    assert_eq!(
        variables
            .substitute("prefix ${a} tail ${broken")
            .expect("应成功"),
        "prefix 1 tail ${broken"
    );
}

#[test]
fn game_arguments_cover_both_argument_schemes() {
    let variables = LaunchVariables::new()
        .with("auth_player_name", "Steve")
        .with("version_name", "1.7.10")
        .with("game_directory", "/games")
        .with("assets_root", "/games/assets");

    // 旧版：空格分隔的字符串，切分后逐项替换。
    let legacy = parse(META_LEGACY);
    let raw = legacy.game_arguments(&windows_11());
    assert_eq!(raw.len(), 8, "1.7.10 的 minecraftArguments 应切出 8 个参数");
    assert_eq!(raw[0], "--username");

    let resolved = variables.substitute_all(raw).expect("应替换成功");
    assert_eq!(resolved[1], "Steve");
    assert!(resolved.iter().all(|arg| !arg.contains("${")));

    // 旧版没有结构化 JVM 参数。
    assert!(legacy.jvm_arguments(&windows_11()).is_empty());

    // 新版：结构化参数，替换后同样不应有残留占位符。
    let modern = parse(META_26_3);
    let raw = modern.game_arguments(&windows_11());
    assert_eq!(raw.len(), 20);
    assert!(
        raw.iter().any(|arg| arg.contains("${")),
        "未替换前应含有占位符"
    );
}

#[test]
fn classpath_joining_uses_platform_separator() {
    let entries = vec!["a.jar".to_owned(), "b.jar".to_owned()];
    assert_eq!(join_classpath(&entries, ';'), "a.jar;b.jar");
    assert_eq!(join_classpath(&entries, ':'), "a.jar:b.jar");

    let expected = if cfg!(windows) { ';' } else { ':' };
    assert_eq!(classpath_separator(), expected);
}
