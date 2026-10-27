// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

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
use crate::source::DownloadSource;
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
    source: DownloadSource,
}

impl HttpDownloader {
    /// 以默认客户端配置创建下载器（官方源）。
    pub fn new(game_dir: impl Into<PathBuf>) -> Result<Self, DownloadError> {
        Self::with_source(game_dir, DownloadSource::Official)
    }

    /// 以指定下载源创建下载器。
    pub fn with_source(
        game_dir: impl Into<PathBuf>,
        source: DownloadSource,
    ) -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("QingciCraftLauncher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| DownloadError::Network(error.to_string()))?;
        Ok(Self::with_client(client, game_dir).using(source))
    }

    /// 换一个下载源。
    ///
    /// 下载器是克隆出去的（安装任务各自持有一份），所以用「生成新的」而不是
    /// 改字段——避免改到别的任务手里那份。
    #[must_use]
    pub fn using(mut self, source: DownloadSource) -> Self {
        self.source = source;
        self
    }

    /// 当前下载源。
    #[must_use]
    pub fn source(&self) -> DownloadSource {
        self.source
    }

    /// 按下载源的顺序发请求，成功就返回。
    ///
    /// 镜像优先、官方兜底。**必须有兜底**：BMCLAPI 的元数据端点会 302 到
    /// `bmclapi*.sakura.sld.tw` 这些 CDN 节点，它们在某些网络下直接 reset——
    /// 没有兜底时用户切到镜像反而连版本列表都拿不到，比不切更糟。
    ///
    /// 只有「连接失败」或「明确的失败状态」才换下一个地址。重定向是镜像的正常行为
    /// （reqwest 会自己跟），不能当成失败。
    async fn send_first_ok<F>(
        &self,
        url: &str,
        build: F,
    ) -> Result<reqwest::Response, DownloadError>
    where
        F: Fn(&reqwest::Client, &str) -> reqwest::RequestBuilder,
    {
        let mut last_response: Option<reqwest::Response> = None;
        let mut last_error: Option<DownloadError> = None;

        for candidate in self.source.candidates(url) {
            match build(&self.client, candidate.as_ref()).send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    // 全部失败时把最后一个响应交回去，让调用方按原有逻辑报错，
                    // 这样错误信息里才有真实的状态码，而不是一句笼统的「都失败了」。
                    last_response = Some(response);
                }
                Err(error) => last_error = Some(DownloadError::Network(error.to_string())),
            }
        }

        if let Some(response) = last_response {
            return Ok(response);
        }

        Err(last_error.unwrap_or_else(|| DownloadError::Other("没有可用的下载地址".to_owned())))
    }

    /// 使用外部提供的客户端创建下载器（便于复用连接池或注入测试配置）。
    #[must_use]
    pub fn with_client(client: reqwest::Client, game_dir: impl Into<PathBuf>) -> Self {
        Self {
            client,
            game_dir: game_dir.into(),
            source: DownloadSource::Official,
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

    /// 把文本内容读入内存，用于清单类小文件（版本 JSON、运行时清单等）。
    ///
    /// 与 [`DownloadOps::fetch_once`] 的区别：不落盘、不做 SHA-1 校验。
    /// 需要校验时由调用方拿返回值自行比对——清单文件很小，留在内存里更直接。
    pub async fn fetch_text(&self, url: &str) -> Result<String, DownloadError> {
        let response = self
            .send_first_ok(url, |client, url| client.get(url))
            .await?;

        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            return Err(if is_retryable_status(code) {
                DownloadError::Network(format!("HTTP {code}"))
            } else {
                DownloadError::Other(format!("HTTP {code}"))
            });
        }

        response
            .text()
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))
    }

    /// 探测远端文件大小。
    ///
    /// 某些清单只给下载地址、不给大小（Fabric 的 loader 构件就是这样）。缺了大小，
    /// [`DownloadOps::is_present`] 就无法判断本地文件是否完整，会导致每次启动都重下。
    ///
    /// 先发 HEAD；拿不到有效长度时退到 `Range: bytes=0-0` 读 `Content-Range`。
    /// 这个兜底不是理论情况：Cloudflare 对分块传输的响应不给 `Content-Length`，
    /// 而 Fabric 的元数据与 maven 都在 Cloudflare 后面。
    pub async fn fetch_size(&self, url: &str) -> Result<u64, DownloadError> {
        if let Ok(size) = self.head_size(url).await
            && size > 0
        {
            return Ok(size);
        }
        self.ranged_size(url).await
    }

    /// 用 HEAD 请求取 `Content-Length`。
    async fn head_size(&self, url: &str) -> Result<u64, DownloadError> {
        let response = self
            .send_first_ok(url, |client, url| client.head(url))
            .await?;

        check_status(response.status())?;

        // 服务端可以不返回 Content-Length（分块传输）。此时明确失败，
        // 让调用方走兜底，而不是返回 0 让上层误以为文件是空的。
        response
            .content_length()
            .filter(|size| *size > 0)
            .ok_or_else(|| DownloadError::Other("响应未提供有效的 Content-Length".to_owned()))
    }

    /// 用单字节范围请求读总长度。
    async fn ranged_size(&self, url: &str) -> Result<u64, DownloadError> {
        let response = self
            .send_first_ok(url, |client, url| {
                client.get(url).header(reqwest::header::RANGE, "bytes=0-0")
            })
            .await?;

        let status = response.status();
        check_status(status)?;

        // 服务端忽略 Range 时返回 200 与完整内容，此时长度就写在 Content-Length 里。
        if status == reqwest::StatusCode::OK {
            return response
                .content_length()
                .filter(|size| *size > 0)
                .ok_or_else(|| DownloadError::Other("响应未提供有效的 Content-Length".to_owned()));
        }

        let header = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| DownloadError::Other("响应未提供 Content-Range".to_owned()))?;

        parse_content_range_total(header)
            .ok_or_else(|| DownloadError::Other(format!("无法解析 Content-Range：{header}")))
    }

    /// 与 `fetch_text` 相同，但可指定 User-Agent。
    ///
    /// Modrinth 要求客户端标明身份，否则可能被限流；而全局改客户端的 UA
    /// 会影响 Mojang 端点的行为，所以只在需要的地方单独带。
    pub async fn fetch_text_with_user_agent(
        &self,
        url: &str,
        user_agent: &str,
    ) -> Result<String, DownloadError> {
        let response = self
            .send_first_ok(url, |client, url| {
                client
                    .get(url)
                    .header(reqwest::header::USER_AGENT, user_agent)
            })
            .await?;

        let status = response.status();
        if !status.is_success() {
            let code = status.as_u16();
            return Err(if is_retryable_status(code) {
                DownloadError::Network(format!("HTTP {code}"))
            } else {
                DownloadError::Other(format!("HTTP {code}"))
            });
        }

        response
            .text()
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))
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
            .send_first_ok(&task.url, |client, url| client.get(url))
            .await?;

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

