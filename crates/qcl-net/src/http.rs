//! 基于 reqwest 与本地文件系统的下载实现。
//!
//! 关键约束：
//!
//! 1. **先写 `.part` 再原子重命名**。中途失败或被取消时，最终路径上永远不会出现半截文件；
//!    下次启动的完整性检查因此可以信任「文件存在」这一事实。
//! 2. **落地路径不得越出游戏目录**。`version.json` 来自网络，若其中出现 `../` 或绝对路径，
//!    不加拦截就等于给了对方任意写文件的能力。
//! 3. **边下边算哈希**。避免为了校验把几十 MB 的文件再读一遍。

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::executor::{DownloadError, DownloadOps};
use crate::plan::DownloadTask;
use crate::retry::is_retryable_status;
use crate::verify::Sha1Hasher;

/// 建立连接的超时。不设整体超时：大文件传输耗时本就不可预估。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// 读取本地文件校验时的缓冲区大小。
const VERIFY_BUFFER: usize = 64 * 1024;

/// 基于 HTTP 与本地文件系统的下载器。
#[derive(Debug, Clone)]
pub struct HttpDownloader {
    client: reqwest::Client,
    game_dir: PathBuf,
}

impl HttpDownloader {
    /// 以默认客户端配置创建下载器。
    pub fn new(game_dir: impl Into<PathBuf>) -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("QingciCraftLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        Ok(Self::with_client(client, game_dir))
    }

    /// 使用外部提供的客户端创建下载器（便于复用连接池或注入测试配置）。
    #[must_use]
    pub fn with_client(client: reqwest::Client, game_dir: impl Into<PathBuf>) -> Self {
        Self {
            client,
            game_dir: game_dir.into(),
        }
    }

    /// 游戏目录。
    #[must_use]
    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    /// 把计划中的相对路径解析为绝对路径，并拒绝任何越界路径。
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, DownloadError> {
        let candidate = Path::new(relative);

        if candidate.is_absolute() {
            return Err(DownloadError::UnsafePath(relative.to_owned()));
        }
        // 只接受普通路径段：拒绝 `..`、盘符前缀与根目录。
        if !candidate
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(DownloadError::UnsafePath(relative.to_owned()));
        }

        let mut path = self.game_dir.clone();
        for component in candidate.components() {
            if let Component::Normal(part) = component {
                path.push(part);
            }
        }
        Ok(path)
    }

    /// 主动校验磁盘上的文件是否与期望摘要一致。
    ///
    /// 与 [`DownloadOps::is_present`] 的区别：本方法**总是**做完整哈希，
    /// 供「修复文件」这类用户主动触发的场景使用。
    pub async fn verify_file(&self, relative: &str, expected_sha1: &str) -> bool {
        let Ok(path) = self.resolve(relative) else {
            return false;
        };
        hash_file(&path)
            .await
            .is_some_and(|digest| digest.eq_ignore_ascii_case(expected_sha1))
    }

    /// 下载到 `.part` 临时文件并校验，不负责重命名。
    async fn download_to_part(
        &self,
        task: &DownloadTask,
        part: &Path,
    ) -> Result<(), DownloadError> {
        let response = self
            .client
            .get(&task.url)
            .send()
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            return Err(if is_retryable_status(code) {
                DownloadError::Network(format!("HTTP {code}"))
            } else {
                DownloadError::Other(format!("HTTP {code}"))
            });
        }

        let mut file = tokio::fs::File::create(part)
            .await
            .map_err(|error| DownloadError::Io(error.to_string()))?;
        let mut hasher = Sha1Hasher::new();
        let mut written: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| DownloadError::Network(error.to_string()))?;
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|error| DownloadError::Io(error.to_string()))?;
            written += chunk.len() as u64;
        }

        file.flush()
            .await
            .map_err(|error| DownloadError::Io(error.to_string()))?;
        drop(file);

        // 长度不符通常意味着响应被截断，按网络问题处理并重试。
        if written != task.size {
            return Err(DownloadError::Network(format!(
                "长度不符：期望 {} 字节，实际 {written} 字节",
                task.size
            )));
        }

        if !hasher.finish_and_verify(&task.sha1) {
            return Err(DownloadError::Checksum(task.path.clone()));
        }

        Ok(())
    }
}

#[async_trait]
impl DownloadOps for HttpDownloader {
    /// 判断文件是否已就绪。
    ///
    /// 先比大小（廉价，能挡住截断），再比 SHA-1（较贵，但能挡住内容被篡改或损坏）。
    /// 当前只对版本文件（客户端 jar + 依赖库，约 100 MB）做校验，耗时可接受；
    /// 若将来对全量资产（数百 MB）也逐次校验，需要引入基于大小与修改时间的缓存。
    async fn is_present(&self, task: &DownloadTask) -> bool {
        let Ok(path) = self.resolve(&task.path) else {
            return false;
        };
        let Ok(metadata) = tokio::fs::metadata(&path).await else {
            return false;
        };
        if !metadata.is_file() || metadata.len() != task.size {
            return false;
        }

        hash_file(&path)
            .await
            .is_some_and(|digest| digest.eq_ignore_ascii_case(&task.sha1))
    }

    async fn fetch_once(&self, task: &DownloadTask) -> Result<(), DownloadError> {
        let target = self.resolve(&task.path)?;
        let part = part_path(&target);

        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| DownloadError::Io(error.to_string()))?;
        }

        if let Err(error) = self.download_to_part(task, &part).await {
            // 失败时清掉半成品，避免下次被误认为「已下载」。
            let _ = tokio::fs::remove_file(&part).await;
            return Err(error);
        }

        // 同一目录内重命名：同文件系统上这是原子操作。
        tokio::fs::rename(&part, &target)
            .await
            .map_err(|error| DownloadError::Io(error.to_string()))
    }
}

fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

async fn hash_file(path: &Path) -> Option<String> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let mut hasher = Sha1Hasher::new();
    let mut buffer = vec![0_u8; VERIFY_BUFFER];

    loop {
        match file.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return None,
        }
    }

    Some(hasher.finish())
}
