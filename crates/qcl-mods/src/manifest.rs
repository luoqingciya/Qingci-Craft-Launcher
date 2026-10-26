// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 已安装模组的元数据清单。
//!
//! 为什么要单独记一份：`mods/` 里只有文件名，而文件名无法可靠地反推
//! 「这是哪个 Modrinth 项目、哪个版本」。没有这份清单就做不了更新检查——
//! 只能靠猜文件名，而文件名格式各作者都不一样。
//!
//! 清单放在 `mods/.qcl-manifest.json`，以文件名为键：
//!
//! - 用户手动扔进 `mods/` 的 jar 不在清单里，界面上就是「未知来源」，不参与更新检查
//! - 文件被手动删掉时，清单里的孤儿条目会在读取时被清理，不会无限增长

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ModError;

/// 清单文件名。以点开头，避免被当成模组。
const MANIFEST_NAME: &str = ".qcl-manifest.json";

/// 一个已安装模组的来源信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledMod {
    /// Modrinth 项目 id。
    pub project_id: String,
    /// 安装时的版本号（如 `mc26.3-0.9.3-beta.1-fabric`）。
    pub version_number: String,
}

/// 清单：文件名 → 来源信息。
pub type Manifest = BTreeMap<String, InstalledMod>;

fn manifest_path(mods_dir: &Path) -> PathBuf {
    mods_dir.join(MANIFEST_NAME)
}

/// 读取清单。
///
/// 文件不存在、内容损坏都返回空清单：清单只是「更新检查」的辅助信息，
/// 坏了不该让模组页打不开。
#[must_use]
pub fn load(mods_dir: &Path) -> Manifest {
    std::fs::read_to_string(manifest_path(mods_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// 写回清单。失败时报错——否则「装了但记不下」会让更新检查一直漏。
pub fn save(mods_dir: &Path, manifest: &Manifest) -> Result<(), ModError> {
    std::fs::create_dir_all(mods_dir).map_err(|error| ModError::Download(error.to_string()))?;

    let raw = serde_json::to_string_pretty(manifest)?;
    std::fs::write(manifest_path(mods_dir), raw)
        .map_err(|error| ModError::Download(error.to_string()))
}

/// 记下一个刚装好的模组。
pub fn remember(
    mods_dir: &Path,
    filename: &str,
    project_id: &str,
    version_number: &str,
) -> Result<(), ModError> {
    let mut manifest = load(mods_dir);
    manifest.insert(
        filename.to_owned(),
        InstalledMod {
            project_id: project_id.to_owned(),
            version_number: version_number.to_owned(),
        },
    );
    save(mods_dir, &manifest)
}

/// 忘掉一个被删除的模组。
pub fn forget(mods_dir: &Path, filename: &str) -> Result<(), ModError> {
    let mut manifest = load(mods_dir);
    if manifest.remove(filename).is_some() {
        save(mods_dir, &manifest)?;
    }
    Ok(())
}

/// 清理掉磁盘上已不存在的条目。
///
/// 用户可能直接删 jar 文件（或整目录拷走一部分），清单会留下孤儿。
/// 返回清理后的清单，调用方决定是否写回。
#[must_use]
pub fn prune(mods_dir: &Path, manifest: Manifest) -> Manifest {
    manifest
        .into_iter()
        .filter(|(filename, _)| mods_dir.join(filename).is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{InstalledMod, forget, load, prune, remember, save};
    use tempfile::TempDir;

    #[test]
    fn remember_then_load() {
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("mods");

        remember(&mods, "sodium.jar", "AANobbMI", "mc26.3-0.9.3").expect("应能记下");

        let manifest = load(&mods);
        assert_eq!(
            manifest.get("sodium.jar"),
            Some(&InstalledMod {
                project_id: "AANobbMI".to_owned(),
                version_number: "mc26.3-0.9.3".to_owned(),
            })
        );
    }

    #[test]
    fn remember_overwrites_the_same_file() {
        // 更新模组时文件名可能变，但同一个项目不该留下两条互不相干的记录。
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("mods");

        remember(&mods, "sodium-1.jar", "AANobbMI", "1.0").expect("应能记下");
        remember(&mods, "sodium-2.jar", "AANobbMI", "2.0").expect("应能记下");

        let manifest = load(&mods);
        assert_eq!(manifest.len(), 2, "不同文件名各占一条");
        assert_eq!(manifest["sodium-2.jar"].version_number, "2.0");
    }

    #[test]
    fn forget_removes_entry() {
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("mods");

        remember(&mods, "sodium.jar", "AANobbMI", "1.0").expect("应能记下");
        forget(&mods, "sodium.jar").expect("应能忘掉");

        assert!(load(&mods).is_empty());
    }

    #[test]
    fn forget_missing_entry_is_not_an_error() {
        let dir = TempDir::new().expect("临时目录");
        forget(&dir.path().join("mods"), "ghost.jar").expect("删不存在的条目不该报错");
    }

    #[test]
    fn corrupt_manifest_reads_as_empty() {
        // 清单坏了不该让模组页打不开。
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).expect("应能建目录");
        std::fs::write(mods.join(".qcl-manifest.json"), b"{ not json").expect("应能写入");

        assert!(load(&mods).is_empty());
    }

    #[test]
    fn prune_drops_entries_for_missing_files() {
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).expect("应能建目录");

        remember(&mods, "kept.jar", "A", "1").expect("应能记下");
        remember(&mods, "gone.jar", "B", "1").expect("应能记下");
        std::fs::write(mods.join("kept.jar"), b"x").expect("应能写入");

        let pruned = prune(&mods, load(&mods));

        assert!(pruned.contains_key("kept.jar"));
        assert!(!pruned.contains_key("gone.jar"), "文件不存在时应清理");
    }

    #[test]
    fn save_creates_missing_directory() {
        let dir = TempDir::new().expect("临时目录");
        let mods = dir.path().join("nested/mods");

        save(&mods, &Default::default()).expect("应能创建目录并写入");
        assert!(mods.join(".qcl-manifest.json").is_file());
    }
}