/// 把 HTTP 状态码转成可分类的错误。
fn check_status(status: reqwest::StatusCode) -> Result<(), DownloadError> {
    if status.is_success() {
        return Ok(());
    }
    let code = status.as_u16();
    Err(if is_retryable_status(code) {
        DownloadError::Network(format!("HTTP {code}"))
    } else {
        DownloadError::Other(format!("HTTP {code}"))
    })
}

/// 从 `Content-Range: bytes 0-0/12345` 里取出总长度。
///
/// `bytes 0-0/*` 表示总长未知，返回 `None`——调用方应把它当成失败，
/// 而不是猜一个数字。
fn parse_content_range_total(value: &str) -> Option<u64> {
    let (_, total) = value.rsplit_once('/')?;
    total.trim().parse().ok()
}

/// 需要自行处理状态码的请求方法。
///
/// 与 [`HttpDownloader::fetch_text`] 的区别：那个把非 2xx 当成错误返回，
/// 而 OAuth 这类协议的错误信息**就写在 4xx 的响应体里**（`authorization_pending`、
/// `expired_token` 等）。把状态码和响应体原样交出去，由调用方解释。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// GET。
    Get,
    /// POST。
    Post,
}

/// 请求体。
#[derive(Debug, Clone, Copy)]
pub enum RequestBody<'a> {
    /// 无请求体。
    Empty,
    /// `application/x-www-form-urlencoded`。
    Form(&'a [(&'a str, &'a str)]),
    /// `application/json`。
    Json(&'a serde_json::Value),
}

/// 原始响应：状态码 + 文本体。
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应体文本。
    pub body: String,
}

impl HttpDownloader {
    /// 发送请求并原样返回状态码与响应体。
    ///
    /// `bearer` 非空时附加 `Authorization: Bearer <token>`。
    pub async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        bearer: Option<&str>,
        body: RequestBody<'_>,
    ) -> Result<HttpResponse, DownloadError> {
        let url = self.source.rewrite(url);
        let mut request = match method {
            HttpMethod::Get => self.client.get(url.as_ref()),
            HttpMethod::Post => self.client.post(url.as_ref()),
        };

        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }

        request = match body {
            RequestBody::Empty => request,
            RequestBody::Form(fields) => request.form(fields),
            RequestBody::Json(value) => request.json(value),
        };

        let response = request
            .send()
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|error| DownloadError::Network(error.to_string()))?;

        Ok(HttpResponse { status, body })
    }
}
