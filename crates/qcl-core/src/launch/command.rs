// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 把元数据、classpath 与参数模板组装成一条可直接执行的启动命令。
//!
//! 这是核心模块的收口：前面各模块产出的路径、参数、变量在这里合成最终形态。
//! 本模块仍然**不启动进程**，只产出一个可断言的数据结构——这样命令行是否正确
//! 可以在任何平台上验证，而不需要真的把游戏跑起来。

use std::path::PathBuf;

use crate::error::Result;
use crate::launch::classpath::{build_classpath, classpath_separator, join_classpath};
use crate::launch::template::LaunchVariables;
use crate::version::{LaunchContext, VersionMeta};

/// 目录布局。路径均为**相对游戏目录**的形式，classpath 需要这种形态。
#[derive(Debug, Clone)]
pub struct LaunchLayout {
    /// 游戏根目录（相对路径，通常为 `.`）。启动器会以此为工作目录启动进程。
    pub game_dir: String,
    /// 版本目录，如 `versions/26.3`。
    pub version_dir: String,
    /// 资源根目录，如 `assets`。
    pub assets_dir: String,
    /// natives 解压目录，如 `versions/26.3/natives`。
    pub natives_dir: String,
    /// 依赖库根目录，如 `libraries`。
    ///
    /// Forge / NeoForge 的 JVM 参数用 `${library_directory}` 引用它（模块路径与
    /// `-DlibraryDirectory` 都靠这个），原版参数里没有这个变量。
    pub libraries_dir: String,
    /// 追加到 classpath 末尾的条目（模组加载器的库）。
    pub classpath_extra: Vec<String>,
}

/// 账户信息。
#[derive(Debug, Clone)]
pub struct AccountInfo {
    /// 玩家名。
    pub player_name: String,
    /// 玩家 UUID（无连字符）。
    pub uuid: String,
    /// 访问令牌。离线模式下可为空字符串。
    pub access_token: String,
    /// 账户类型：`msa` / `legacy` / `offline`。
    pub user_type: String,
    /// 客户端 ID。
    pub client_id: Option<String>,
    /// Xbox 用户 ID。
    pub xuid: Option<String>,
}

/// 启动选项。
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// 最大堆内存（MB）。为 `None` 时采用官方 `default-user-jvm` 的默认值。
    pub memory_mb: Option<u32>,
    /// 窗口分辨率。仅当调用方开启 `has_custom_resolution` 功能开关时才有意义。
    pub resolution: Option<(u32, u32)>,
    /// 追加的 JVM 参数，排在清单参数之后，因此可以覆盖清单里的同名设置。
    pub extra_jvm_args: Vec<String>,
    /// 追加的游戏参数，排在最后。
    pub extra_game_args: Vec<String>,
}

/// 组装完成的启动命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    /// Java 可执行文件。
    pub executable: PathBuf,
    /// 命令行参数（不含可执行文件本身）。
    ///
    /// 以数组而非字符串保存：路径中的空格与中文是最常见的启动失败原因，
    /// 交给操作系统按数组传参可以从根本上避免拼接引号的问题。
    pub args: Vec<String>,
}

/// 组装启动命令。
///
/// 参数顺序遵循官方启动器：清单 JVM 参数 → 默认用户 JVM 参数或用户内存设置 →
/// 用户追加 JVM 参数 → 主类 → 游戏参数 → 用户追加游戏参数。
///
/// 这个顺序有意义：JVM 对 `-Xmx` 取**最后一次**出现的值，用户设置必须排在清单默认值之后
/// 才能生效。
pub fn build_launch_command(
    java: impl Into<PathBuf>,
    meta: &VersionMeta,
    context: &LaunchContext,
    layout: &LaunchLayout,
    account: &AccountInfo,
    options: &LaunchOptions,
) -> Result<LaunchCommand> {
    let version_jar = format!("{}/{}.jar", layout.version_dir, meta.id);
    let mut classpath = build_classpath(meta, context, &version_jar);
    classpath.extend(layout.classpath_extra.iter().cloned());

    let variables = build_variables(meta, layout, account, options, &classpath);

    let mut args = resolve_jvm_arguments(meta, context, &variables)?;
    apply_memory_settings(meta, context, options, &variables, &mut args)?;
    args.extend(options.extra_jvm_args.iter().cloned());
    args.push(meta.main_class.clone());

    args.extend(variables.substitute_all(meta.game_arguments(context))?);
    args.extend(options.extra_game_args.iter().cloned());

    Ok(LaunchCommand {
        executable: java.into(),
        args,
    })
}

