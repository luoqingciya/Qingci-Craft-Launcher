// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 自研 processor 执行链：不跑官方安装器，自己把 `install_profile` 的步骤跑完。
//!
//! # 为什么不用官方安装器
//!
//! 安装器的 processor 是**独立 JVM**，各自联网，`--offline` 关不掉；
//! 其中 `DOWNLOAD_MOJMAPS` 把 `launchermeta.mojang.com` 写死在字节码里，
//! 而且先把输出删掉再下载——预置文件也没用。国内网络下这一步经常取不到。
//!
//! `install_profile.json` 已经把每一步的参数写清楚了，所以直接自己跑：
//!
//! | 步骤 | 谁来跑 |
//! | --- | --- |
//! | `MCP_DATA` / `MERGE_MAPPING` / `jarsplitter` / `AutoRenamingTool` / `binarypatcher` | 原样跑官方 processor（`java -cp … Main-Class …`） |
//! | `DOWNLOAD_MOJMAPS` | **换成本项目下载器**：地址取自本地 `versions/<mc>/<mc>.json` 的 `downloads.client_mappings`，可走镜像、可校验 sha1 |
//! | `EXTRACT_FILES` / `BUNDLER_EXTRACT` | 只服务端需要，客户端直接跳过（`sides` 里没有 `client`） |
//!
//! 换句话说：仍然用官方的 processor 实现（重写 jarsplitter / binarypatcher 没有意义），
//! 但**联网那一步换成我们自己的**，其余纯本地。这样既不依赖 Mojang 官方主机，
//! 也不需要理解 tsrg 合并、二进制补丁这些格式。
//!
//! # 与官方安装器的差异
//!
//! - 不写 `launcher_profiles.json`：那是给官方启动器看的，本项目不需要
//! - 不校验 processor 的 `outputs` 摘要：官方用它做「已存在就跳过」，我们每次都重跑，
//!   重跑是幂等的（工具都是整文件覆盖），少一层不一致的可能

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use qcl_net::HttpDownloader;
use serde::Deserialize;

use crate::forge::{ForgeError, proxy_args, tail_lines, user_agent};
use crate::forge_stage::{coordinate_path, read_install_profile, stage_prerequisites};

/// 客户端一侧的标识：`data` 的键名与 processor 的 `sides` 都用它。
const SIDE_CLIENT: &str = "client";

/// 需要联网的那一步，由本项目接管。
const TASK_DOWNLOAD_MOJMAPS: &str = "DOWNLOAD_MOJMAPS";

/// 安装器缓存目录（相对游戏目录），与 [`crate::forge`] 用的是同一个。
const INSTALLER_CACHE: &str = "installer-cache";

/// 一次自研安装的结果。
#[derive(Debug, Clone, Default)]
pub struct ProcessorReport {
    /// 依次执行的步骤（`--task` 名，没有 task 的用 processor jar 名）。
    pub steps: Vec<String>,
    /// 各步骤输出的尾部，出错时用来定位。
    pub log_tail: Vec<String>,
    /// 写出的版本 id，即 `versions/<id>/` 的目录名。
    pub version_id: String,
}

#[derive(Debug, Deserialize)]
struct InstallProfile {
    minecraft: String,
    version: String,
    /// 版本 JSON 在安装器 jar 里的路径（NeoForge 写 `/version.json`，Forge 省略）。
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    data: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    processors: Vec<Processor>,
}

#[derive(Debug, Deserialize)]
struct Processor {
    #[serde(default)]
    sides: Vec<String>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
}

impl Processor {
    /// `sides` 缺省表示两端都跑。
    fn runs_on_client(&self) -> bool {
        self.sides.is_empty() || self.sides.iter().any(|side| side == SIDE_CLIENT)
    }
}

/// 一个 `data` 值的落地形态。
#[derive(Debug, Clone, PartialEq, Eq)]
enum DataValue {
    /// `'...'` 形式的字面量（版本号、sha1 之类）。
    Literal(String),
    /// 本地文件：`/data/*`（从安装器 jar 解出）或 `[...]`（`libraries/` 下的构件）。
    File(PathBuf),
}

