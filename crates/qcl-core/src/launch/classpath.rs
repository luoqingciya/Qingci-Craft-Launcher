//! classpath 与 natives 构件组装。
//!
//! 这里只做**路径计算**，不碰文件系统：下载、校验、解压属于安装层职责。
//! 这样划分的好处是组装逻辑可以在任意平台上完整测试，且不依赖磁盘状态。

use crate::version::{LaunchContext, Library, VersionMeta};

/// 把 Maven 坐标转换为仓库内相对路径。
///
/// 支持 `group:artifact:version`、`group:artifact:version:classifier` 与
/// `...@zip` 形式。字段数量异常或缺少必需字段时返回 `None`。
///
/// 部分旧版本条目没有 `downloads.artifact`，只能靠坐标推导路径，
/// 因此这个函数是必要的兜底，而不是可选的便利函数。
#[must_use]
pub fn maven_to_path(coordinate: &str) -> Option<String> {
    let (coordinate, extension) = match coordinate.split_once('@') {
        Some((left, extension)) => (left, extension),
        None => (coordinate, "jar"),
    };

    let mut parts = coordinate.split(':');
    let group = parts.next().filter(|part| !part.is_empty())?;
    let artifact = parts.next().filter(|part| !part.is_empty())?;
    let version = parts.next().filter(|part| !part.is_empty())?;
    let classifier = parts.next().filter(|part| !part.is_empty());
    if parts.next().is_some() {
        return None;
    }

    let file_name = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };

    Some(format!(
        "{}/{artifact}/{version}/{file_name}",
        group.replace('.', "/")
    ))
}

/// 依赖库在游戏目录下的相对路径。
///
/// 优先使用官方给出的 `artifact.path`；缺失时按 Maven 坐标推导。
#[must_use]
pub fn library_path(library: &Library) -> Option<String> {
    if let Some(path) = library
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
        .and_then(|artifact| artifact.path.clone())
    {
        return Some(path);
    }
    maven_to_path(&library.name)
}

/// 组装 classpath 条目（相对游戏目录的路径）。
///
/// 规则：
/// 1. 只取当前环境下适用的库；
/// 2. 保持 `version.json` 中的原始顺序——类加载顺序会影响行为，不能重排；
/// 3. 按路径去重（合并模组加载器的库列表时会出现重复条目）；
/// 4. 版本 jar 放在最后，与官方启动器一致。
///
/// 注意：这里**不做**「同一 artifact 取最高版本」的冲突消解。Maven 版本号的比较语义
/// （如 `1.0-beta` 与 `1.0`）不是简单的字符串或数值比较，凭猜测实现风险高于收益。
/// 等模组加载器 PR 引入真实冲突样本后再针对性实现。
#[must_use]
pub fn build_classpath(
    meta: &VersionMeta,
    context: &LaunchContext,
    version_jar: &str,
) -> Vec<String> {
    let mut entries: Vec<String> = Vec::with_capacity(meta.libraries.len() + 1);

    for library in meta
        .libraries
        .iter()
        .filter(|library| library.applies_to(context))
    {
        let Some(path) = library_path(library) else {
            continue;
        };
        // 线性查重：条目规模在百级，哈希表带来的借用复杂度不划算。
        if entries.contains(&path) {
            continue;
        }
        entries.push(path);
    }

    if !entries.iter().any(|entry| entry == version_jar) {
        entries.push(version_jar.to_owned());
    }

    entries
}

/// 当前平台的 classpath 分隔符。
#[must_use]
pub fn classpath_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

/// 把条目拼接成 JVM 需要的 classpath 字符串。
#[must_use]
pub fn join_classpath(entries: &[String], separator: char) -> String {
    let mut joined = String::new();
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            joined.push(separator);
        }
        joined.push_str(entry);
    }
    joined
}

/// 需要解压到 natives 目录的构件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativesArtifact {
    /// 来源库的 Maven 坐标，便于定位问题。
    pub library: String,
    /// natives jar 的相对路径。
    pub path: String,
    /// 解压时需要排除的路径前缀，通常为 `META-INF/`。
    pub exclude: Vec<String>,
}

/// 收集当前环境需要解压的 natives 构件。
///
/// 1.13 之前的版本把本地库以 classifier 形式挂在依赖库上（如 `natives-windows`），
/// 必须解压到 `natives` 目录并加入 `java.library.path` 才能加载。
#[must_use]
pub fn natives_artifacts(meta: &VersionMeta, context: &LaunchContext) -> Vec<NativesArtifact> {
    let Some(os_name) = context.os_name.as_deref() else {
        return Vec::new();
    };

    let mut artifacts = Vec::new();

    for library in meta
        .libraries
        .iter()
        .filter(|library| library.applies_to(context))
    {
        let Some(natives) = &library.natives else {
            continue;
        };
        let Some(classifier) = natives.get(os_name) else {
            continue;
        };
        let classifier = expand_arch(classifier, context);

        let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.classifiers.as_ref())
            .and_then(|classifiers| classifiers.get(&classifier))
        else {
            continue;
        };

        let path = artifact
            .path
            .clone()
            .or_else(|| maven_to_path(&format!("{}:{classifier}", library.name)));
        let Some(path) = path else {
            continue;
        };

        artifacts.push(NativesArtifact {
            library: library.name.clone(),
            path,
            exclude: library
                .extract
                .as_ref()
                .map(|extract| extract.exclude.clone())
                .unwrap_or_default(),
        });
    }

    artifacts
}

/// 展开 classifier 中的 `${arch}` 占位符。
///
/// 1.7 时代的条目写成 `natives-windows-${arch}`，官方启动器按 32/64 位替换。
fn expand_arch(classifier: &str, context: &LaunchContext) -> String {
    if classifier.contains("${arch}") {
        let bits = if context.arch.as_deref() == Some("x86") {
            "32"
        } else {
            "64"
        };
        classifier.replace("${arch}", bits)
    } else {
        classifier.to_owned()
    }
}