fn build_variables(
    meta: &VersionMeta,
    layout: &LaunchLayout,
    account: &AccountInfo,
    options: &LaunchOptions,
    classpath: &[String],
) -> LaunchVariables {
    let assets_index = meta
        .asset_index
        .as_ref()
        .map(|index| index.id.clone())
        .or_else(|| meta.assets.clone())
        .unwrap_or_default();

    let (width, height) = options.resolution.unwrap_or((854, 480));

    LaunchVariables::new()
        .with("auth_player_name", account.player_name.as_str())
        .with("auth_uuid", account.uuid.as_str())
        .with("auth_access_token", account.access_token.as_str())
        // 1.6 及更早的版本使用 ${auth_session}，格式为 `token:<令牌>:<UUID>`。
        .with(
            "auth_session",
            format!("token:{}:{}", account.access_token, account.uuid),
        )
        .with("user_type", account.user_type.as_str())
        .with("clientid", account.client_id.clone().unwrap_or_default())
        .with("auth_xuid", account.xuid.clone().unwrap_or_default())
        .with("version_name", meta.id.as_str())
        .with("version_type", meta.version_type.as_str())
        .with("game_directory", layout.game_dir.as_str())
        .with("assets_root", layout.assets_dir.as_str())
        .with("assets_index_name", assets_index)
        .with("natives_directory", layout.natives_dir.as_str())
        .with("launcher_name", crate::LAUNCHER_NAME)
        .with("launcher_version", crate::LAUNCHER_VERSION)
        .with("classpath", join_classpath(classpath, classpath_separator()))
        // Forge / NeoForge 的模块路径用这两个变量拼；原版 JSON 里不出现，缺了会直接
        // 报 `UnknownVariable`。分隔符随平台变，所以同样要作为变量提供。
        .with("library_directory", layout.libraries_dir.as_str())
        .with("classpath_separator", classpath_separator().to_string())
        .with("user_properties", "{}")
        .with("resolution_width", width.to_string())
        .with("resolution_height", height.to_string())
}

impl LaunchLayout {
    /// 构造默认布局：游戏根目录、`assets` 与版本目录按官方 `.minecraft` 约定。
    #[must_use]
    pub fn standard(version_id: &str) -> Self {
        Self {
            game_dir: ".".to_owned(),
            version_dir: format!("versions/{version_id}"),
            assets_dir: "assets".to_owned(),
            natives_dir: format!("versions/{version_id}/natives"),
            libraries_dir: "libraries".to_owned(),
            classpath_extra: Vec::new(),
        }
    }
}

/// 解析 JVM 参数。旧版本没有结构化 JVM 参数，由启动器补齐。
fn resolve_jvm_arguments(
    meta: &VersionMeta,
    context: &LaunchContext,
    variables: &LaunchVariables,
) -> Result<Vec<String>> {
    if meta.arguments.is_some() {
        return variables.substitute_all(meta.jvm_arguments(context));
    }

    // 1.13 之前的版本依赖启动器提供 natives 路径与 classpath，缺一不可。
    let legacy = [
        "-Djava.library.path=${natives_directory}",
        "-cp",
        "${classpath}",
    ];
    variables.substitute_all(legacy)
}

/// 应用内存设置。
///
/// 用户未指定内存时采用官方的 `default-user-jvm`（26.3 起提供 `-Xms2G -Xmx4G`
/// 以及按平台选择的 GC 参数）；用户指定时跳过官方默认值，只用用户的 `-Xmx`，
/// 避免两份堆设置同时出现在命令行里造成困惑。
fn apply_memory_settings(
    meta: &VersionMeta,
    context: &LaunchContext,
    options: &LaunchOptions,
    variables: &LaunchVariables,
    args: &mut Vec<String>,
) -> Result<()> {
    if let Some(memory) = options.memory_mb {
        args.push(format!("-Xmx{memory}M"));
        return Ok(());
    }

    let Some(defaults) = meta
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.default_user_jvm.as_ref())
    else {
        return Ok(());
    };

    for argument in defaults {
        args.extend(
            variables.substitute_all(argument.resolve(context).iter().map(String::as_str))?,
        );
    }
    Ok(())
}
