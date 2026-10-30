// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! Forge / NeoForge：版本清单与安装器编排。
//!
//! 与 Fabric 的根本差别：**Fabric 只要合并一个 JSON，Forge 必须真的跑一次安装器**。
//! 安装器自己会下 vanilla jar、下 libraries、再跑 processors（把 mappings 与补丁
//! 应用到 jar 上）——最后一步是自研实现里最贵的部分，所以这里直接调用官方安装器。
//!
//! 安装器的契约（用真实 installer 实测过）：
//!
//! ```text
//! java -jar forge-installer.jar --installClient <目录>
//! ```
//!
//! 三个必须处理的坑：
//!
//! 1. **目标目录必须有 `launcher_profiles.json`**，否则安装器报「你还没有运行过启动器」。
//! 2. **MC 本体永远从 `launchermeta.mojang.com` 取**，`--mirror` 管不到（地址硬编码）。
//! 3. **安装前会探测若干 Mojang 主机并校验证书**，任一失败即中止。
//!
//! 第 2、3 条是同一个根因：安装器绕不开 Mojang 官方主机。因此网络受限时安装会失败，
//! 这不是本模块能绕过的——只能把错误说清楚，让用户知道卡在哪一步。

use std::path::{Path, PathBuf};

use qcl_net::{DownloadError, HttpDownloader};
use serde::Deserialize;

use crate::loader::LoaderInfo;

/// Forge Maven 根地址。
pub const FORGE_MAVEN_BASE: &str = "https://maven.minecraftforge.net/";

/// NeoForge Maven 根地址。
pub const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases/";

/// BMCLAPI 的 Forge 版本接口（比官方 maven metadata 小得多，也快得多）。
///
/// 官方 `maven-metadata.xml` 有 213 KB，本机实测约 500 B/s；这个接口只有 56 KB
/// 且是 JSON。镜像不可用时才回落到官方 XML。
pub const BMCLAPI_FORGE_API: &str = "https://bmclapi2.bangbang93.com/forge/minecraft";

/// BMCLAPI 的 NeoForge 构建列表。实测 26.2 返回 92 个构建（含 beta）。
pub const BMCLAPI_NEOFORGE_API: &str = "https://bmclapi2.bangbang93.com/neoforge/list";

/// 安装器缓存目录（相对游戏目录）。装完不必保留，但重装时能省一次下载。
const INSTALLER_DIR: &str = "installer-cache";

/// 支持的加载器种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeKind {
    /// Minecraft Forge。
    Forge,
    /// NeoForge（1.20.2 起独立于 Forge 发布）。
    NeoForge,
}

impl ForgeKind {
    /// 存进设置里的标识。
    pub fn id(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    /// 界面上的显示名。
    pub fn display(self) -> &'static str {
        match self {
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }

    /// 从设置里的字符串解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "forge" => Some(Self::Forge),
            "neoforge" => Some(Self::NeoForge),
            _ => None,
        }
    }
}

/// Forge / NeoForge 相关错误。
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    /// 网络失败。
    #[error("获取失败：{0}")]
    Download(#[from] DownloadError),

    /// JSON 解析失败。
    #[error("解析失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// 该 MC 版本不受支持，或没有可用构建。
    #[error("{0}")]
    Unsupported(String),

    /// 安装器进程失败。
    #[error("{0}")]
    Installer(String),

    /// 文件操作失败。
    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),
}

/// BMCLAPI `/forge/minecraft/<mc>` 的单个条目。只取版本号。
#[derive(Debug, Deserialize)]
struct BmclapiBuild {
    version: String,
}

/// 安装结果。
#[derive(Debug, Clone)]
pub struct ForgeInstallReport {
    /// 安装后的版本 id（`versions/<id>/<id>.json` 的目录名）。
    pub version_id: String,
    /// 安装器输出的最后若干行，失败时用来定位。
    pub log_tail: Vec<String>,
}

