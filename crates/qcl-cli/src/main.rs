//! `qcl`：Qingci Craft Launcher 的无界面入口。
//!
//! 存在的意义是把散落在各 crate 里的能力串成一条可执行的链路，作为端到端冒烟手段：
//!
//! ```text
//! 拉版本清单 → 拉 version.json → 安装全部文件 → 选 Java → 组装命令 → 启动并转发日志
//! ```
//!
//! 它不追求功能完整（没有账号、没有模组管理、没有界面），只保证「这条链路能跑通」。

mod args;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;

use qcl_core::launch::{
    AccountInfo, LaunchCommand, LaunchLayout, LaunchOptions, build_launch_command,
};
use qcl_core::version::{LaunchContext, VersionEntry, VersionManifest, VersionMeta};
use qcl_install::{InstallPhase, InstallProgress, VersionInstallOptions, install_version};
use qcl_java::{detect, select};
use qcl_net::{HttpDownloader, verify_sha1};
use qcl_runtime::{LogStream, spawn};

use crate::args::{Args, ArgsError};

/// 版本清单地址。
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[tokio::main]
async fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();

    let args = match args::parse(raw) {
        Ok(args) => args,
        Err(ArgsError::Help) => {
            print!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    match run(args).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("错误：{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let game_dir = absolute(&args.game_dir)?;
    std::fs::create_dir_all(&game_dir)?;

    let downloader = Arc::new(HttpDownloader::new(&game_dir)?);

    // 1. 版本清单与版本元数据。
    let entry = resolve_version(&downloader, &args.version).await?;
    println!("版本：{}（{}）", entry.id, entry.release_time);
    let meta = fetch_meta(&downloader, &entry).await?;

    // 2. 安装。幂等，因此每次启动前调用都能起到修复作用。
    //
    // `LaunchContext::current()` 不带系统版本号（取真实构建号需要平台专用 API）。
    // 已知后果：26.3 用 `os.versionRange` 区分 Windows 版本以选择 GC，缺版本号时
    // 两条分支都不命中，命令行里不会出现 `-XX:+UseZGC`——游戏照常启动（JVM 用默认 GC），
    // 但与官方启动器不完全一致。补上系统版本探测是后续独立改动。
    println!("检查文件…");
    let options = VersionInstallOptions::for_version(&meta.id);
    let report = install_version(
        Arc::clone(&downloader),
        &meta,
        &LaunchContext::current(),
        &options,
        progress_printer(),
    )
    .await?;

    println!(
        "文件就绪：版本文件 {}/{}（跳过 {}），natives {} 个，资产 {}/{}",
        report.downloaded + report.skipped,
        report.planned_files,
        report.skipped,
        report.natives.extracted,
        report.assets.downloaded + report.assets.skipped,
        report.assets.objects
    );

    // 3. 选 Java。缺失时明确失败，而不是用低版本硬跑（那会抛 UnsupportedClassVersionError）。
    let required = meta.required_java_major().unwrap_or(8);
    let installations = detect(&[], Some(&game_dir.join("runtime")));
    let Some(java) = select(&installations, required) else {
        return Err(format!(
            "需要 Java {required}，但本机未找到。已探测到：{}",
            if installations.is_empty() {
                "（无）".to_owned()
            } else {
                installations
                    .iter()
                    .map(|java| format!("{}", java.version))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        )
        .into());
    };
    println!("Java：{}（{}）", java.version, java.home.display());

    // 4. 组装命令。
    let account = offline_account(&args.player_name);
    let mut layout = LaunchLayout::standard(&meta.id);
    // `${game_directory}` 传绝对路径：进程工作目录虽然是游戏目录，但游戏内部还会
    // 用它拼存档与日志路径，绝对路径可以避免二次拼接时的歧义。
    layout.game_dir = game_dir.to_string_lossy().into_owned();
    let launch_options = LaunchOptions {
        memory_mb: args.memory_mb,
        ..LaunchOptions::default()
    };

    let command = build_launch_command(
        &java.executable,
        &meta,
        &LaunchContext::current(),
        &layout,
        &account,
        &launch_options,
    )?;

    println!("\n启动命令：\n{}", render_command(&command));

    if args.dry_run {
        println!("\n--dry-run：不启动游戏。");
        return Ok(ExitCode::SUCCESS);
    }

    // 5. 启动并转发日志。
    println!("\n启动游戏…\n");
    let mut process = spawn(&command, &game_dir)?;
    let outcome = process
        .wait(|line| {
            // stderr 原样输出到 stderr，便于 `qcl ... 2>err.log` 分离。
            match line.stream {
                LogStream::Stdout => println!("{}", line.text),
                LogStream::Stderr => eprintln!("{}", line.text),
            }
        })
        .await?;

    println!(
        "\n游戏退出：{:?}，耗时 {:.1} 秒",
        outcome.exit_code,
        outcome.duration.as_secs_f64()
    );

    Ok(match outcome.exit_code {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

/// 解析版本号：`latest` 取最新正式版，否则按 id 精确匹配。
async fn resolve_version(
    downloader: &HttpDownloader,
    requested: &str,
) -> Result<VersionEntry, Box<dyn std::error::Error>> {
    let raw = downloader.fetch_text(VERSION_MANIFEST_URL).await?;
    let manifest: VersionManifest = serde_json::from_str(&raw)?;

    let id = if requested == "latest" {
        manifest.latest.release.clone()
    } else {
        requested.to_owned()
    };

    manifest
        .find(&id)
        .cloned()
        .ok_or_else(|| format!("版本清单中不存在 `{id}`").into())
}

/// 拉取并校验版本元数据。
///
/// 校验 SHA-1 的成本几乎为零，但它挡住了代理注入与截断——版本元数据决定了
/// 后面要下载哪些文件、用什么参数启动，被篡改的后果比单个文件损坏严重得多。
async fn fetch_meta(
    downloader: &HttpDownloader,
    entry: &VersionEntry,
) -> Result<VersionMeta, Box<dyn std::error::Error>> {
    let raw = downloader.fetch_text(&entry.url).await?;
    if let Some(expected) = entry.sha1.as_deref()
        && !verify_sha1(expected, raw.as_bytes())
    {
        return Err(format!("版本元数据校验失败：{}", entry.url).into());
    }
    Ok(serde_json::from_str(&raw)?)
}

/// 离线账户。启动器尚未接入微软登录，这里用占位凭据。
fn offline_account(player_name: &str) -> AccountInfo {
    AccountInfo {
        player_name: player_name.to_owned(),
        // 全零 UUID 是离线模式的常见约定；正版服务器会拒绝，单机与离线服可用。
        uuid: "00000000000000000000000000000000".to_owned(),
        access_token: "0".to_owned(),
        user_type: "legacy".to_owned(),
        client_id: None,
        xuid: None,
    }
}

/// 把相对路径转成绝对路径。
///
/// 游戏进程的工作目录就是游戏目录，而 `${game_directory}` 传的是绝对路径——
/// 与官方启动器一致，避免游戏内部再拼接时出现相对路径歧义。
fn absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

/// 渲染成可直接粘贴到终端的字符串（仅用于展示）。
fn render_command(command: &LaunchCommand) -> String {
    let mut rendered = quote(&command.executable.to_string_lossy());
    for argument in &command.args {
        rendered.push(' ');
        rendered.push_str(&quote(argument));
    }
    rendered
}

fn quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_owned();
    }
    if value.contains(' ') || value.contains('\t') || value.contains('"') {
        return format!("\"{}\"", value.replace('"', "\\\""));
    }
    value.to_owned()
}

/// 进度打印：每跨过 10% 打一行，阶段切换时重置。
///
/// 不打 `\r` 覆盖行：CLI 常被重定向到文件或在 CI 里跑，回车符只会让日志变成一团。
fn progress_printer() -> impl Fn(InstallProgress) + Send + Sync + 'static {
    let state = Arc::new(Mutex::new((None::<InstallPhase>, u32::MAX)));

    move |InstallProgress { phase, progress }| {
        // 百分比按**文件数**算，而不是 `Progress::ratio()` 的字节占比：
        // 屏幕上同时显示 `已完成/总数`，两者口径不一致会让「20% 才 166/5147」
        // 看起来像算错了。字节占比留给图形界面的进度条。
        let percent = (progress.completed_files * 100)
            .checked_div(progress.total_files)
            .unwrap_or(100) as u32;
        let bucket = percent / 10;

        let mut state = state.lock().expect("锁未中毒");
        if state.0 != Some(phase) {
            *state = (Some(phase), u32::MAX);
        }
        if state.1 == bucket {
            return;
        }
        state.1 = bucket;

        let label = match phase {
            InstallPhase::VersionFiles => "版本文件",
            InstallPhase::Assets => "资产",
        };
        println!(
            "  [{label}] {percent}%  {}/{}",
            progress.completed_files, progress.total_files
        );
    }
}
