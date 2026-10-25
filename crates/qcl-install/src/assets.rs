//! 资产安装。
//!
//! 流程：取索引任务 → 拉索引并校验 SHA-1 → 生成对象计划 → 复用下载执行器 →
//! （旧版索引）展开虚拟目录。
//!
//! 资产是首次启动下载量的主体（26.3 约 462 MB / 5147 个对象），因此这里刻意
//! 不做任何额外拷贝：现代布局下对象仓库就是最终形态，游戏直接按哈希读取。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use qcl_core::assets::{
    ASSET_OBJECTS_BASE, AssetObjects, is_legacy_index, is_safe_logical_path, maps_to_resources,
    object_relative_path,
};
use qcl_core::version::VersionMeta;
use qcl_net::{
    DownloadError, DownloadOps, DownloadOptions, HttpDownloader, Progress, asset_index_task,
    execute_plan, plan_assets_with_base, verify_sha1,
};

/// 安装参数。
#[derive(Debug, Clone)]
pub struct AssetInstallOptions {
    /// 资产根目录（相对游戏目录），官方布局是 `assets`。
    pub assets_dir: String,
    /// 对象仓库的下载根地址。
    ///
    /// 默认是官方 CDN。留成配置项有两个实际用途：镜像站，以及让测试能指向本地
    /// 服务——否则验证下载路径就只能真的访问公网。
    pub objects_base: String,
}

impl Default for AssetInstallOptions {
    fn default() -> Self {
        Self {
            assets_dir: "assets".to_owned(),
            objects_base: ASSET_OBJECTS_BASE.to_owned(),
        }
    }
}

/// 安装结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetInstallReport {
    /// 索引代号，如 `34`、`legacy`。
    pub index_id: String,
    /// 计划中的对象数量（已按哈希去重）。
    pub objects: usize,
    /// 实际下载的文件数。
    pub downloaded: usize,
    /// 已存在且校验通过、被跳过的文件数。
    pub skipped: usize,
    /// 计划字节数，等于索引的去重合计。
    pub total_bytes: u64,
    /// 旧版目录树展开出的文件数（含 `pre-1.6` 的 `resources/` 副本）。
    ///
    /// 现代版本为 0。已是正确大小而跳过拷贝的文件不计入。
    pub virtual_files: usize,
}

