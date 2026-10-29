// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! `inheritsFrom` 合并：把父版本的元数据叠进加载器版本的 JSON。
//!
//! Forge / NeoForge 安装器写出的 `versions/<id>/<id>.json` 只有 **29 个库**、没有
//! `assetIndex`、没有 `downloads`，游戏参数里也没有 `--username` 一类——这些全在
//! `inheritsFrom` 指向的原版版本里。不合并就直接启动，得到的是一条缺 88 个库、
//! 缺资产索引、缺玩家参数的命令，游戏必然起不来。
//!
//! 合并规则（与官方启动器一致）：
//!
//! - **身份类字段**取子版本：`id`、`mainClass`、`type`、发布时间
//! - **文件类字段**子版本没写就继承父版本：`assetIndex`、`downloads`、`javaVersion`、`logging`
//! - **库**：父在前、子在后，同名只留一份（子覆盖父）。两个版本各带一份 asm 时都要保留，
//!   Forge 靠 `-p` 模块路径与 `-DignoreList` 自己处理，去重反而会破坏它
//! - **参数**：父在前、子追加在后。Forge 的 `--launchTarget forgeclient` 必须排在
//!   原版的 `--username` 之后
//! - **1.12.2 及更早**：只有 `minecraftArguments` 字符串，同样父在前、子追加

use std::path::{Path, PathBuf};

use qcl_core::version::VersionMeta;
use qcl_net::HttpDownloader;

use crate::manifest::{ManifestError, fetch_manifest, fetch_meta, resolve_version};