impl DataValue {
    fn as_arg(&self) -> String {
        match self {
            Self::Literal(value) => value.clone(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

/// `install_profile` 是否带 processors。
///
/// 1.13 以后才有这套结构；1.12.2 及更早的 profile 只有一个 `install` 段
/// （把内嵌 jar 复制成 `versions/<id>/<id>.jar`），那条路仍要交给官方安装器。
///
/// 这里只探 `processors` 一个字段：老 profile 连 `minecraft` 都没有，
/// 用完整的 [`InstallProfile`] 去解析会因缺字段直接失败，把「老版本」误报成「坏文件」。
pub fn has_processors(installer: &Path) -> Result<bool, ForgeError> {
    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        processors: Vec<serde_json::Value>,
    }

    let text = read_install_profile(installer)?;
    let probe: Probe = serde_json::from_str(&text)?;
    Ok(!probe.processors.is_empty())
}

/// 客户端安装：备料 → 跑 processor → 写出版本 JSON。
///
/// 调用方必须**先装好原版**（`versions/<mc>/<mc>.jar` 与同名 JSON），
/// 因为 processor 要切分、打补丁的就是那个 jar。
pub async fn install_client(
    downloader: &HttpDownloader,
    java: &Path,
    installer: &Path,
    game_dir: &Path,
    proxy: Option<&str>,
) -> Result<ProcessorReport, ForgeError> {
    // processor 会以游戏目录为 `current_dir` 启动，classpath 与参数里必须是绝对路径，
    // 否则相对路径会变成「游戏目录/游戏目录/…」。用 `dunce` 避免 Windows 的 `\\?\` 前缀。
    let installer = &dunce::canonicalize(installer)?;
    let game_dir = &dunce::canonicalize(game_dir)?;

    let text = read_install_profile(installer)?;
    let profile: InstallProfile = serde_json::from_str(&text)?;

    // 库先备齐：processor 的 classpath 与 `[...]` 输入都在 `libraries/` 下。
    stage_prerequisites(downloader, installer, game_dir).await?;

    let data = resolve_data(&profile, installer, game_dir)?;
    // processor 的输出目录不一定会自己建，先统一建好。
    for value in data.values() {
        if let DataValue::File(path) = value
            && let Some(parent) = path.parent()
        {
            std::fs::create_dir_all(parent)?;
        }
    }

    let minecraft_jar = game_dir
        .join("versions")
        .join(&profile.minecraft)
        .join(format!("{}.jar", profile.minecraft));
    if !minecraft_jar.is_file() {
        return Err(ForgeError::Installer(format!(
            "缺少原版 client jar（{}）。processor 要切分并给它打补丁，请先安装 Minecraft {}。",
            minecraft_jar.display(),
            profile.minecraft
        )));
    }

    let builtins = builtins(game_dir, installer, &profile.minecraft, &minecraft_jar);
    let mut report = ProcessorReport {
        version_id: profile.version.clone(),
        ..ProcessorReport::default()
    };

    for processor in profile
        .processors
        .iter()
        .filter(|item| item.runs_on_client())
    {
        let args = resolved_args(processor, &data, &builtins, game_dir)?;
        let task = task_name(&args).unwrap_or_else(|| processor.jar.clone());

        if task == TASK_DOWNLOAD_MOJMAPS {
            let output = flag_value(&args, "--output").ok_or_else(|| {
                ForgeError::Installer("DOWNLOAD_MOJMAPS 没有给出 --output".to_owned())
            })?;
            fetch_mojmaps(
                downloader,
                game_dir,
                &profile.minecraft,
                SIDE_CLIENT,
                Path::new(output),
            )
            .await?;
            report.steps.push(task);
            continue;
        }

        let classpath = classpath(&processor.jar, &processor.classpath, game_dir)?;
        let main = main_class(&classpath[0])?;
        let tail = run_java(java, &classpath, &main, &args, game_dir, proxy).await?;
        report.steps.push(task);
        report.log_tail.extend(tail);
    }

    write_version_json(installer, game_dir, &profile)?;
    Ok(report)
}

/// 固定参数：不是 `data` 里的键，但 processor 参数里会出现。
fn builtins(
    game_dir: &Path,
    installer: &Path,
    minecraft: &str,
    minecraft_jar: &Path,
) -> BTreeMap<&'static str, String> {
    BTreeMap::from([
        ("SIDE", SIDE_CLIENT.to_owned()),
        ("ROOT", game_dir.display().to_string()),
        ("INSTALLER", installer.display().to_string()),
        (
            "LIBRARY_DIR",
            game_dir.join("libraries").display().to_string(),
        ),
        ("MINECRAFT_JAR", minecraft_jar.display().to_string()),
        ("MINECRAFT_VERSION", minecraft.to_owned()),
    ])
}

/// 把 `data` 段解析成「键 → 本地值」。
///
/// `${KEY}` 形式的值引用另一个键，要等被引用者解析完才能定，所以分两轮。
fn resolve_data(
    profile: &InstallProfile,
    installer: &Path,
    game_dir: &Path,
) -> Result<BTreeMap<String, DataValue>, ForgeError> {
    let mut resolved: BTreeMap<String, DataValue> = BTreeMap::new();
    let mut deferred: Vec<(String, String)> = Vec::new();

    for (key, entry) in &profile.data {
        let Some(raw) = entry
            .get(SIDE_CLIENT)
            .or_else(|| entry.values().next())
            .map(|value| value.trim())
        else {
            continue;
        };
        if raw.starts_with("${") {
            deferred.push((key.clone(), raw.to_owned()));
        } else {
            resolved.insert(key.clone(), classify(raw, installer, game_dir)?);
        }
    }

    // 引用链一般只有一层，给 4 轮足够；跑不动说明是循环引用。
    for _ in 0..4 {
        if deferred.is_empty() {
            break;
        }
        let mut progressed = false;
        deferred.retain(|(key, raw)| {
            let target = raw.trim_start_matches("${").trim_end_matches('}');
            match resolved.get(target) {
                Some(value) => {
                    resolved.insert(key.clone(), value.clone());
                    progressed = true;
                    false
                }
                None => true,
            }
        });
        if !progressed {
            break;
        }
    }
    if let Some((key, raw)) = deferred.first() {
        return Err(ForgeError::Installer(format!(
            "data 里的 `{key}` 引用了无法解析的值：{raw}"
        )));
    }

    Ok(resolved)
}

/// `data` 值的三种形态。
fn classify(raw: &str, installer: &Path, game_dir: &Path) -> Result<DataValue, ForgeError> {
    if let Some(literal) = raw
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Ok(DataValue::Literal(literal.to_owned()));
    }
    if let Some(inner) = raw
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let path = coordinate_path(inner)?;
        return Ok(DataValue::File(game_dir.join("libraries").join(path)));
    }
    if raw.starts_with('/') {
        // 安装器 jar 内嵌的文件（`/data/client.lzma` 之类），解到缓存目录再用。
        return Ok(DataValue::File(extract_from_installer(
            installer, raw, game_dir,
        )?));
    }
    Err(ForgeError::Installer(format!(
        "无法识别的 data 值：`{raw}`"
    )))
}