/// 资产安装过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// 版本未声明资产索引。
    ///
    /// 原版版本都有索引；缺失通常意味着传入的是自定义或被裁剪过的元数据。
    #[error("该版本未声明资产索引，无法安装资产")]
    MissingIndex,

    /// 索引文件的 SHA-1 与版本元数据不符。
    #[error("资产索引校验失败（可能被截断或篡改）")]
    IndexChecksum,

    /// 索引 JSON 解析失败。
    #[error("资产索引解析失败：{0}")]
    IndexParse(#[from] serde_json::Error),

    /// 网络或落盘失败。
    #[error("资产下载失败：{0}")]
    Download(#[from] DownloadError),

    /// 虚拟目录展开时的文件系统错误。
    #[error("展开虚拟资产目录失败：{0}")]
    Io(String),
}

/// 安装某个版本的全部资产。
///
/// `on_progress` 收到的是**对象下载**阶段的进度；虚拟目录展开是本地拷贝，
/// 不计入该进度。
pub async fn install_assets<F>(
    downloader: Arc<HttpDownloader>,
    meta: &VersionMeta,
    options: &AssetInstallOptions,
    on_progress: F,
) -> Result<AssetInstallReport, AssetError>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    let index_task = asset_index_task(meta, &options.assets_dir).ok_or(AssetError::MissingIndex)?;

    // 1. 索引本身也校验。它是后续所有文件的来源，不校验则逐对象校验形同虚设。
    //    校验在写盘之前完成，失败时磁盘上不会留下任何东西。
    let raw = downloader.fetch_text(&index_task.url).await?;
    if !verify_sha1(&index_task.sha1, raw.as_bytes()) {
        return Err(AssetError::IndexChecksum);
    }
    let index: AssetObjects = serde_json::from_str(&raw)?;

    // 校验通过后把索引落盘到 `assets/indexes/<id>.json`，与官方启动器一致。
    // 游戏不读它，但保留一份能让「对象目录被误删后重新校验」和离线排查有据可依；
    // 端到端跑一次才发现此前只用了索引的 URL 与摘要，文件本身没有写出来。
    let index_file = downloader.game_dir().join(&index_task.path);
    if let Some(parent) = index_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| AssetError::Io(format!("{}: {error}", parent.display())))?;
    }
    std::fs::write(&index_file, &raw)
        .map_err(|error| AssetError::Io(format!("{}: {error}", index_file.display())))?;

    // 2. 生成对象计划并交给下载执行器。重试、并发、`.part` 原子落盘全部继承。
    let plan = plan_assets_with_base(&index, &options.assets_dir, &options.objects_base);
    let total_bytes = plan.total_bytes();

    // 先取具体类型再在绑定处 unsize——直接对 &Arc<HttpDownloader> 调 Arc::clone
    // 会被推断成 Arc<dyn DownloadOps>::clone，类型对不上。
    let concrete: Arc<HttpDownloader> = Arc::clone(&downloader);
    let ops: Arc<dyn DownloadOps> = concrete;
    let report = execute_plan(&plan, &DownloadOptions::default(), ops, on_progress).await;

    let downloaded = report.downloaded;
    let skipped = report.skipped;
    if let Some(failure) = report.failures.into_iter().next() {
        return Err(AssetError::Download(failure.error));
    }

    // 3. 旧版索引需要把对象仓库还原成目录树。
    let index_id = meta
        .asset_index
        .as_ref()
        .map_or_else(String::new, |index| index.id.clone());

    let mut virtual_files = 0;
    if is_legacy_index(&index_id) {
        let game_dir = downloader.game_dir();
        let objects_root = game_dir.join(&options.assets_dir).join("objects");

        virtual_files += expand_virtual(
            &index,
            &objects_root,
            &game_dir
                .join(&options.assets_dir)
                .join("virtual")
                .join(&index_id),
        )?;

        // 1.5.x 及更早还会直接从 resources/ 读声音等资源。
        if maps_to_resources(&index_id) {
            virtual_files += expand_virtual(&index, &objects_root, &game_dir.join("resources"))?;
        }
    }

    Ok(AssetInstallReport {
        index_id,
        objects: plan.len(),
        downloaded,
        skipped,
        total_bytes,
        virtual_files,
    })
}

/// 按逻辑路径把对象仓库展开成目录树，返回实际拷贝的文件数。
///
/// 这是本地拷贝，文件数可达数千（1.5.2 约 5000 个），因此放在下载完成之后执行，
/// 不占用下载的并发额度。
///
/// 目标已存在且大小一致时跳过。这里只比大小而不重算哈希：源文件本身在下载阶段
/// 已按 SHA-1 校验过，拷贝是逐字节的，大小一致足以排除上次中断留下的半截文件。
fn expand_virtual(
    index: &AssetObjects,
    objects_root: &Path,
    target_root: &Path,
) -> Result<usize, AssetError> {
    let mut written = 0;

    for (logical, object) in &index.objects {
        // 逻辑路径直接拼到目标目录下，`..` 就能写到游戏目录之外。
        // 官方数据里不会有这种键，但它来自网络，必须拦。
        if !is_safe_logical_path(logical) {
            continue;
        }
        let Some(relative) = object_relative_path(&object.hash) else {
            continue;
        };

        let source: PathBuf = objects_root.join(&relative);
        let target: PathBuf = target_root.join(logical);

        if std::fs::metadata(&target).is_ok_and(|metadata| metadata.len() == object.size) {
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| AssetError::Io(format!("{}: {error}", parent.display())))?;
        }
        std::fs::copy(&source, &target).map_err(|error| {
            AssetError::Io(format!(
                "{} → {}: {error}",
                source.display(),
                target.display()
            ))
        })?;
        written += 1;
    }

    Ok(written)
}