/// 合并父版本时可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum InheritError {
    /// 读取本地版本文件失败。
    #[error("读取版本文件失败：{0}")]
    Io(#[from] std::io::Error),

    /// 版本文件不是合法 JSON。
    #[error("解析版本文件失败：{0}")]
    Parse(#[from] serde_json::Error),

    /// 父版本既不在本地，也无法从上游取得。
    #[error("父版本 {version} 不可用：{source}")]
    Parent {
        /// 父版本 id。
        version: String,
        /// 上游错误。
        #[source]
        source: ManifestError,
    },
}

/// 把父版本的元数据合并进子版本。
#[must_use]
pub fn merge_inherited(parent: &VersionMeta, child: &VersionMeta) -> VersionMeta {
    let mut merged = parent.clone();

    // 身份类字段以子版本为准。
    merged.id = child.id.clone();
    merged.main_class = child.main_class.clone();
    merged.version_type = child.version_type.clone();
    merged.release_time = child.release_time.clone();
    merged.time = child.time.clone();

    // 文件类字段：子版本显式写了才覆盖。Forge 这些一律不写。
    if child.asset_index.is_some() {
        merged.asset_index = child.asset_index.clone();
    }
    if child.assets.is_some() {
        merged.assets = child.assets.clone();
    }
    if child.downloads.is_some() {
        merged.downloads = child.downloads.clone();
    }
    if child.java_version.is_some() {
        merged.java_version = child.java_version.clone();
    }
    if child.logging.is_some() {
        merged.logging = child.logging.clone();
    }
    if child.minimum_launcher_version.is_some() {
        merged.minimum_launcher_version = child.minimum_launcher_version;
    }
    if child.compliance_level.is_some() {
        merged.compliance_level = child.compliance_level;
    }

    // 库：父在前、子在后；同名只留一份（子覆盖父）。
    let mut libraries = parent.libraries.clone();
    for library in &child.libraries {
        match libraries
            .iter_mut()
            .find(|existing| existing.name == library.name)
        {
            Some(existing) => *existing = library.clone(),
            None => libraries.push(library.clone()),
        }
    }
    merged.libraries = libraries;

    // 参数：父在前、子追加在后。两边都没有 `arguments` 时保持 `None`，
    // 否则会把 1.12.2 这类只用 `minecraftArguments` 的版本误判成新格式。
    let mut arguments = parent.arguments.clone();
    if let Some(extra) = &child.arguments {
        let target = arguments.get_or_insert_with(qcl_core::version::Arguments::default);
        target.game.extend(extra.game.iter().cloned());
        target.jvm.extend(extra.jvm.iter().cloned());
    }
    merged.arguments = arguments;

    // 1.12.2 及更早的字符串形式，同样是父在前、子追加。
    merged.minecraft_arguments = match (&parent.minecraft_arguments, &child.minecraft_arguments) {
        (Some(base), Some(extra)) => Some(format!("{base} {extra}")),
        (None, Some(extra)) => Some(extra.clone()),
        (Some(base), None) => Some(base.clone()),
        (None, None) => None,
    };

    merged
}

/// 如果本地版本 JSON 声明了 `inheritsFrom`，就把父版本合并进来；否则原样返回。
///
/// 父版本优先取本地（离线可用），本地没有才去上游取。
pub async fn resolve_inherited(
    downloader: &HttpDownloader,
    game_dir: &Path,
    child: VersionMeta,
) -> Result<VersionMeta, InheritError> {
    let Some(parent_id) = declared_parent(game_dir, &child.id)? else {
        return Ok(child);
    };

    let parent = match read_local(game_dir, &parent_id) {
        Some(meta) => meta,
        None => {
            let manifest =
                fetch_manifest(downloader)
                    .await
                    .map_err(|source| InheritError::Parent {
                        version: parent_id.clone(),
                        source,
                    })?;
            let entry =
                resolve_version(&manifest, &parent_id).map_err(|source| InheritError::Parent {
                    version: parent_id.clone(),
                    source,
                })?;
            fetch_meta(downloader, entry)
                .await
                .map_err(|source| InheritError::Parent {
                    version: parent_id.clone(),
                    source,
                })?
        }
    };

    Ok(merge_inherited(&parent, &child))
}

/// 本地版本 JSON 里声明的 `inheritsFrom`。
///
/// 单独读一遍原始 JSON：`VersionMeta` 不保留这个字段（它只描述合并后的结果），
/// 为它加一个字段会让所有构造点都要跟着改。
fn declared_parent(game_dir: &Path, version: &str) -> Result<Option<String>, InheritError> {
    let path = meta_path(game_dir, version);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    Ok(value
        .get("inheritsFrom")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

fn read_local(game_dir: &Path, version: &str) -> Option<VersionMeta> {
    let raw = std::fs::read_to_string(meta_path(game_dir, version)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn meta_path(game_dir: &Path, version: &str) -> PathBuf {
    game_dir
        .join("versions")
        .join(version)
        .join(format!("{version}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> VersionMeta {
        serde_json::from_str(raw).expect("测试用的版本 JSON 应当合法")
    }

    /// 把参数列表摊平成字符串，便于断言顺序。
    fn plain(arguments: &[qcl_core::version::Argument]) -> Vec<String> {
        arguments
            .iter()
            .flat_map(|argument| match argument {
                qcl_core::version::Argument::Plain(value) => vec![value.clone()],
                qcl_core::version::Argument::Conditional { value, .. } => value.as_slice().to_vec(),
            })
            .collect()
    }

    const PARENT: &str = r#"{
        "id": "1.20.1",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "releaseTime": "2023-06-12T00:00:00+00:00",
        "time": "2023-06-12T00:00:00+00:00",
        "javaVersion": { "component": "java-runtime-gamma", "majorVersion": 17 },
        "assetIndex": { "id": "5", "sha1": "aa", "size": 1, "totalSize": 2, "url": "https://x/5.json" },
        "downloads": { "client": { "sha1": "bb", "size": 3, "url": "https://x/client.jar" } },
        "libraries": [
            { "name": "com.google.guava:guava:31.1-jre" },
            { "name": "org.ow2.asm:asm:9.3" }
        ],
        "arguments": {
            "game": ["--username", "${auth_player_name}"],
            "jvm": ["-Djava.library.path=${natives_directory}"]
        }
    }"#;

    const CHILD: &str = r#"{
        "id": "1.20.1-forge-47.2.0",
        "type": "release",
        "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
        "releaseTime": "2023-09-21T00:00:00+00:00",
        "time": "2023-09-21T00:00:00+00:00",
        "libraries": [
            { "name": "org.ow2.asm:asm:9.5" },
            { "name": "com.google.guava:guava:31.1-jre" }
        ],
        "arguments": {
            "game": ["--launchTarget", "forgeclient"],
            "jvm": ["-p", "${library_directory}/cpw/mods/bootstraplauncher.jar"]
        }
    }"#;

    #[test]
    fn identity_comes_from_the_child() {
        let merged = merge_inherited(&parse(PARENT), &parse(CHILD));
        assert_eq!(merged.id, "1.20.1-forge-47.2.0");
        assert_eq!(
            merged.main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(merged.release_time, "2023-09-21T00:00:00+00:00");
    }

    #[test]
    fn files_come_from_the_parent() {
        let merged = merge_inherited(&parse(PARENT), &parse(CHILD));
        // 这三样 Forge 的 JSON 里根本没有，缺了就没法装也没法启动
        assert!(merged.asset_index.is_some());
        assert!(merged.downloads.is_some());
        assert_eq!(merged.java_version.map(|java| java.major_version), Some(17));
    }

    #[test]
    fn libraries_keep_both_asm_versions() {
        let merged = merge_inherited(&parse(PARENT), &parse(CHILD));
        let names: Vec<&str> = merged
            .libraries
            .iter()
            .map(|library| library.name.as_str())
            .collect();
        // 父在前、子在后
        assert_eq!(
            names,
            vec![
                "com.google.guava:guava:31.1-jre",
                "org.ow2.asm:asm:9.3",
                "org.ow2.asm:asm:9.5",
            ]
        );
    }

    #[test]
    fn same_library_is_not_duplicated() {
        // 子版本重复声明父版本已有的库时，只留一份（以子为准）
        let merged = merge_inherited(&parse(PARENT), &parse(CHILD));
        let guava = merged
            .libraries
            .iter()
            .filter(|library| library.name == "com.google.guava:guava:31.1-jre")
            .count();
        assert_eq!(guava, 1);
    }

    #[test]
    fn arguments_are_parent_then_child() {
        let merged = merge_inherited(&parse(PARENT), &parse(CHILD));
        let arguments = merged.arguments.expect("应当有 arguments");
        let game = plain(&arguments.game);
        let jvm = plain(&arguments.jvm);
        assert_eq!(game[0], "--username");
        assert_eq!(game[2], "--launchTarget");
        assert_eq!(jvm[0], "-Djava.library.path=${natives_directory}");
        assert_eq!(jvm[1], "-p");
    }

    #[test]
    fn legacy_arguments_are_concatenated() {
        let parent = parse(
            r#"{ "id": "1.12.2", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                 "releaseTime": "", "time": "",
                 "minecraftArguments": "--username ${auth_player_name}" }"#,
        );
        let child = parse(
            r#"{ "id": "1.12.2-forge-1", "type": "release", "mainClass": "net.minecraft.launchwrapper.Launch",
                 "releaseTime": "", "time": "",
                 "minecraftArguments": "--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker" }"#,
        );
        let merged = merge_inherited(&parent, &child);
        assert_eq!(
            merged.minecraft_arguments.as_deref(),
            Some(
                "--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"
            )
        );
        // 没有 `arguments` 时不能被强行塞一个空的，否则会被判成新格式
        assert!(merged.arguments.is_none());
        assert!(merged.uses_legacy_arguments());
    }

    #[test]
    fn declared_parent_is_read_from_the_raw_json() {
        let dir = tempfile::tempdir().unwrap();
        let version_dir = dir.path().join("versions").join("1.20.1-forge-47.2.0");
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(
            version_dir.join("1.20.1-forge-47.2.0.json"),
            r#"{ "id": "1.20.1-forge-47.2.0", "inheritsFrom": "1.20.1" }"#,
        )
        .unwrap();

        assert_eq!(
            declared_parent(dir.path(), "1.20.1-forge-47.2.0").unwrap(),
            Some("1.20.1".to_owned())
        );
        // 原版版本没有 inheritsFrom
        assert_eq!(declared_parent(dir.path(), "1.20.1").unwrap(), None);
    }
}
