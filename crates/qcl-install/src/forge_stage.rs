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
//!
//! # `data` 段不是下载清单
//!
//! `install_profile.json` 的 `data` 里那些 `[g:a:v:classifier@ext]` **不是**要下载的构件，
//! 而是 processor 的**输出路径**。以 Forge 1.20.1 为例，14 个坐标全部能对应到某个
//! processor 的 `--output`：
//!
//! | data 键 | 由哪个 processor 产出 |
//! | --- | --- |
//! | `MAPPINGS` | `MCP_DATA --output {MAPPINGS}`（输入是 `mcp_config@zip`） |
//! | `MOJMAPS` | `DOWNLOAD_MOJMAPS --output {MOJMAPS}` |
//! | `MERGED_MAPPINGS` | `MERGE_MAPPING --output {MERGED_MAPPINGS}` |
//! | `MC_UNPACKED` | `BUNDLER_EXTRACT --output {MC_UNPACKED}` |
//! | `MC_SLIM` / `MC_EXTRA` | `jarsplitter --slim/--extra` |
//! | `MC_SRG` | `ForgeAutoRenamingTool --output {MC_SRG}` |
//! | `PATCHED` | `binarypatcher --output {PATCHED}` |
//!
//! 安装器自己也从不下载它们：`PostProcessors.loadData` 只把 `[...]` 包成 `ArtifactEntry`
//! 并解析成 `libraries/` 下的**本地路径**。所以拿它们去请求 Maven 必然 404——
//! 四个源（Forge maven / BMCLAPI / Central / libraries.minecraft.net）实测全 404。
//!
//! 真正要备的输入只有三类：`libraries[]`、`processors[].jar`、`processors[].classpath`
//! 与 `processors[].args` 里的 `[...]`。后两类实测是**冗余兜底**——Forge 1.20.1（46 个库）
//! 与 NeoForge 21.1.72（88 条 / 79 个唯一路径）的 processor 引用全部已被 `libraries[]` 覆盖，
//! 加上去不会多下任何文件（去重后仍是 46 / 79）。保留扫描只为防上游改结构时静默缺件。

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
    /// `install_profile` 给出的 SHA-1。实测 1.20.1 的 46 个、21.1.72 的 88 个库
    /// 全部带摘要；processor 里 `[...]` 引用的构件没有摘要，那时为 `None`。
    pub sha1: Option<String>,
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
    processors: Vec<ProfileProcessor>,
}

/// 一个 processor 步骤。只关心它需要本地存在哪些 jar。
#[derive(Deserialize)]
struct ProfileProcessor {
    #[serde(default)]
    jar: Option<String>,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
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
    #[serde(default)]
    sha1: Option<String>,
}

/// 从 install_profile 文本里算出要下载的构件清单。
///
/// 三类输入：`libraries[]`、`processors[]` 的 `jar` / `classpath` / `args` 里的 `[...]`。
/// `data` 段里的 `[...]` **不是**输入（见模块文档），不会被收进来。
///
/// 每个构件都给出**一组**地址：profile 自带的 url 在前，其后是镜像回落。
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
        urls.extend(mirror_urls(&path));
        out.push(StagedArtifact {
            sha1: artifact.and_then(|a| a.sha1.clone()),
            path,
            urls,
        });
    }

    // processor 自身与它引用的构件。实测这两个版本都被 `libraries[]` 覆盖，
    // 这里只做兜底：processor 是独立 JVM，classpath 上缺一个只会报
    // `Missing Jar for processor`，不说是哪个，排查成本很高。
    for processor in &parsed.processors {
        let mut references: Vec<&str> = Vec::new();
        if let Some(jar) = processor.jar.as_deref() {
            references.push(jar);
        }
        references.extend(processor.classpath.iter().map(String::as_str));
        for arg in &processor.args {
            if let Some(inner) = arg
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                references.push(inner);
            }
        }

        for reference in references {
            let path = coordinate_path(reference)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            out.push(StagedArtifact {
                urls: mirror_urls(&path),
                path,
                // processor 引用没有摘要可校验；这类构件都在官方 maven 上，
                // 且它们的用途只是喂给 processor，最终产物另有 SHA 校验。
                sha1: None,
            });
        }
    }

    Ok(out)
}

/// 一个构件在各镜像上的候选地址。
///
/// `net/neoforged/` 只发布在 NeoForge 仓库，先试它能省两次 404 往返。
fn mirror_urls(path: &str) -> Vec<String> {
    if path.starts_with("net/neoforged/") {
        vec![
            format!("{NEOFORGE_MAVEN_BASE}{path}"),
            format!("{MAVEN_CENTRAL_BASE}{path}"),
            format!("{FORGE_MAVEN_BASE}{path}"),
        ]
    } else {
        vec![
            format!("{MAVEN_CENTRAL_BASE}{path}"),
            format!("{FORGE_MAVEN_BASE}{path}"),
            format!("{NEOFORGE_MAVEN_BASE}{path}"),
        ]
    }
}