/// 把安装器 jar 里的条目解到 `<game>/installer-cache/extract/`。
fn extract_from_installer(
    installer: &Path,
    entry_path: &str,
    game_dir: &Path,
) -> Result<PathBuf, ForgeError> {
    let relative = entry_path.trim_start_matches('/');
    let target = game_dir
        .join(INSTALLER_CACHE)
        .join("extract")
        .join(relative);
    if target.is_file() {
        return Ok(target);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut archive = open_zip(installer)?;
    let mut entry = archive.by_name(relative).map_err(|_| {
        ForgeError::Installer(format!("安装器里没有 `{relative}`，data 值无法落地"))
    })?;
    let mut file = std::fs::File::create(&target)?;
    std::io::copy(&mut entry, &mut file)?;
    Ok(target)
}

fn resolved_args(
    processor: &Processor,
    data: &BTreeMap<String, DataValue>,
    builtins: &BTreeMap<&'static str, String>,
    game_dir: &Path,
) -> Result<Vec<String>, ForgeError> {
    processor
        .args
        .iter()
        .map(|arg| resolve_arg(arg, data, builtins, game_dir))
        .collect()
}

/// 单个参数：`[坐标]` 先换成本地库路径，再展开 `{KEY}`。
///
/// 参数里两种写法都有，且都可能出现：
/// - `--output {MAPPINGS}`：`{…}` 是 `data` 的键
/// - `--input [de.oceanlabs.mcp:mcp_config:1.20.1-…@zip]`：`[…]` 是 Maven 坐标
///
/// 漏掉后者时 installertools 会报 `Input does not exist: [坐标]`——它只认本地路径。
fn resolve_arg(
    arg: &str,
    data: &BTreeMap<String, DataValue>,
    builtins: &BTreeMap<&'static str, String>,
    game_dir: &Path,
) -> Result<String, ForgeError> {
    let trimmed = arg.trim();
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        let path = game_dir.join("libraries").join(coordinate_path(inner)?);
        return Ok(path.display().to_string());
    }
    expand(arg, data, builtins)
}