/// 拉取某个 MC 版本可用的加载器版本，新版本在前。
pub async fn fetch_versions(
    downloader: &HttpDownloader,
    kind: ForgeKind,
    game_version: &str,
) -> Result<Vec<LoaderInfo>, ForgeError> {
    match kind {
        ForgeKind::Forge => fetch_forge_versions(downloader, game_version).await,
        ForgeKind::NeoForge => fetch_neoforge_versions(downloader, game_version).await,
    }
}

async fn fetch_forge_versions(
    downloader: &HttpDownloader,
    game_version: &str,
) -> Result<Vec<LoaderInfo>, ForgeError> {
    // 先试镜像的专用接口：体积小、无需解析 XML。
    let url = format!("{BMCLAPI_FORGE_API}/{game_version}");
    if let Ok(raw) = downloader.fetch_text(&url).await
        && let Ok(list) = parse_bmclapi_builds(&raw)
        && !list.is_empty()
    {
        return Ok(list);
    }

    // 回落官方 maven metadata。慢，但权威且不依赖镜像。
    let url = format!("{FORGE_MAVEN_BASE}net/minecraftforge/forge/maven-metadata.xml");
    let raw = downloader.fetch_text(&url).await?;
    let list = parse_forge_metadata(&raw, game_version);
    if list.is_empty() {
        return Err(ForgeError::Unsupported(format!(
            "Forge 没有适用于 {game_version} 的构建"
        )));
    }
    Ok(list)
}

async fn fetch_neoforge_versions(
    downloader: &HttpDownloader,
    game_version: &str,
) -> Result<Vec<LoaderInfo>, ForgeError> {
    let prefix = neoforge_prefix(game_version).ok_or_else(|| {
        ForgeError::Unsupported(format!(
            "NeoForge 从 Minecraft 1.20.2 起才独立发布，{game_version} 不受支持"
        ))
    })?;

    // 先试镜像的专用接口：`maven.neoforged.net` 在部分网络下连不通（实测直连超时），
    // 而 BMCLAPI 这个接口可用（26.2 返回 92 个构建）。与 Forge 那边同样的取舍：
    // 体积小、是 JSON、不依赖 maven。
    let url = format!("{BMCLAPI_NEOFORGE_API}/{game_version}");
    if let Ok(raw) = downloader.fetch_text(&url).await
        && let Ok(list) = parse_bmclapi_builds(&raw)
    {
        // 接口已按 MC 版本过滤，这里再按前缀筛一次：防止镜像把别的版本混进来。
        let filtered: Vec<LoaderInfo> = list
            .into_iter()
            .filter(|loader| loader.version.starts_with(&prefix))
            .collect();
        if !filtered.is_empty() {
            return Ok(filtered);
        }
    }

    let url = format!("{NEOFORGE_MAVEN_BASE}net/neoforged/neoforge/maven-metadata.xml");
    let raw = downloader.fetch_text(&url).await?;
    let list = parse_neoforge_metadata(&raw, &prefix);
    if list.is_empty() {
        return Err(ForgeError::Unsupported(format!(
            "NeoForge 没有适用于 {game_version} 的构建"
        )));
    }
    Ok(list)
}

/// 解析 BMCLAPI 的 Forge 构建列表。
///
/// 接口没有稳定/测试的区分，所以 `stable` 一律给 `false`——界面据此显示「官方构建」，
/// 不去编造一个「稳定版」标签。
pub fn parse_bmclapi_builds(raw: &str) -> Result<Vec<LoaderInfo>, serde_json::Error> {
    let builds: Vec<BmclapiBuild> = serde_json::from_str(raw)?;
    let mut versions: Vec<LoaderInfo> = builds
        .into_iter()
        .map(|build| build.version)
        .filter(|version| is_numeric_version(version))
        .map(|version| LoaderInfo {
            version,
            stable: false,
        })
        .collect();
    sort_desc(&mut versions);
    versions.dedup_by(|a, b| a.version == b.version);
    Ok(versions)
}