/// Maven 坐标 → 仓库内路径。`g:a:v[:classifier][@ext]`
pub(crate) fn coordinate_path(coordinate: &str) -> Result<String, ForgeError> {
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
                    // 摘要不符就换下一个源，而不是直接失败：镜像返回 200 却给错内容时，
                    // 重试同一个镜像永远拿到同一份错文件（与 qcl-net 的回落策略一致）。
                    if let Some(expected) = &item.sha1
                        && !qcl_net::verify_sha1(expected, &bytes)
                    {
                        last_error = Some(format!("{url} → SHA-1 不匹配（期望 {expected}）"));
                        continue;
                    }
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
                "url": "https://maven.minecraftforge.net/com/google/guava/guava/20.0/guava-20.0.jar",
                "sha1": "b8947f6c9e7b4b0c2b8b0f8c8b0c2b8b0f8c8b0c"
              } } },
            { "name": "net.minecraftforge:forge:1.20.1-47.2.0:universal" },
            { "name": "de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip",
              "downloads": { "artifact": {
                "path": "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip",
                "url": "https://maven.minecraftforge.net/de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
              } } }
        ],
        "processors": [
            { "jar": "net.minecraftforge:installertools:1.3.0",
              "classpath": [ "net.minecraftforge:installertools:1.3.0", "net.md-5:SpecialSource:1.11.0" ],
              "args": [ "--task", "MCP_DATA",
                        "--input", "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]",
                        "--output", "{MAPPINGS}", "--key", "mappings" ] }
        ],
        "data": {
            "MAPPINGS": { "client": "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412:mappings@txt]" },
            "MC_SLIM": { "client": "[net.minecraft:client:1.20.1-20230612.114412:slim]" },
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
        // `/data/xxx` 是 jar 内嵌文件，不该出现在下载清单里
        assert!(!paths.iter().any(|path| path.starts_with("/data/")));
    }

    #[test]
    fn data_coordinates_are_outputs_not_downloads() {
        // 回归：曾把 data 里的 14 个坐标当下载项，四个源全 404。
        // 它们都是 processor 的 --output，安装器自己写，不该出现在清单里。
        let plan = plan_from_profile(PROFILE).expect("应当能解析");
        let paths: Vec<&str> = plan.iter().map(|item| item.path.as_str()).collect();

        assert!(
            !paths
                .iter()
                .any(|path| path.ends_with("-mappings.txt") || path.ends_with("-slim.jar")),
            "data 段坐标不该进下载清单：{paths:?}"
        );
        // 真正的输入 `mcp_config@zip` 来自 libraries，仍然要在
        assert!(paths.contains(
            &"de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        ));
    }

    #[test]
    fn processor_jars_and_args_are_staged() {
        // 兜底：真实 profile 里这些都被 libraries 覆盖，但缺一个只会报
        // `Missing Jar for processor`，所以清单要能自己兜住
        let plan = plan_from_profile(PROFILE).expect("应当能解析");
        let paths: Vec<&str> = plan.iter().map(|item| item.path.as_str()).collect();

        assert!(
            paths.contains(&"net/minecraftforge/installertools/1.3.0/installertools-1.3.0.jar")
        );
        // classpath 上的依赖同样要备
        assert!(paths.contains(&"net/md-5/SpecialSource/1.11.0/SpecialSource-1.11.0.jar"));
        // processor args 里的 [坐标] 是输入，@zip 决定扩展名
        assert!(paths.contains(
            &"de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        ));
        // `{KEY}` 占位符不是坐标，不能被当成构件
        assert!(
            !paths
                .iter()
                .any(|path| path.contains('{') || path.contains('}'))
        );
    }

    #[test]
    fn neoforge_artifacts_try_neoforged_repo_first() {
        let profile = r#"{ "libraries": [
            { "name": "net.neoforged.installertools:installertools:2.1.2" }
        ] }"#;
        let plan = plan_from_profile(profile).expect("应当能解析");
        assert!(
            plan[0].urls[0].starts_with(NEOFORGE_MAVEN_BASE),
            "NeoForge 构件应先试 NeoForge 仓库：{:?}",
            plan[0].urls
        );
    }

    #[test]
    fn libraries_carry_their_sha1_for_verification() {
        // 实测 1.20.1 的 46 个库、21.1.72 的 88 个库全都带摘要，备料时要校验
        let plan = plan_from_profile(PROFILE).expect("应当能解析");
        let guava = plan
            .iter()
            .find(|item| item.path.ends_with("guava-20.0.jar"))
            .expect("应当有 guava");
        assert_eq!(
            guava.sha1.as_deref(),
            Some("b8947f6c9e7b4b0c2b8b0f8c8b0c2b8b0f8c8b0c")
        );
        // processor 引用的构件没有摘要，不能凭空造一个
        let tool = plan
            .iter()
            .find(|item| item.path.contains("installertools"))
            .expect("应当有 installertools");
        assert_eq!(tool.sha1, None);
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