/// 展开 `{KEY}` 占位符。`data` 里的键与固定参数同处一个命名空间。
fn expand(
    arg: &str,
    data: &BTreeMap<String, DataValue>,
    builtins: &BTreeMap<&'static str, String>,
) -> Result<String, ForgeError> {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(ForgeError::Installer(format!(
                "参数 `{arg}` 里的 `{{` 没有闭合"
            )));
        };
        let key = &after[..end];
        let value = builtins
            .get(key)
            .cloned()
            .or_else(|| data.get(key).map(DataValue::as_arg));
        let Some(value) = value else {
            return Err(ForgeError::Installer(format!(
                "processor 参数引用了未定义的值 `{{{key}}}`"
            )));
        };
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// `--task <名字>` 的名字；没有 `--task` 的步骤（jarsplitter 等）返回 `None`。
fn task_name(args: &[String]) -> Option<String> {
    flag_value(args, "--task").map(str::to_owned)
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// processor 的 classpath：自身 jar 在前，其余依赖按 profile 给出的顺序。
fn classpath(jar: &str, entries: &[String], game_dir: &Path) -> Result<Vec<PathBuf>, ForgeError> {
    let libraries = game_dir.join("libraries");
    let mut out: Vec<PathBuf> = Vec::with_capacity(entries.len() + 1);
    out.push(libraries.join(coordinate_path(jar)?));
    for entry in entries {
        let path = libraries.join(coordinate_path(entry)?);
        if !out.contains(&path) {
            out.push(path);
        }
    }
    Ok(out)
}

/// 从 jar 的 `META-INF/MANIFEST.MF` 里读 `Main-Class`。
fn main_class(jar: &Path) -> Result<String, ForgeError> {
    let mut archive = open_zip(jar)?;
    let mut entry = archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|_| ForgeError::Installer(format!("{} 没有 MANIFEST.MF", jar.display())))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Main-Class:") {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_owned());
            }
        }
    }
    Err(ForgeError::Installer(format!(
        "{} 的 MANIFEST 里没有 Main-Class",
        jar.display()
    )))
}

fn open_zip(jar: &Path) -> Result<zip::ZipArchive<std::fs::File>, ForgeError> {
    let file = std::fs::File::open(jar)?;
    zip::ZipArchive::new(file).map_err(|error| {
        ForgeError::Installer(format!("{} 不是有效的 jar：{error}", jar.display()))
    })
}