/// 从 Forge 的 `maven-metadata.xml` 里挑出某个 MC 版本的构建。
///
/// 条目形如 `<version>1.20.1-47.2.0</version>`，前缀是 MC 版本。
pub fn parse_forge_metadata(raw: &str, game_version: &str) -> Vec<LoaderInfo> {
    let prefix = format!("{game_version}-");
    let mut versions: Vec<LoaderInfo> = versions_in_metadata(raw)
        .into_iter()
        .filter_map(|entry| entry.strip_prefix(&prefix).map(str::to_owned))
        .filter(|version| is_numeric_version(version))
        .map(|version| LoaderInfo {
            version,
            stable: false,
        })
        .collect();
    sort_desc(&mut versions);
    versions.dedup_by(|a, b| a.version == b.version);
    versions
}

/// 从 NeoForge 的 `maven-metadata.xml` 里按前缀挑构建。
///
/// NeoForge 的版本号是 `<MC 去掉前导 1>.<构建号>`，前缀由 [`neoforge_prefix`] 给出。
pub fn parse_neoforge_metadata(raw: &str, prefix: &str) -> Vec<LoaderInfo> {
    let mut versions: Vec<LoaderInfo> = versions_in_metadata(raw)
        .into_iter()
        .filter(|entry| entry.starts_with(prefix))
        .filter(|entry| is_numeric_version(entry))
        .map(|version| LoaderInfo {
            version,
            stable: false,
        })
        .collect();
    sort_desc(&mut versions);
    versions.dedup_by(|a, b| a.version == b.version);
    versions
}

/// MC 版本 → NeoForge 版本前缀。
///
/// | Minecraft | NeoForge |
/// |---|---|
/// | `1.21` | `21.0.` |
/// | `1.21.1` | `21.1.` |
/// | `26.3` | `26.3.` |
///
/// 规则：去掉前导的 `1.`；剩下的只有一段时补一个 `.0`。1.20.1 及更早没有独立的
/// NeoForge 制品（那时它还叫 `net.neoforged:forge`），返回 `None`。
pub fn neoforge_prefix(game_version: &str) -> Option<String> {
    let parts: Vec<&str> = game_version.split('.').collect();
    let stripped: Vec<&str> = match parts.as_slice() {
        ["1", minor, patch] if is_minor_ge_20(minor, patch) => vec![minor, patch],
        ["1", minor] if is_minor_ge_20(minor, "0") => vec![minor, "0"],
        [major, minor] if !major.is_empty() && !minor.is_empty() => vec![major, minor],
        _ => return None,
    };
    if stripped.iter().any(|part| part.parse::<u32>().is_err()) {
        return None;
    }
    Some(format!("{}.", stripped.join(".")))
}

/// 1.20.2 及以上才有独立的 NeoForge。
fn is_minor_ge_20(minor: &str, patch: &str) -> bool {
    let Ok(minor) = minor.parse::<u32>() else {
        return false;
    };
    let Ok(patch) = patch.parse::<u32>() else {
        return false;
    };
    minor > 20 || (minor == 20 && patch >= 2)
}

/// 安装器 jar 的下载地址。
pub fn installer_url(kind: ForgeKind, game_version: &str, loader_version: &str) -> String {
    match kind {
        ForgeKind::Forge => format!(
            "{FORGE_MAVEN_BASE}net/minecraftforge/forge/{game_version}-{loader_version}/\
             forge-{game_version}-{loader_version}-installer.jar"
        ),
        ForgeKind::NeoForge => format!(
            "{NEOFORGE_MAVEN_BASE}net/neoforged/neoforge/{loader_version}/\
             neoforge-{loader_version}-installer.jar"
        ),
    }
}

/// 安装完成后版本目录的名字。
pub fn version_id(kind: ForgeKind, game_version: &str, loader_version: &str) -> String {
    match kind {
        ForgeKind::Forge => format!("{game_version}-forge-{loader_version}"),
        ForgeKind::NeoForge => format!("neoforge-{loader_version}"),
    }
}

