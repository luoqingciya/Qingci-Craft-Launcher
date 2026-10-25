//! 下载计划：从版本元数据推导出「本次安装需要下载哪些文件」。

use std::collections::BTreeSet;

use qcl_core::launch::{maven_to_path, natives_artifacts};
use qcl_core::version::{LaunchContext, VersionMeta};

/// 单个待下载文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    /// 下载地址。
    pub url: String,
    /// 相对游戏目录的落地路径，一律使用正斜杠。
    pub path: String,
    /// 官方给出的 SHA-1，用于完整性校验。
    pub sha1: String,
    /// 文件字节数，用于进度估算与磁盘预检。
    pub size: u64,
}

/// 一次安装所需的全部下载任务。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadPlan {
    /// 任务列表，顺序确定（客户端 jar → 日志配置 → 依赖库 → natives）。
    pub tasks: Vec<DownloadTask>,
}

impl DownloadPlan {
    /// 任务数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// 需要下载的总字节数，用于磁盘空间预检与进度分母。
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.tasks.iter().map(|task| task.size).sum()
    }

    /// 是否已包含指定落地路径。
    #[must_use]
    pub fn contains_path(&self, path: &str) -> bool {
        self.tasks.iter().any(|task| task.path == path)
    }
}

/// 从版本元数据生成下载计划。
///
/// `version_dir` 是该版本在游戏目录下的相对目录，如 `versions/26.3`。
/// 客户端 jar 与 natives 目录都挂在这个目录下。
///
/// 只收集**文件本身**，不含资源索引（`assets`）：索引需要先下载索引文件才能展开，
/// 属于执行器的两阶段流程。
#[must_use]
pub fn plan_version_files(
    meta: &VersionMeta,
    context: &LaunchContext,
    version_dir: &str,
) -> DownloadPlan {
    let mut tasks = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    let mut push = |url: &str, path: String, sha1: &str, size: u64| {
        if sha1.is_empty() || !seen.insert(path.clone()) {
            return;
        }
        tasks.push(DownloadTask {
            url: url.to_owned(),
            path,
            sha1: sha1.to_owned(),
            size,
        });
    };

    // 1. 客户端 jar
    if let Some(client) = meta
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
    {
        push(
            &client.url,
            format!("{version_dir}/{}.jar", meta.id),
            &client.sha1,
            client.size,
        );
    }

    // 2. 日志配置（log4j2 配置缺失会导致游戏无法启动）
    if let Some(logging) = meta
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
        .and_then(|client| client.file.id.as_ref().map(|id| (id, &client.file)))
    {
        let (id, file) = logging;
        push(
            &file.url,
            format!("assets/log_configs/{id}"),
            &file.sha1,
            file.size,
        );
    }

    // 3. 依赖库主构件
    for library in meta
        .libraries
        .iter()
        .filter(|library| library.applies_to(context))
    {
        let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
        else {
            continue;
        };
        let Some(path) = artifact
            .path
            .clone()
            .or_else(|| maven_to_path(&library.name))
        else {
            continue;
        };
        push(&artifact.url, path, &artifact.sha1, artifact.size);
    }

    // 4. natives 构件（1.13 之前的版本才有）
    for natives in natives_artifacts(meta, context) {
        push(
            &natives.artifact.url,
            natives.path,
            &natives.artifact.sha1,
            natives.artifact.size,
        );
    }

    DownloadPlan { tasks }
}
