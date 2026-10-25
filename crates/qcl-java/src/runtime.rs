//! Mojang 官方 Java 运行时清单。
//!
//! 清单分两级：
//!
//! 1. **索引**（`all.json`）：平台 → 组件 → 版本条目，每个条目指向下一级的文件清单。
//! 2. **文件清单**（`manifest.json`）：逐文件给出路径、SHA-1、大小与下载地址。
//!
//! 相比 Adoptium 的整包 zip，官方清单是**文件级**的：不需要解压，能精确复用已存在的文件，
//! 也能拿到每个文件的可执行标志。因此优先走这条路，Adoptium 只作为平台缺失时的兜底。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 运行时索引：`all.json`。外层键是平台，内层键是组件名。
pub type RuntimeIndex = BTreeMap<String, BTreeMap<String, Vec<RuntimeRelease>>>;

/// 索引中的一个运行时版本条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRelease {
    /// 版本信息。
    pub version: RuntimeVersion,
    /// 文件清单的下载信息。
    pub manifest: ManifestPointer,
    /// 可用性信息（发布进度等），不参与选型决策。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
}

/// 运行时版本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeVersion {
    /// 版本名，如 `25.0.1`。
    pub name: String,
    /// 发布时间（ISO 8601）。
    pub released: String,
}

/// 文件清单的下载指针。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPointer {
    /// 清单文件的 SHA-1。
    pub sha1: String,
    /// 清单文件大小（字节）。
    pub size: u64,
    /// 清单文件下载地址。
    pub url: String,
}

/// 可用性信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    /// 发布分组。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// 发布进度百分比。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u32>,
}

/// 文件清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeManifest {
    /// 相对路径 → 条目。
    pub files: BTreeMap<String, RuntimeEntry>,
}

/// 清单中的单个条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEntry {
    /// 条目类型。
    #[serde(rename = "type")]
    pub kind: RuntimeEntryKind,
    /// 下载信息。`directory` 类型没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<RuntimeDownloads>,
    /// 是否需要在类 Unix 系统上置为可执行。
    ///
    /// 官方对 124 个文件标了该标志，其中包含大量 DLL——在 Windows 上没有实际作用，
    /// 但解压到 Linux/macOS 时必须照做，否则 `bin/java` 无法执行。
    #[serde(default)]
    pub executable: bool,
    /// 符号链接目标，仅 `link` 类型有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// 条目类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeEntryKind {
    /// 目录。
    Directory,
    /// 普通文件。
    File,
    /// 符号链接。
    Link,
}

/// 条目的下载信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDownloads {
    /// 未压缩文件。启动器使用这一项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<RuntimeArtifact>,
    /// LZMA 压缩文件。需要额外解压能力，暂不使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lzma: Option<RuntimeArtifact>,
}

/// 单个可下载文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeArtifact {
    /// SHA-1。
    pub sha1: String,
    /// 大小（字节）。
    pub size: u64,
    /// 下载地址。
    pub url: String,
}

/// 运行时安装计划中的一个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFileTask {
    /// 相对游戏目录的落地路径。
    pub path: String,
    /// 下载地址。
    pub url: String,
    /// SHA-1。
    pub sha1: String,
    /// 大小（字节）。
    pub size: u64,
    /// 落地后是否需要置为可执行。
    pub executable: bool,
}

/// 运行时安装计划。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimePlan {
    /// 待下载文件。
    pub files: Vec<RuntimeFileTask>,
}

impl RuntimePlan {
    /// 需要下载的总字节数。
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }

    /// 文件数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// 运行时在游戏目录下的安装根目录。
#[must_use]
pub fn runtime_home(component: &str, platform: &str) -> String {
    format!("runtime/{component}/{platform}")
}

/// 把运行时的相对路径解析成游戏目录下的落地路径。
#[must_use]
pub fn runtime_install_path(component: &str, platform: &str, relative: &str) -> String {
    format!("{}/{relative}", runtime_home(component, platform))
}

/// 当前平台对应的官方运行时平台键。
#[must_use]
pub fn runtime_platform_key() -> Option<&'static str> {
    platform_key_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// 由系统名与架构推出官方运行时平台键。
///
/// 官方把 32 位 x86 单独列为一个平台（`windows-x86`、`linux-i386`），
/// 缺失映射会让这些用户完全无法安装运行时，因此逐项穷举而非模糊匹配。
#[must_use]
pub fn platform_key_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("windows-x64"),
        ("windows", "x86") => Some("windows-x86"),
        ("windows", "aarch64") => Some("windows-arm64"),
        ("macos", "x86_64") => Some("mac-os"),
        ("macos", "aarch64") => Some("mac-os-arm64"),
        ("linux", "x86_64") => Some("linux"),
        ("linux", "x86") => Some("linux-i386"),
        _ => None,
    }
}

/// 在索引中查找指定组件与平台的运行时条目。
///
/// 注意：官方索引里存在「组件键存在但条目数组为空」的情况——32 位 x86 平台
/// （`windows-x86`、`linux-i386`）就是如此，Mojang 不为它们发布现代运行时。
/// 因此不能用「键是否存在」判断可用性，必须检查数组非空。
#[must_use]
pub fn select_release<'a>(
    index: &'a RuntimeIndex,
    component: &str,
    platform: &str,
) -> Option<&'a RuntimeRelease> {
    index
        .get(platform)?
        .get(component)?
        // 同一组件理论上只有一条；若出现多条，取最后一条（官方按发布时间追加）。
        .last()
}

/// 生成运行时安装计划。
///
/// 只收集 `file` 类型且带 `raw` 下载地址的条目：
/// - `directory` 由下载器按需创建父目录，不必单列；
/// - `link` 属于 macOS 上的符号链接，解压后的实际内容已覆盖，暂不处理；
/// - 只有 `lzma` 而没有 `raw` 的条目跳过（需要额外的解压能力）。
#[must_use]
pub fn plan_runtime_files(
    manifest: &RuntimeManifest,
    component: &str,
    platform: &str,
) -> RuntimePlan {
    let mut files = Vec::new();

    for (relative, entry) in &manifest.files {
        if entry.kind != RuntimeEntryKind::File {
            continue;
        }
        let Some(artifact) = entry
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.raw.as_ref())
        else {
            continue;
        };

        files.push(RuntimeFileTask {
            path: runtime_install_path(component, platform, relative),
            url: artifact.url.clone(),
            sha1: artifact.sha1.clone(),
            size: artifact.size,
            executable: entry.executable,
        });
    }

    RuntimePlan { files }
}