/// 安装过程中可选的「钩子」。
///
/// 两样都不是必需信息，但都得从应用层一路传进来；合成一个结构比再加两个参数清楚。
pub struct InstallHooks<'a> {
    /// 官方安装器 JVM 的代理（`host:port`）。自研 processor 链用不到。
    pub proxy: Option<&'a str>,
    /// 步骤进度回调（自研链按 processor 步骤数报，老版本只报一次）。
    pub on_step: crate::forge_processor::StepCallback<'a>,
}

/// 隐藏子进程的控制台窗口（仅 Windows 有效）。
///
/// 安装器与 processor 跑的都是 `java.exe` 而不是 `javaw.exe`，不隐藏就会弹出一个黑框，
/// 看起来像启动器在偷偷跑命令行脚本。游戏进程那边（`qcl-runtime`）是同样的处理。
#[cfg(windows)]
pub(crate) fn hide_console(command: &mut tokio::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// 非 Windows 平台不需要处理。
#[cfg(not(windows))]
pub(crate) fn hide_console(_command: &mut tokio::process::Command) {}

/// 安装 Forge / NeoForge 到游戏目录。
///
/// 两条路径，按 install_profile 的形态自动选：
///
/// - **有 processors**（1.13 以后）：走 [`crate::forge_processor::install_client`]，
///   备料 + 自研执行链 + 写版本 JSON。除 Mojang mappings 一步（改走镜像）外纯本地，
///   不需要官方安装器，也不依赖 `launchermeta.mojang.com`。
/// - **没有 processors**（1.12.2 及更早）：那套 profile 只有一个 `install` 段，
///   由安装器把内嵌的 universal jar 复制成 `versions/<id>/<id>.jar`，交给官方安装器。
///
/// 调用方必须**先装好原版**：processor 要切分并打补丁的就是原版 client jar。
pub async fn install(
    downloader: &HttpDownloader,
    kind: ForgeKind,
    java: &Path,
    game_version: &str,
    loader_version: &str,
    game_dir: &Path,
    hooks: InstallHooks<'_>,
) -> Result<ForgeInstallReport, ForgeError> {
    let InstallHooks { proxy, on_step } = hooks;
    let jar = download_installer(downloader, kind, game_version, loader_version, game_dir).await?;

    // 参数里要放绝对路径：安装器与 processor 都会切到 `current_dir` 再解析相对路径。
    // 用 `dunce` 而不是 `std::fs::canonicalize`：后者在 Windows 上会带上
    // `\\?\` 前缀，进了日志与 processor 参数就没法看了。
    let jar = dunce::canonicalize(&jar)?;
    let game_dir = dunce::canonicalize(game_dir)?;

    let expected = version_id(kind, game_version, loader_version);

    if crate::forge_processor::has_processors(&jar)? {
        let report = crate::forge_processor::install_client(
            downloader, java, &jar, &game_dir, proxy, on_step,
        )
        .await?;
        // 版本 id 是应用层定位 `versions/<id>/` 的依据，与约定不一致必须当场失败，
        // 否则会写出一个永远读不到的实例目录。
        if report.version_id != expected {
            return Err(ForgeError::Installer(format!(
                "上游版本 id 是 {}，与约定的 {expected} 不一致",
                report.version_id
            )));
        }
        return Ok(ForgeInstallReport {
            version_id: expected,
            log_tail: report.log_tail,
        });
    }

    // 老版本：没有 processors，只有内嵌 jar 复制，直接跑官方安装器。
    // 这一趟没有中间步骤可报，只给一个「进行中」，免得界面一直停在零。
    on_step(1, 1);
    ensure_launcher_profiles(&game_dir)?;

    let output = {
        let mut command = tokio::process::Command::new(java);
        hide_console(&mut command);
        // 安装器自己不走我们的下载器与镜像，要让它走代理只能给 JVM 设系统属性。
        for argument in proxy_args(proxy) {
            command.arg(argument);
        }
        command
            .arg("-jar")
            .arg(&jar)
            .arg("--installClient")
            .arg(&game_dir)
            .current_dir(&game_dir)
            .output()
            .await
            .map_err(|error| {
                ForgeError::Installer(format!("无法启动安装器（{}）：{error}", java.display()))
            })?
    };

    let log_tail = tail_lines(&output);
    if !output.status.success() {
        return Err(ForgeError::Installer(explain_failure(
            &log_tail,
            output.status.code(),
        )));
    }

    let json = game_dir
        .join("versions")
        .join(&expected)
        .join(format!("{expected}.json"));
    if !json.is_file() {
        return Err(ForgeError::Installer(format!(
            "安装器报告成功，但没有生成 {expected}.json。最后几行输出：\n{}",
            log_tail.join("\n")
        )));
    }

    Ok(ForgeInstallReport {
        version_id: expected,
        log_tail,
    })
}

/// 安装器要求目标目录「看起来像启动器目录」。
///
/// 缺这个文件时它直接报「你还没有运行过启动器」，而我们的游戏目录是自己管的，
/// 从来不会有它。写一个最小 stub 即可，不覆盖已有内容。
fn ensure_launcher_profiles(game_dir: &Path) -> Result<(), ForgeError> {
    let path = game_dir.join("launcher_profiles.json");
    if path.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(&path, "{\"profiles\":{},\"settings\":{},\"version\":3}\n")?;
    Ok(())
}

async fn download_installer(
    downloader: &HttpDownloader,
    kind: ForgeKind,
    game_version: &str,
    loader_version: &str,
    game_dir: &Path,
) -> Result<PathBuf, ForgeError> {
    let url = installer_url(kind, game_version, loader_version);
    let name = url.rsplit('/').next().unwrap_or("installer.jar").to_owned();

    let dir = game_dir.join(INSTALLER_DIR);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&name);
    if path.is_file() {
        return Ok(path);
    }

    // 安装器 jar 有 5-8 MB，走下载器的镜像与重试逻辑，别自己拼 reqwest。
    let bytes = downloader
        .fetch_bytes_with_headers(&url, &crate::forge::user_agent(), &[])
        .await?;
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

/// 下载器要求的 UA。
pub(crate) fn user_agent() -> String {
    format!("QingciCraftLauncher/{}", env!("CARGO_PKG_VERSION"))
}

/// 取输出的最后若干行，失败时给人看。
pub(crate) fn tail_lines(output: &std::process::Output) -> Vec<String> {
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    let keep = 15;
    if lines.len() > keep {
        lines = lines.split_off(lines.len() - keep);
    }
    lines
}

/// 把安装器的失败翻译成能指导用户的话。
fn explain_failure(log: &[String], code: Option<i32>) -> String {
    let joined = log.join("\n");
    let hint = if joined.contains("launcher_profiles") {
        "安装器认为目标目录不是启动器目录（缺少 launcher_profiles.json）"
    } else if joined.contains("Failed to validate certificates") || joined.contains("certificate") {
        "安装器无法校验 Mojang / Forge 主机的证书——通常是网络被劫持或代理拦截了 HTTPS"
    } else if joined.contains("Could not connect") || joined.contains("UnknownHost") {
        "安装器连不上 Mojang 或 Forge 的主机。注意安装器**不走镜像**：它访问的是\
         launchermeta.mojang.com 等官方地址"
    } else if joined.contains("java.lang.UnsupportedClassVersionError") {
        "安装器要求的 Java 版本比当前运行时更高，请换一个更新的 Java"
    } else {
        "安装器执行失败"
    };
    format!(
        "{hint}（退出码 {}）：\n{joined}",
        code.map_or("-".to_owned(), |c| c.to_string())
    )
}

/// 从 `<version>…</version>` 里抽出版本号。
fn versions_in_metadata(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        out.push(after[..end].trim().to_owned());
        rest = &after[end + "</version>".len()..];
    }
    out
}

