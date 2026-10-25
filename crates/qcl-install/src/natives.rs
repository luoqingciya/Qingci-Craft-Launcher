//! natives 归档解压。
//!
//! 1.13 之前的版本把本地库（`.dll` / `.so` / `.dylib`）以 classifier 形式挂在依赖库上，
//! 必须解压到 `natives` 目录并把它加进 `-Djava.library.path`，否则 `UnsatisfiedLinkError`。
//!
//! 现代版本不再需要这一步：LWJGL 3 的 natives 作为独立构件进入 classpath，由 LWJGL
//! 自己在运行时解压。因此 `natives_artifacts` 对 26.3 返回空列表，本模块也就什么都不做。

use std::fs::File;
use std::io::copy;
use std::path::Path;

use qcl_core::launch::natives_artifacts;
use qcl_core::version::{LaunchContext, VersionMeta};

/// 一次解压的统计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractOutcome {
    /// 实际写出的文件数。
    pub extracted: usize,
    /// 因命中排除前缀而跳过的条目数（通常是 `META-INF/`）。
    pub excluded: usize,
    /// 因路径不安全而跳过的条目数。
    pub unsafe_entries: usize,
}

/// 全部 natives 构件的安装结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativesReport {
    /// 处理的归档数量。
    pub archives: usize,
    /// 写出的文件总数。
    pub extracted: usize,
    /// 命中排除前缀而跳过的条目总数。
    pub excluded: usize,
    /// 路径不安全而跳过的条目总数。
    pub unsafe_entries: usize,
}

/// natives 安装过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    /// 归档文件不存在——通常是下载阶段漏了这个构件。
    #[error("natives 归档不存在：{0}")]
    MissingArchive(String),

    /// 归档无法打开或读取。
    #[error("natives 归档无法解析：{0}")]
    Archive(String),

    /// 写出文件失败。
    #[error("解压 natives 失败：{0}")]
    Io(String),
}

/// 解压某个版本声明的全部 natives 构件。
///
/// `version_dir` 是版本目录（如 `versions/1.7.10`），natives 解压到其下的 `natives` 子目录，
/// 与 `LaunchLayout` 里的 `natives_dir` 保持一致——两处路径写错的表现都是
/// 「游戏启动后报找不到本地库」，而且很难从日志看出来。
pub fn install_natives(
    game_dir: &Path,
    meta: &VersionMeta,
    context: &LaunchContext,
    version_dir: &str,
) -> Result<NativesReport, NativeError> {
    let artifacts = natives_artifacts(meta, context);
    if artifacts.is_empty() {
        return Ok(NativesReport::default());
    }

    let natives_dir = game_dir.join(version_dir).join("natives");
    let mut report = NativesReport::default();

    for artifact in artifacts {
        let archive = game_dir.join(&artifact.path);
        if !archive.is_file() {
            return Err(NativeError::MissingArchive(archive.display().to_string()));
        }

        let outcome = extract_natives(&archive, &natives_dir, &artifact.exclude)?;
        report.archives += 1;
        report.extracted += outcome.extracted;
        report.excluded += outcome.excluded;
        report.unsafe_entries += outcome.unsafe_entries;
    }

    Ok(report)
}

/// 把归档解压到目标目录，按前缀排除条目。
///
/// 已有同名文件会被覆盖：natives 目录的正确状态完全由归档决定，保留旧文件只会
/// 让「换了版本但加载了旧 dll」这类问题变得难以解释。
pub fn extract_natives(
    archive: &Path,
    target_dir: &Path,
    exclude: &[String],
) -> Result<ExtractOutcome, NativeError> {
    let file = File::open(archive)
        .map_err(|error| NativeError::Archive(format!("{}: {error}", archive.display())))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| NativeError::Archive(format!("{}: {error}", archive.display())))?;

    let mut outcome = ExtractOutcome::default();

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| NativeError::Archive(format!("{}: {error}", archive.display())))?;

        if entry.is_dir() {
            continue;
        }

        // 归档来自网络，条目名不保证是合法 UTF-8；无法解码的一律跳过。
        let name = match entry.name() {
            Ok(name) => name.into_owned(),
            Err(_) => {
                outcome.unsafe_entries += 1;
                continue;
            }
        };
        if exclude.iter().any(|prefix| name.starts_with(prefix)) {
            outcome.excluded += 1;
            continue;
        }

        // 以分隔符开头或带盘符的条目一律拒绝。
        //
        // 不能只靠 `enclosed_name`：`/tmp/x` 在 Unix 上是绝对路径，在 Windows 上
        // 却是「根目录相对」，两者结论不同——只依赖它会让实现与测试都出现平台差异。
        if name.starts_with('/') || name.starts_with('\\') || name.as_bytes().get(1) == Some(&b':')
        {
            outcome.unsafe_entries += 1;
            continue;
        }

        // `..` 穿越交给 `enclosed_name`：它按宿主平台的路径语义判断，
        // 自己拼路径容易漏掉平台差异。
        let Some(relative) = entry.enclosed_name() else {
            outcome.unsafe_entries += 1;
            continue;
        };

        let destination = target_dir.join(&relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| NativeError::Io(format!("{}: {error}", parent.display())))?;
        }

        let mut output = File::create(&destination)
            .map_err(|error| NativeError::Io(format!("{}: {error}", destination.display())))?;
        copy(&mut entry, &mut output)
            .map_err(|error| NativeError::Io(format!("{}: {error}", destination.display())))?;

        outcome.extracted += 1;
    }

    Ok(outcome)
}
