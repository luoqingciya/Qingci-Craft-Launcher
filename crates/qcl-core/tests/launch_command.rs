//! 启动命令组装的测试。
//!
//! 断言的是**参数序列**而不是拼接后的字符串：空格与中文路径的引号问题
//! 只有在按数组传参时才彻底消失，所以这里也按数组验证。

use std::collections::BTreeMap;

use qcl_core::launch::{
    AccountInfo, LaunchLayout, LaunchOptions, build_launch_command, classpath_separator,
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

fn account() -> AccountInfo {
    AccountInfo {
        player_name: "Steve".to_owned(),
        uuid: "069a79f444e94726a5befca90e38aaf5".to_owned(),
        access_token: "token-value".to_owned(),
        user_type: "msa".to_owned(),
        client_id: Some("client-id".to_owned()),
        xuid: Some("xuid-value".to_owned()),
    }
}

fn parse(raw: &str) -> VersionMeta {
    serde_json::from_str(raw).expect("夹具应可解析")
}

#[test]
fn modern_command_has_no_unresolved_placeholders() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let command = build_launch_command(
        "C:/java/bin/java.exe",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    assert!(
        command.args.iter().all(|arg| !arg.contains("${")),
        "命令行里不能残留任何未替换的占位符：{:?}",
        command
            .args
            .iter()
            .filter(|arg| arg.contains("${"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn main_class_sits_between_jvm_and_game_arguments() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let command = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    let main_class = command
        .args
        .iter()
        .position(|arg| arg == "net.minecraft.client.main.Main")
        .expect("应包含主类");

    // 主类之前是 JVM 参数，之后是游戏参数。
    assert!(main_class > 0);
    assert!(
        command.args[..main_class].iter().any(|arg| arg == "-cp"),
        "主类之前应有 classpath 参数"
    );
    assert!(
        command.args[main_class + 1..]
            .windows(2)
            .any(|pair| pair[0] == "--username" && pair[1] == "Steve"),
        "主类之后应出现游戏参数"
    );
}

#[test]
fn classpath_contains_libraries_and_version_jar() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let command = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    let cp_index = command
        .args
        .iter()
        .position(|arg| arg == "-cp")
        .expect("应有 -cp");
    let classpath = &command.args[cp_index + 1];

    let separator = classpath_separator();
    let entries: Vec<&str> = classpath.split(separator).collect();

    assert_eq!(entries.len(), 75, "74 个库 + 版本 jar");
    assert_eq!(entries.last().copied(), Some("versions/26.3/26.3.jar"));
}

#[test]
fn extra_classpath_entries_are_appended_for_loaders() {
    let meta = parse(META_26_3);
    let mut layout = LaunchLayout::standard("26.3");
    layout.classpath_extra =
        vec!["libraries/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar".to_owned()];

    let command = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    let cp_index = command.args.iter().position(|arg| arg == "-cp").unwrap();
    let classpath = &command.args[cp_index + 1];

    assert!(
        classpath.ends_with("fabric-loader-0.20.1.jar"),
        "模组加载器的库应排在版本 jar 之后"
    );
}

#[test]
fn official_default_memory_is_used_when_user_sets_nothing() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let command = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    assert!(
        command.args.iter().any(|arg| arg == "-Xms2G"),
        "未指定内存时应采用官方的 default-user-jvm"
    );
    assert!(command.args.iter().any(|arg| arg == "-Xmx4G"));
}

#[test]
fn gc_arguments_depend_on_system_version() {
    // 26.3 的 default-user-jvm 用 versionRange 按 Windows 版本选 GC：
    // 分界点是 10.0.17134（Windows 10 1803）。缺系统版本号时两条分支都不命中，
    // 命令行里一个 GC 参数都没有——这正是补系统版本探测的原因。
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let modern = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");
    assert!(
        modern.args.iter().any(|arg| arg == "-XX:+UseZGC"),
        "Windows 10 1803 之后应使用 ZGC"
    );

    let mut legacy = windows_11();
    legacy.os_version = Some("6.1.7601".to_owned());
    let legacy_command = build_launch_command(
        "java",
        &meta,
        &legacy,
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");
    assert!(
        legacy_command.args.iter().any(|arg| arg == "-XX:+UseG1GC"),
        "旧版 Windows 应退回 G1"
    );
    assert!(
        !legacy_command.args.iter().any(|arg| arg == "-XX:+UseZGC"),
        "两个分支互斥，不能同时出现"
    );

    let mut unknown = windows_11();
    unknown.os_version = None;
    let unknown_command = build_launch_command(
        "java",
        &meta,
        &unknown,
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");
    assert!(
        !unknown_command
            .args
            .iter()
            .any(|arg| arg.contains("UseZGC") || arg.contains("UseG1GC")),
        "系统版本未知时不应猜测 GC——宁可留给 JVM 默认值"
    );
}

#[test]
fn user_memory_overrides_official_defaults() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let options = LaunchOptions {
        memory_mb: Some(8192),
        ..LaunchOptions::default()
    };
    let command = build_launch_command("java", &meta, &windows_11(), &layout, &account(), &options)
        .expect("应能组装命令");

    assert!(command.args.iter().any(|arg| arg == "-Xmx8192M"));
    assert!(
        !command.args.iter().any(|arg| arg == "-Xmx4G"),
        "用户指定内存后不应再叠加官方默认值，否则命令行里会出现两份堆设置"
    );
    assert!(
        !command.args.iter().any(|arg| arg == "-Xms2G"),
        "官方默认的初始堆同样应被跳过"
    );
}

#[test]
fn user_jvm_args_come_after_official_ones() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let options = LaunchOptions {
        extra_jvm_args: vec!["-XX:+UseSerialGC".to_owned()],
        ..LaunchOptions::default()
    };
    let command = build_launch_command("java", &meta, &windows_11(), &layout, &account(), &options)
        .expect("应能组装命令");

    let serial_gc = command
        .args
        .iter()
        .position(|arg| arg == "-XX:+UseSerialGC")
        .unwrap();
    let main_class = command
        .args
        .iter()
        .position(|arg| arg == "net.minecraft.client.main.Main")
        .unwrap();

    assert!(serial_gc < main_class, "用户 JVM 参数必须排在主类之前");
    assert!(
        serial_gc > 0 && command.args[..serial_gc].iter().any(|arg| arg == "-Xms2G"),
        "用户参数应排在清单默认参数之后，才能覆盖同名设置"
    );
}

#[test]
fn legacy_versions_get_launcher_provided_jvm_arguments() {
    let meta = parse(META_LEGACY);
    let layout = LaunchLayout::standard("1.7.10");

    let command = build_launch_command(
        "java",
        &meta,
        &windows_11(),
        &layout,
        &account(),
        &LaunchOptions::default(),
    )
    .expect("应能组装命令");

    // 1.7.10 的 version.json 没有结构化 JVM 参数，启动器必须补上这两项。
    assert!(
        command
            .args
            .iter()
            .any(|arg| arg == "-Djava.library.path=versions/1.7.10/natives"),
        "旧版本必须由启动器提供 natives 路径"
    );
    assert!(command.args.iter().any(|arg| arg == "-cp"));
    assert!(
        command
            .args
            .iter()
            .any(|arg| arg == "net.minecraft.client.main.Main")
    );

    // 旧版 minecraftArguments 有 8 个参数，替换后应原样出现在主类之后。
    let main_class = command
        .args
        .iter()
        .position(|arg| arg == "net.minecraft.client.main.Main")
        .unwrap();
    let game_args = &command.args[main_class + 1..];
    assert_eq!(game_args.len(), 8);
    assert_eq!(game_args[0], "--username");
    assert_eq!(game_args[1], "Steve");
    assert!(
        game_args.iter().all(|arg| !arg.contains("${")),
        "旧版参数同样不能残留占位符"
    );
}

#[test]
fn custom_resolution_is_passed_when_enabled() {
    let meta = parse(META_26_3);
    let layout = LaunchLayout::standard("26.3");

    let context = windows_11().with_feature("has_custom_resolution", true);
    let options = LaunchOptions {
        resolution: Some((1920, 1080)),
        ..LaunchOptions::default()
    };

    let command = build_launch_command("java", &meta, &context, &layout, &account(), &options)
        .expect("应能组装命令");

    let width = command
        .args
        .iter()
        .position(|arg| arg == "--width")
        .expect("应包含 --width");
    assert_eq!(command.args[width + 1], "1920");
    let height = command
        .args
        .iter()
        .position(|arg| arg == "--height")
        .expect("应包含 --height");
    assert_eq!(command.args[height + 1], "1080");
}
