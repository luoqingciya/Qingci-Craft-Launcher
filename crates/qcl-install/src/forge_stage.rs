// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 离线安装的「备料」阶段：把安装器 `--offline` 所需的前置文件放到本地。
//!
//! 为什么要这么做：安装器是独立 JVM，硬编码访问 `launchermeta` 等官方主机，
//! 不走我们的下载器与镜像。国内网络下它会卡在下载库文件——实测 17 分钟没跑完，
//! 而同一台机器配代理后 58 秒装完。
//!
//! `--offline` 能让它跳过所有网络调用，但有两个前提（实测得出）：
//!
//! 1. **本地必须有 vanilla client jar**，否则它仍要读版本清单去找下载地址，
//!    离线模式下读不到就中止（`Failed to download version manifest`）
//! 2. **install_profile 里的库必须齐备**，缺一个就报错，且错误信息不说是哪个
//!
//! 所以本模块只做「备料」：解析 profile、把库下载到 `libraries/`。
//! 原版那部分（client jar 与 29 个库）由 [`crate::install_version`] 负责，调用方先跑它。
//!
//! 数据文件（`data/*.lzma` 等）不用我们解：安装器手里有 jar，自己会解。

use std::io::Read;
use std::path::{Path, PathBuf};

use qcl_net::HttpDownloader;
use serde::Deserialize;

use crate::forge::{FORGE_MAVEN_BASE, ForgeError, NEOFORGE_MAVEN_BASE};

/// Maven Central。BMCLAPI 的 `/maven/` **不镜像 Central**——install_profile 里的
/// guava / jsr305 / gson / errorprone 都在 Central，只试镜像会 404。
pub const MAVEN_CENTRAL_BASE: &str = "https://repo1.maven.org/maven2/";

/// 一个待下载的构件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArtifact {
    /// 相对 `libraries/` 的路径，如 `com/google/guava/guava/20.0/guava-20.0.jar`。
    pub path: String,
    /// 依次尝试的地址（镜像 → Central）。
    pub urls: Vec<String>,
}

/// 备料结果。
#[derive(Debug, Clone, Default)]
pub struct StageReport {
    /// 已就绪（含原本就在的）。
    pub ready: usize,
    /// 本次实际下载的。
    pub downloaded: usize,
}

/// 从安装器 jar 里读出 `install_profile.json`。
pub fn read_install_profile(installer: &Path) -> Result<String, ForgeError> {
    let file = std::fs::File::open(installer)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| ForgeError::Installer(format!("安装器不是有效的 jar：{error}")))?;
    let mut entry = archive
        .by_name("install_profile.json")
        .map_err(|_| ForgeError::Installer("安装器里没有 install_profile.json".to_owned()))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(text)
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    libraries: Vec<ProfileLibrary>,
    #[serde(default)]
    data: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[derive(Deserialize)]
struct ProfileLibrary {
    name: String,
    #[serde(default)]
    downloads: Option<LibraryDownloads>,
}