/// 起一个 processor JVM 并等它结束。
async fn run_java(
    java: &Path,
    classpath: &[PathBuf],
    main: &str,
    args: &[String],
    cwd: &Path,
    proxy: Option<&str>,
) -> Result<Vec<String>, ForgeError> {
    let mut command = tokio::process::Command::new(java);
    // 现在只有极少数 processor 会联网（Mojang mappings 那步已被我们接管），
    // 代理属性留着是为了上游改结构时不至于直接卡死。
    for argument in proxy_args(proxy) {
        command.arg(argument);
    }
    command
        .arg("-cp")
        .arg(join_paths(classpath))
        .arg(main)
        .args(args)
        .current_dir(cwd);

    let output = command.output().await.map_err(|error| {
        ForgeError::Installer(format!("无法启动 processor（{}）：{error}", java.display()))
    })?;
    let tail = tail_lines(&output);
    if !output.status.success() {
        return Err(ForgeError::Installer(format!(
            "processor `{main}` 执行失败（退出码 {}）：\n{}",
            output
                .status
                .code()
                .map_or("-".to_owned(), |code| code.to_string()),
            tail.join("\n")
        )));
    }
    Ok(tail)
}

/// classpath 分隔符随平台变：Windows 是 `;`，其余是 `:`。
fn join_paths(paths: &[PathBuf]) -> std::ffi::OsString {
    let mut joined = std::ffi::OsString::new();
    for (index, path) in paths.iter().enumerate() {
        if index > 0 {
            joined.push(if cfg!(windows) { ";" } else { ":" });
        }
        joined.push(path);
    }
    joined
}

#[derive(Debug, Deserialize)]
struct VanillaVersion {
    #[serde(default)]
    downloads: BTreeMap<String, VanillaDownload>,
}

#[derive(Debug, Deserialize)]
struct VanillaDownload {
    url: String,
    #[serde(default)]
    sha1: Option<String>,
}

/// 取代 `DOWNLOAD_MOJMAPS`：从本地原版版本 JSON 取地址，走下载器（含镜像）拉取。
///
/// 官方 processor 把 `launchermeta.mojang.com` 写死，且先删输出再下载；
/// 换成这条路之后，这一步与其它资产下载一样走镜像、能校验 sha1。
async fn fetch_mojmaps(
    downloader: &HttpDownloader,
    game_dir: &Path,
    minecraft: &str,
    side: &str,
    output: &Path,
) -> Result<(), ForgeError> {
    let version_json = game_dir
        .join("versions")
        .join(minecraft)
        .join(format!("{minecraft}.json"));
    let raw = std::fs::read_to_string(&version_json).map_err(|error| {
        ForgeError::Installer(format!(
            "读不到原版版本 JSON（{}）：{error}",
            version_json.display()
        ))
    })?;
    let parsed: VanillaVersion = serde_json::from_str(&raw)?;

    let key = format!("{side}_mappings");
    let download = parsed.downloads.get(&key).ok_or_else(|| {
        ForgeError::Installer(format!("原版版本 JSON 里没有 `{key}`，无法生成 mappings"))
    })?;

    let bytes = downloader
        .fetch_bytes_with_headers(&download.url, &user_agent(), &[])
        .await?;
    if let Some(expected) = &download.sha1
        && !qcl_net::verify_sha1(expected, &bytes)
    {
        return Err(ForgeError::Installer(format!(
            "`{key}` 的 sha1 不匹配（期望 {expected}）"
        )));
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &bytes)?;
    Ok(())
}

