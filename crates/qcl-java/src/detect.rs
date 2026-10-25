//! Java 安装探测与选择。
//!
//! 探测顺序即优先级：显式指定 → `JAVA_HOME` → `PATH` → 启动器自管目录 → 平台常见安装目录。
//! 先出现的候选先被采用，因此用户显式指定的 Java 永远优先于自动发现的结果。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::version::{
    JavaVersion, parse_java_version, parse_java_version_output, parse_release_file,
};

/// 安装来源。用于向用户解释「为什么选中了这个 Java」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaSource {
    /// 实例或全局设置中显式指定。
    Explicit,
    /// 由 `JAVA_HOME` 环境变量指定。
    JavaHome,
    /// 由 `PATH` 中的 `java` 反推。
    Path,
    /// 启动器自行下载管理的运行时。
    Managed,
    /// 平台常见安装目录中扫描到。
    PlatformDirectory,
}

/// 一个可用的 Java 安装。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallation {
    /// JDK / JRE 根目录。
    pub home: PathBuf,
    /// 可执行文件完整路径。
    pub executable: PathBuf,
    /// 版本。
    pub version: JavaVersion,
    /// 来源。
    pub source: JavaSource,
}

/// 当前平台的 Java 可执行文件名。
#[must_use]
pub const fn java_executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// 由根目录推出可执行文件路径。
#[must_use]
pub fn java_executable(home: &Path) -> PathBuf {
    home.join("bin").join(java_executable_name())
}

/// 目录是否是有效的 Java 根目录（存在 `bin/java`）。
#[must_use]
pub fn is_java_home(home: &Path) -> bool {
    java_executable(home).is_file()
}

/// 检查目录并读取版本，得到可用的安装。
///
/// 版本读取失败时返回 `None`：没有版本的安装无法参与匹配，
/// 保留它只会让后续逻辑需要处理一堆 `Option`。
#[must_use]
pub fn inspect(home: &Path, source: JavaSource) -> Option<JavaInstallation> {
    let executable = java_executable(home);
    if !executable.is_file() {
        return None;
    }
    let version = read_version(home)?;
    Some(JavaInstallation {
        home: home.to_path_buf(),
        executable,
        version,
        source,
    })
}

/// 在给定根目录下寻找 Java 安装。
///
/// 根目录本身就是一个 Java 根目录时直接返回它；否则扫描其直接子目录，
/// 并兼容 macOS 的 `*.jdk/Contents/Home` 结构。
#[must_use]
pub fn discover_in(root: &Path) -> Vec<PathBuf> {
    if is_java_home(root) {
        return vec![root.to_path_buf()];
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    // 排序保证探测结果稳定，便于测试与问题复现。
    children.sort();

    let mut homes = Vec::new();
    for child in children {
        if is_java_home(&child) {
            homes.push(child);
            continue;
        }
        let bundle_home = child.join("Contents").join("Home");
        if is_java_home(&bundle_home) {
            homes.push(bundle_home);
        }
    }
    homes
}

/// 探测本机可用的 Java 安装。
///
/// `explicit` 是用户显式指定的根目录（实例级或全局设置），优先级最高。
/// `managed_root` 是启动器自管运行时的父目录。
#[must_use]
pub fn detect(explicit: &[PathBuf], managed_root: Option<&Path>) -> Vec<JavaInstallation> {
    let mut candidates: Vec<(PathBuf, JavaSource)> = Vec::new();

    for home in explicit {
        candidates.push((home.clone(), JavaSource::Explicit));
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push((PathBuf::from(home), JavaSource::JavaHome));
    }
    for home in path_candidates() {
        candidates.push((home, JavaSource::Path));
    }
    if let Some(root) = managed_root {
        for home in discover_in(root) {
            candidates.push((home, JavaSource::Managed));
        }
    }
    for root in platform_roots() {
        for home in discover_in(&root) {
            candidates.push((home, JavaSource::PlatformDirectory));
        }
    }

    let mut installations = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    for (home, source) in candidates {
        // 同一份安装可能通过多条路径被发现（例如 JAVA_HOME 同时出现在 PATH 里）。
        let identity = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
        if seen.contains(&identity) {
            continue;
        }
        if let Some(installation) = inspect(&home, source) {
            seen.push(identity);
            installations.push(installation);
        }
    }

    installations
}

/// 为指定主版本挑选安装。
///
/// 规则：**精确匹配优先**，否则取「不低于要求的最低版本」。
/// 绝不向下兼容——用 Java 17 跑要求 25 的版本不会降级运行，而是直接报错，
/// 与其让用户在游戏里看到莫名其妙的异常，不如在启动前明确失败。
#[must_use]
pub fn select(
    installations: &[JavaInstallation],
    required_major: u32,
) -> Option<&JavaInstallation> {
    if let Some(exact) = installations
        .iter()
        .find(|installation| installation.version.major == required_major)
    {
        return Some(exact);
    }

    installations
        .iter()
        .filter(|installation| installation.version.major > required_major)
        .min_by_key(|installation| installation.version.major)
}

/// 平台常见安装根目录。
#[must_use]
pub fn platform_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            let Some(base) = std::env::var_os(variable) else {
                continue;
            };
            let base = PathBuf::from(base);
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "Zulu",
                "Microsoft",
            ] {
                roots.push(base.join(vendor));
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        roots.push(PathBuf::from("/System/Library/Java/JavaVirtualMachines"));
    } else {
        roots.push(PathBuf::from("/usr/lib/jvm"));
        roots.push(PathBuf::from("/usr/java"));
    }

    roots.retain(|root| root.is_dir());
    roots
}

/// 从 `PATH` 反推 Java 根目录。
fn path_candidates() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };

    std::env::split_paths(&path)
        .filter(|entry| entry.join(java_executable_name()).is_file())
        .filter_map(|entry| entry.parent().map(Path::to_path_buf))
        .collect()
}

/// 读取安装版本：优先 `release` 文件，失败再执行 `java -version`。
fn read_version(home: &Path) -> Option<JavaVersion> {
    if let Ok(content) = std::fs::read_to_string(home.join("release"))
        && let Some(raw) = parse_release_file(&content)
        && let Some(version) = parse_java_version(&raw)
    {
        return Some(version);
    }
    version_from_process(home)
}

/// 执行 `java -version` 并解析输出。
fn version_from_process(home: &Path) -> Option<JavaVersion> {
    let output = Command::new(java_executable(home))
        .arg("-version")
        .output()
        .ok()?;

    // 版本信息写到 stderr 是 Java 的长期行为；部分发行版也会写 stdout，两者都尝试。
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    parse_java_version_output(&stderr).or_else(|| parse_java_version_output(&stdout))
}