/// 只接受 `数字.数字[.数字…]` 形式。
///
/// 挡掉两类脏数据：`latest` 这类非版本号，以及旧式 Forge 坐标
/// `1.20.1-47.2.0-1.20.1`（去掉 MC 前缀后剩下 `47.2.0-1.20.1`，段里带连字符）。
fn is_numeric_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// 按版本号数值降序排列。
fn sort_desc(list: &mut [LoaderInfo]) {
    list.sort_by(|a, b| compare_versions(&b.version, &a.version));
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(left), parse(right));
    for index in 0..a.len().max(b.len()) {
        let x = a.get(index).copied().unwrap_or(0);
        let y = b.get(index).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

/// 把 `host:port` 形式的代理翻译成 JVM 系统属性。
///
/// 安装器用的是 `HttpURLConnection`，它认 `http.proxyHost` / `https.proxyHost`
/// 这一组属性（SOCKS 的 `socksProxyHost` 对 HTTPS 支持不稳）。
/// 只给 `host:port`，不支持 `http://` 前缀与认证——这两样在本地代理里用不到，
/// 多写的解析分支反而是没人测的代码。
pub(crate) fn proxy_args(proxy: Option<&str>) -> Vec<String> {
    let Some(proxy) = proxy.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let Some((host, port)) = proxy.rsplit_once(':') else {
        return Vec::new();
    };
    if host.is_empty() || port.parse::<u16>().is_err() {
        return Vec::new();
    }
    vec![
        format!("-Dhttp.proxyHost={host}"),
        format!("-Dhttp.proxyPort={port}"),
        format!("-Dhttps.proxyHost={host}"),
        format!("-Dhttps.proxyPort={port}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmclapi_neoforge_list_filters_beta_and_sorts() {
        // 形状取自 BMCLAPI `/neoforge/list/26.2` 的真实响应：
        // 四段版本号、beta 与正式混排、顺序不保证。
        let raw = r#"[
            {"version":"26.2.0.0-beta","mcversion":"26.2"},
            {"version":"26.2.0.57","mcversion":"26.2"},
            {"version":"26.2.0.93","mcversion":"26.2"},
            {"version":"26.1.2.125","mcversion":"26.1"}
        ]"#;
        let list = parse_bmclapi_builds(raw).expect("应可解析");

        // beta 被 `is_numeric_version` 挡掉，且按数值降序：93 > 57。
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].version, "26.2.0.93");
        assert_eq!(list[1].version, "26.2.0.57");
        assert_eq!(list[2].version, "26.1.2.125");
    }

    #[test]
    fn neoforge_prefix_maps_minecraft_versions() {
        // 去掉前导的 1；只剩一段时补 .0
        assert_eq!(neoforge_prefix("1.21"), Some("21.0.".to_owned()));
        assert_eq!(neoforge_prefix("1.21.1"), Some("21.1.".to_owned()));
        // 新版本号体系（26.x）不再有前导 1
        assert_eq!(neoforge_prefix("26.3"), Some("26.3.".to_owned()));
        assert_eq!(neoforge_prefix("1.20.4"), Some("20.4.".to_owned()));
    }

    #[test]
    fn neoforge_rejects_versions_without_a_standalone_build() {
        // 1.20.1 时代 NeoForge 还叫 net.neoforged:forge，没有独立制品
        assert_eq!(neoforge_prefix("1.20.1"), None);
        assert_eq!(neoforge_prefix("1.19.2"), None);
        assert_eq!(neoforge_prefix("garbage"), None);
    }

    #[test]
    fn forge_metadata_filters_by_game_version_and_sorts_desc() {
        let raw = r#"<metadata>
            <versioning><versions>
                <version>1.20.1-47.1.0</version>
                <version>1.20.1-47.2.0</version>
                <version>1.20.1-47.10.1</version>
                <version>1.21.1-52.0.1</version>
                <version>1.20.1-47.2.0-1.20.1</version>
            </versions></versioning>
        </metadata>"#;

        let list = parse_forge_metadata(raw, "1.20.1");
        let versions: Vec<&str> = list.iter().map(|item| item.version.as_str()).collect();

        // 数值序：47.10.1 比 47.2.0 新，不能按字符串排
        assert_eq!(versions, ["47.10.1", "47.2.0", "47.1.0"]);
        // 脏数据（带 MC 后缀的旧式坐标）被挡掉
        assert!(!versions.contains(&"47.2.0-1.20.1"));
    }

    #[test]
    fn bmclapi_builds_are_parsed_and_sorted() {
        let raw = r#"[{"version":"47.1.0"},{"version":"47.10.1"},{"version":"beta"}]"#;
        let list = parse_bmclapi_builds(raw).expect("应当能解析");
        let versions: Vec<&str> = list.iter().map(|item| item.version.as_str()).collect();
        assert_eq!(versions, ["47.10.1", "47.1.0"]);
        // 这两个数据源都没有稳定/测试之分，不能编造
        assert!(list.iter().all(|item| !item.stable));
    }

    #[test]
    fn neoforge_metadata_filters_by_prefix() {
        let raw = r#"<versions>
            <version>21.1.80</version>
            <version>21.1.79</version>
            <version>21.10.1</version>
            <version>21.4.100</version>
        </versions>"#;

        let list = parse_neoforge_metadata(raw, "21.1.");
        let versions: Vec<&str> = list.iter().map(|item| item.version.as_str()).collect();

        // 前缀必须带结尾的点，否则 21.10.x 会被当成 21.1 的构建
        assert_eq!(versions, ["21.1.80", "21.1.79"]);
    }

    #[test]
    fn installer_urls_follow_maven_layout() {
        assert_eq!(
            installer_url(ForgeKind::Forge, "1.20.1", "47.2.0"),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.2.0/\
             forge-1.20.1-47.2.0-installer.jar"
        );
        assert_eq!(
            installer_url(ForgeKind::NeoForge, "1.21.1", "21.1.80"),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.80/\
             neoforge-21.1.80-installer.jar"
        );
    }

    #[test]
    fn version_ids_match_what_the_installers_create() {
        assert_eq!(
            version_id(ForgeKind::Forge, "1.20.1", "47.2.0"),
            "1.20.1-forge-47.2.0"
        );
        assert_eq!(
            version_id(ForgeKind::NeoForge, "1.21.1", "21.1.80"),
            "neoforge-21.1.80"
        );
    }

    #[test]
    fn kind_round_trips_through_settings_strings() {
        for kind in [ForgeKind::Forge, ForgeKind::NeoForge] {
            assert_eq!(ForgeKind::parse(kind.id()), Some(kind));
        }
        assert_eq!(ForgeKind::parse(" NeoForge "), Some(ForgeKind::NeoForge));
        assert_eq!(ForgeKind::parse("fabric"), None);
    }

    #[test]
    fn proxy_becomes_jvm_properties() {
        assert_eq!(
            proxy_args(Some("127.0.0.1:10808")),
            [
                "-Dhttp.proxyHost=127.0.0.1",
                "-Dhttp.proxyPort=10808",
                "-Dhttps.proxyHost=127.0.0.1",
                "-Dhttps.proxyPort=10808",
            ]
        );
        // 没给、给空、给错格式都不该产生参数——宁可直连也别拼出半截配置
        assert!(proxy_args(None).is_empty());
        assert!(proxy_args(Some("  ")).is_empty());
        assert!(proxy_args(Some("127.0.0.1")).is_empty());
    }

    #[test]
    fn launcher_profiles_stub_is_only_written_when_missing() {
        let dir = tempfile::tempdir().expect("临时目录");
        ensure_launcher_profiles(dir.path()).expect("首次应当写入");
        let path = dir.path().join("launcher_profiles.json");
        assert!(path.is_file());

        // 已有内容不能被覆盖——用户可能真的用官方启动器打开过这个目录
        std::fs::write(&path, "{\"profiles\":{\"keep\":1}}").expect("写入");
        ensure_launcher_profiles(dir.path()).expect("已存在时应当跳过");
        let text = std::fs::read_to_string(&path).expect("读取");
        assert!(text.contains("keep"));
    }
}