/// 把安装器 jar 里的版本 JSON 写到 `versions/<id>/<id>.json`。
fn write_version_json(
    installer: &Path,
    game_dir: &Path,
    profile: &InstallProfile,
) -> Result<PathBuf, ForgeError> {
    let entry_name = profile
        .json
        .as_deref()
        .unwrap_or("/version.json")
        .trim_start_matches('/');

    let mut archive = open_zip(installer)?;
    let mut entry = archive.by_name(entry_name).map_err(|_| {
        ForgeError::Installer(format!("安装器里没有 `{entry_name}`，无法写出版本 JSON"))
    })?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    // 先确认是合法 JSON：写出去一个坏文件要到启动时才报错。
    serde_json::from_str::<serde_json::Value>(&text)?;

    let dir = game_dir.join("versions").join(&profile.version);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", profile.version));
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> BTreeMap<&'static str, String> {
        BTreeMap::from([("SIDE", "client".to_owned()), ("ROOT", "/game".to_owned())])
    }

    fn data() -> BTreeMap<String, DataValue> {
        BTreeMap::from([
            (
                "MAPPINGS".to_owned(),
                DataValue::File(PathBuf::from("/game/libraries/mappings.txt")),
            ),
            (
                "MCP_VERSION".to_owned(),
                DataValue::Literal("2023".to_owned()),
            ),
        ])
    }

    #[test]
    fn expand_substitutes_builtins_and_data() {
        assert_eq!(
            expand("{ROOT}/libraries", &data(), &builtins()).unwrap(),
            "/game/libraries"
        );
        assert_eq!(
            expand("{MAPPINGS}", &data(), &builtins()).unwrap(),
            "/game/libraries/mappings.txt"
        );
        assert_eq!(expand("{SIDE}", &data(), &builtins()).unwrap(), "client");
        assert_eq!(
            expand("{MCP_VERSION}", &data(), &builtins()).unwrap(),
            "2023"
        );
    }

    #[test]
    fn expand_leaves_plain_args_untouched() {
        assert_eq!(expand("--task", &data(), &builtins()).unwrap(), "--task");
        // jarsplitter 的 `--srg` 后面跟的是占位符，但值本身也可能带花括号以外的字符
        assert_eq!(
            expand("EXTRACT_FILES", &data(), &builtins()).unwrap(),
            "EXTRACT_FILES"
        );
    }

    #[test]
    fn expand_reports_unknown_placeholder() {
        assert!(expand("{NOPE}", &data(), &builtins()).is_err());
        assert!(expand("{unclosed", &data(), &builtins()).is_err());
    }

    #[test]
    fn classify_understands_the_three_shapes() {
        let game = Path::new("/game");
        let installer = Path::new("/tmp/installer.jar");

        assert_eq!(
            classify("'1.20.1'", installer, game).unwrap(),
            DataValue::Literal("1.20.1".to_owned())
        );
        // `[...]` 落到 libraries 下，扩展名由 @ 后缀决定
        assert_eq!(
            classify(
                "[de.oceanlabs.mcp:mcp_config:1.0:mappings@txt]",
                installer,
                game
            )
            .unwrap(),
            DataValue::File(PathBuf::from(
                "/game/libraries/de/oceanlabs/mcp/mcp_config/1.0/mcp_config-1.0-mappings.txt"
            ))
        );
        // 其它形态（`/data/...` 需要真 jar、裸字符串）不能静默放过
        assert!(classify("随便什么", installer, game).is_err());
    }

    #[test]
    fn bracketed_args_become_local_library_paths() {
        // 回归：只展开 `{KEY}` 时 installertools 报 `Input does not exist: [坐标]`
        let arg = resolve_arg(
            "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]",
            &data(),
            &builtins(),
            Path::new("/game"),
        )
        .unwrap();
        assert_eq!(
            Path::new(&arg),
            Path::new("/game").join("libraries").join(
                "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
            )
        );
    }

    #[test]
    fn processors_are_filtered_by_side() {
        let client = Processor {
            sides: vec!["client".to_owned()],
            jar: "a:b:1".to_owned(),
            classpath: Vec::new(),
            args: Vec::new(),
        };
        let server = Processor {
            sides: vec!["server".to_owned()],
            jar: "a:b:1".to_owned(),
            classpath: Vec::new(),
            args: Vec::new(),
        };
        let both = Processor {
            sides: Vec::new(),
            jar: "a:b:1".to_owned(),
            classpath: Vec::new(),
            args: Vec::new(),
        };
        assert!(client.runs_on_client());
        assert!(!server.runs_on_client());
        assert!(both.runs_on_client());
    }

    #[test]
    fn classpath_starts_with_the_processor_jar_and_dedupes() {
        let game = Path::new("/game");
        let entries = vec![
            "net.minecraftforge:installertools:1.3.0".to_owned(),
            "net.md-5:SpecialSource:1.11.0".to_owned(),
        ];
        let list = classpath("net.minecraftforge:installertools:1.3.0", &entries, game).unwrap();
        assert_eq!(list.len(), 2, "自身 jar 只出现一次：{list:?}");
        assert_eq!(
            list[0],
            PathBuf::from(
                "/game/libraries/net/minecraftforge/installertools/1.3.0/installertools-1.3.0.jar"
            )
        );
    }

    #[test]
    fn task_name_reads_the_flag() {
        let args = vec![
            "--task".to_owned(),
            "MCP_DATA".to_owned(),
            "--input".to_owned(),
            "x".to_owned(),
        ];
        assert_eq!(task_name(&args).as_deref(), Some("MCP_DATA"));
        assert_eq!(flag_value(&args, "--input"), Some("x"));
        assert_eq!(flag_value(&args, "--output"), None);
        assert_eq!(task_name(&["--input".to_owned()]), None);
    }

    #[test]
    fn join_paths_uses_the_platform_separator() {
        let paths = vec![PathBuf::from("a.jar"), PathBuf::from("b.jar")];
        let joined = join_paths(&paths);
        let expected = if cfg!(windows) {
            "a.jar;b.jar"
        } else {
            "a.jar:b.jar"
        };
        assert_eq!(joined.to_string_lossy(), expected);
    }

    #[test]
    fn has_processors_handles_legacy_profiles() {
        use std::io::Write;

        fn write_jar(dir: &Path, name: &str, profile: &str) -> PathBuf {
            let path = dir.join(name);
            let file = std::fs::File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file(
                    "install_profile.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(profile.as_bytes()).unwrap();
            writer.finish().unwrap();
            path
        }

        let dir = tempfile::tempdir().unwrap();

        let modern = write_jar(
            dir.path(),
            "modern.jar",
            r#"{"minecraft":"1.20.1","version":"1.20.1-forge-47.2.0",
                "processors":[{"jar":"a:b:1","args":[]}]}"#,
        );
        assert!(has_processors(&modern).unwrap());

        // 1.12.2 那代 profile 没有 minecraft / version，只有 install 段：
        // 必须判成「没有 processors」而不是解析失败，否则老版本会被误判成坏文件。
        let legacy = write_jar(
            dir.path(),
            "legacy.jar",
            r#"{"install":{"path":"x"},"versionInfo":{"id":"1.12.2-forge-1"}}"#,
        );
        assert!(!has_processors(&legacy).unwrap());
    }

    #[test]
    fn resolve_data_follows_reference_chains() {
        let profile: InstallProfile = serde_json::from_str(
            r#"{
                "minecraft": "1.20.1",
                "version": "1.20.1-forge-47.2.0",
                "data": {
                    "MCP_VERSION": { "client": "'20230612.114412'" },
                    "PATCHED": { "client": "[net.minecraftforge:forge:1.20.1-47.2.0:client]" },
                    "ALIAS": { "client": "${PATCHED}" },
                    "BROKEN": { "client": "${NOT_THERE}" }
                }
            }"#,
        )
        .unwrap();

        // 引用链本身没问题时不该报错——但这里故意放了 BROKEN，所以应当报错
        let error = resolve_data(
            &profile,
            Path::new("/tmp/installer.jar"),
            Path::new("/game"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("BROKEN"), "{error}");
    }

    #[test]
    fn resolve_data_expands_aliases() {
        let profile: InstallProfile = serde_json::from_str(
            r#"{
                "minecraft": "1.20.1",
                "version": "1.20.1-forge-47.2.0",
                "data": {
                    "PATCHED": { "client": "[net.minecraftforge:forge:1.20.1-47.2.0:client]" },
                    "ALIAS": { "client": "${PATCHED}" }
                }
            }"#,
        )
        .unwrap();

        let data = resolve_data(
            &profile,
            Path::new("/tmp/installer.jar"),
            Path::new("/game"),
        )
        .unwrap();
        assert_eq!(data.get("ALIAS"), data.get("PATCHED"));
        // 按路径比较，不比字符串：Windows 上 `join` 会产生混合分隔符
        let arg = data.get("ALIAS").unwrap().as_arg();
        assert_eq!(
            Path::new(&arg),
            Path::new("/game")
                .join("libraries")
                .join("net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-client.jar")
        );
    }
}