#[derive(Deserialize)]
struct LibraryDownloads {
    #[serde(default)]
    artifact: Option<Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// 从 install_profile 文本里算出要下载的构件清单。
///
/// 每个构件都给出**一组**地址：镜像在前、Central 在后。原因见 [`MAVEN_CENTRAL_BASE`]。
pub fn plan_from_profile(profile: &str) -> Result<Vec<StagedArtifact>, ForgeError> {
    let parsed: Profile = serde_json::from_str(profile)?;
    let mut out: Vec<StagedArtifact> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for library in &parsed.libraries {
        let artifact = library.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let path = match artifact.and_then(|a| a.path.clone()) {
            Some(path) => path,
            None => coordinate_path(&library.name)?,
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        let mut urls = Vec::new();
        if let Some(url) = artifact.and_then(|a| a.url.clone()) {
            urls.push(url);
        }
        urls.push(format!("{MAVEN_CENTRAL_BASE}{path}"));
        urls.push(format!("{FORGE_MAVEN_BASE}{path}"));
        out.push(StagedArtifact { path, urls });
    }

    // `data` 里的 `[group:artifact:version:classifier@ext]` 也要备齐（BINPATCH 之类）。
    for entry in parsed.data.values() {
        for value in entry.values() {
            let Some(inner) = value
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            else {
                continue;
            };
            let path = coordinate_path(inner)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            out.push(StagedArtifact {
                urls: vec![
                    format!("{MAVEN_CENTRAL_BASE}{path}"),
                    format!("{FORGE_MAVEN_BASE}{path}"),
                    format!("{NEOFORGE_MAVEN_BASE}{path}"),
                ],
                path,
            });
        }
    }

    Ok(out)
}

/// Maven 坐标 → 仓库内路径。`g:a:v[:classifier][@ext]`
fn coordinate_path(coordinate: &str) -> Result<String, ForgeError> {
    let (main, ext) = match coordinate.split_once('@') {
        Some((main, ext)) => (main, ext),
        None => (coordinate, "jar"),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if parts.len() < 3 {
        return Err(ForgeError::Installer(format!(
            "无法解析坐标 `{coordinate}`"
        )));
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Ok(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

/// 把清单里的构件备齐到 `<game_dir>/libraries/`。
///
/// 幂等：已存在的文件跳过。**不校验摘要**——install_profile 里没给 sha1，
/// 而安装器在离线模式下自己也不校验，我们多校验一次只会引入不一致。
pub async fn stage_prerequisites(
    downloader: &HttpDownloader,
    installer: &Path,
    game_dir: &Path,
) -> Result<StageReport, ForgeError> {
    let plan = plan_from_profile(&read_install_profile(installer)?)?;
    let mut report = StageReport::default();
    let agent = format!("QingciCraftLauncher/{}", env!("CARGO_PKG_VERSION"));

    for item in plan {
        let target = game_dir.join("libraries").join(&item.path);
        if target.is_file() {
            report.ready += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut last_error = None;
        let mut written = false;
        for url in &item.urls {
            match downloader.fetch_bytes_with_headers(url, &agent, &[]).await {
                Ok(bytes) => {
                    std::fs::write(&target, &bytes)?;
                    written = true;
                    break;
                }
                Err(error) => last_error = Some(format!("{url} → {error}")),
            }
        }
        if !written {
            return Err(ForgeError::Installer(format!(
                "备料失败，所有源都取不到 {}：{}",
                item.path,
                last_error.unwrap_or_default()
            )));
        }
        report.downloaded += 1;
        report.ready += 1;
    }

    Ok(report)
}

/// 备料目录（供调用方定位）。
pub fn libraries_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("libraries")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"{
        "libraries": [
            { "name": "com.google.guava:guava:20.0",
              "downloads": { "artifact": {
                "path": "com/google/guava/guava/20.0/guava-20.0.jar",
                "url": "https://maven.minecraftforge.net/com/google/guava/guava/20.0/guava-20.0.jar"
              } } },
            { "name": "net.minecraftforge:forge:1.20.1-47.2.0:universal" }
        ],
        "data": {
            "MAPPINGS": { "client": "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412:mappings@txt]" },
            "BINPATCH": { "client": "/data/client.lzma" }
        }
    }"#;

    #[test]
    fn plan_prefers_given_path_and_falls_back_to_coordinate() {
        let plan = plan_from_profile(PROFILE).expect("应当能解析");
        let paths: Vec<&str> = plan.iter().map(|item| item.path.as_str()).collect();

        assert!(paths.contains(&"com/google/guava/guava/20.0/guava-20.0.jar"));
        // 没给 downloads 的条目要靠坐标自己拼，classifier 要进文件名
        assert!(
            paths.contains(
                &"net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-universal.jar"
            )
        );
        // data 里的坐标同样要备，且 @txt 决定扩展名
        assert!(paths.contains(
            &"de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412-mappings.txt"
        ));
        // `/data/xxx` 是 jar 内嵌文件，不该出现在下载清单里
        assert!(!paths.iter().any(|path| path.starts_with("/data/")));
    }

    #[test]
    fn every_artifact_has_a_central_fallback() {
        let plan = plan_from_profile(PROFILE).expect("应当能解析");
        for item in &plan {
            assert!(
                item.urls
                    .iter()
                    .any(|url| url.starts_with(MAVEN_CENTRAL_BASE)),
                "{} 缺少 Central 兜底",
                item.path
            );
        }
    }

    #[test]
    fn duplicates_are_collapsed() {
        let profile = r#"{ "libraries": [
            { "name": "a.b:c:1.0" }, { "name": "a.b:c:1.0" }
        ], "data": {} }"#;
        let plan = plan_from_profile(profile).expect("应当能解析");
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn broken_coordinate_is_reported_not_ignored() {
        let profile = r#"{ "libraries": [ { "name": "not-a-coordinate" } ], "data": {} }"#;
        assert!(plan_from_profile(profile).is_err());
    }
}
