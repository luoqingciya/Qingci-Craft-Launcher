// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 下载执行器：并发调度、重试与进度上报。
//!
//! 网络与文件操作通过 [`DownloadOps`] 注入，执行器只负责调度判断。这样可以在测试中
//! 精确控制「第几次失败」「并发峰值」，从而确定性验证并发上限、重试耗尽、进度单调等行为，
//! 而不依赖真实网络。

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::plan::{DownloadPlan, DownloadTask};
use crate::retry::RetryPolicy;

/// 下载过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// 网络层失败，值得重试。
    #[error("网络错误：{0}")]
    Network(String),
    /// 下载完成但 SHA-1 不匹配，同样值得重试——截断与代理注入都能通过重下修复。
    #[error("校验失败：{0}")]
    Checksum(String),
    /// 文件系统错误，重试无意义。
    #[error("文件系统错误：{0}")]
    Io(String),
    /// 计划中的落地路径越出游戏目录，属于非法输入。
    #[error("非法的落地路径：{0}")]
    UnsafePath(String),
    /// 其他不可重试错误（如 404）。
    #[error("{0}")]
    Other(String),
}

impl DownloadError {
    /// 该错误是否值得重试。
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Checksum(_))
    }
}

/// 下载进度快照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// 已处理文件数（含跳过），分母为 [`Progress::total_files`]。
    pub completed_files: usize,
    /// 计划中的文件总数。
    pub total_files: usize,
    /// 已处理字节数（含跳过）。
    pub completed_bytes: u64,
    /// 计划总字节数。
    pub total_bytes: u64,
}

impl Progress {
    /// 是否全部处理完毕。
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.completed_files >= self.total_files
    }

    /// 完成比例，范围 `0.0..=1.0`。
    #[must_use]
    pub fn ratio(&self) -> f32 {
        if self.total_bytes == 0 {
            return if self.is_finished() { 1.0 } else { 0.0 };
        }
        (self.completed_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32
    }
}

/// 执行参数。
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// 并发上限。默认 16：官方 CDN 对单连接限速，适度并发能显著提升总吞吐。
    pub concurrency: usize,
    /// 重试策略。
    pub retry: RetryPolicy,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: 16,
            retry: RetryPolicy::default(),
        }
    }
}

impl DownloadOptions {
    /// 指定并发数，其余取默认值。
    #[must_use]
    pub fn new(concurrency: usize) -> Self {
        Self {
            concurrency,
            ..Self::default()
        }
    }

    /// 替换重试策略。
    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }
}

/// 单个文件的失败记录。
#[derive(Debug)]
pub struct DownloadFailure {
    /// 失败文件相对游戏目录的路径。
    pub path: String,
    /// 具体错误。
    pub error: DownloadError,
}

/// 执行结果。
#[derive(Debug, Default)]
pub struct DownloadReport {
    /// 实际下载的文件数。
    pub downloaded: usize,
    /// 已存在且校验通过、被跳过的文件数。
    pub skipped: usize,
    /// 失败列表。空表示全部成功。
    pub failures: Vec<DownloadFailure>,
}

impl DownloadReport {
    /// 是否全部成功。
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }

    /// 处理成功的文件总数（下载 + 跳过）。
    #[must_use]
    pub fn handled(&self) -> usize {
        self.downloaded + self.skipped
    }
}

/// 执行器依赖的底层操作。
///
/// 把「文件是否已就绪」与「下载一次」两个动作抽象出来，
/// 真实实现见后续的 HTTP/文件系统版本，测试中替换为可控的假实现。
#[async_trait]
pub trait DownloadOps: Send + Sync {
    /// 目标文件是否已存在且内容校验通过。
    async fn is_present(&self, task: &DownloadTask) -> bool;

    /// 执行一次下载尝试（含落盘与校验）。
    async fn fetch_once(&self, task: &DownloadTask) -> Result<(), DownloadError>;
}

/// 执行下载计划。
///
/// 并发模型是**固定数量 worker 抢占任务队列**，而非为每个任务起一个协程再用信号量限流：
/// 后者会在任务数远大于并发数时堆积大量挂起协程，前者内存占用恒定。
///
/// 进度回调在计数器更新后同步触发，并用互斥锁串行化，保证调用方看到的是单调递增序列。
pub async fn execute_plan<F>(
    plan: &DownloadPlan,
    options: &DownloadOptions,
    ops: Arc<dyn DownloadOps>,
    on_progress: F,
) -> DownloadReport
where
    F: Fn(Progress) + Send + Sync + 'static,
{
    let state = Arc::new(SharedState {
        downloaded: AtomicUsize::new(0),
        skipped: AtomicUsize::new(0),
        completed_bytes: AtomicU64::new(0),
        total_files: plan.len(),
        total_bytes: plan.total_bytes(),
        failures: Mutex::new(Vec::new()),
        on_progress: Mutex::new(on_progress),
    });

    if plan.is_empty() {
        state.emit();
        return DownloadReport::default();
    }

    let tasks = Arc::new(plan.tasks.clone());
    let next_index = Arc::new(AtomicUsize::new(0));
    let worker_count = options.concurrency.max(1).min(tasks.len());

    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let tasks = Arc::clone(&tasks);
        let next_index = Arc::clone(&next_index);
        let state = Arc::clone(&state);
        let ops = Arc::clone(&ops);
        let retry = options.retry.clone();

        workers.push(tokio::spawn(async move {
            loop {
                let index = next_index.fetch_add(1, Ordering::Relaxed);
                let Some(task) = tasks.get(index) else {
                    break;
                };
                process_task(task, ops.as_ref(), &retry, &state).await;
            }
        }));
    }

    for worker in workers {
        // worker 内部已捕获全部错误，join 失败只可能是 panic，此处不掩盖。
        let _ = worker.await;
    }

    let failures = std::mem::take(&mut *state.failures.lock().expect("失败列表锁被污染"));
    DownloadReport {
        downloaded: state.downloaded.load(Ordering::Relaxed),
        skipped: state.skipped.load(Ordering::Relaxed),
        failures,
    }
}

struct SharedState<F> {
    downloaded: AtomicUsize,
    skipped: AtomicUsize,
    completed_bytes: AtomicU64,
    total_files: usize,
    total_bytes: u64,
    failures: Mutex<Vec<DownloadFailure>>,
    on_progress: Mutex<F>,
}

impl<F> SharedState<F>
where
    F: Fn(Progress),
{
    fn emit(&self) {
        let progress = Progress {
            completed_files: self.downloaded.load(Ordering::Relaxed)
                + self.skipped.load(Ordering::Relaxed),
            total_files: self.total_files,
            completed_bytes: self.completed_bytes.load(Ordering::Relaxed),
            total_bytes: self.total_bytes,
        };
        let callback = self.on_progress.lock().expect("进度回调锁被污染");
        callback(progress);
    }
}

async fn process_task(
    task: &DownloadTask,
    ops: &dyn DownloadOps,
    retry: &RetryPolicy,
    state: &SharedState<impl Fn(Progress)>,
) {
    if ops.is_present(task).await {
        state.skipped.fetch_add(1, Ordering::Relaxed);
        state
            .completed_bytes
            .fetch_add(task.size, Ordering::Relaxed);
        state.emit();
        return;
    }

    match download_with_retry(task, ops, retry).await {
        Ok(()) => {
            state.downloaded.fetch_add(1, Ordering::Relaxed);
            state
                .completed_bytes
                .fetch_add(task.size, Ordering::Relaxed);
            state.emit();
        }
        Err(error) => {
            state
                .failures
                .lock()
                .expect("失败列表锁被污染")
                .push(DownloadFailure {
                    path: task.path.clone(),
                    error,
                });
        }
    }
}

async fn download_with_retry(
    task: &DownloadTask,
    ops: &dyn DownloadOps,
    retry: &RetryPolicy,
) -> Result<(), DownloadError> {
    let mut attempt: u32 = 1;
    loop {
        match ops.fetch_once(task).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                if !error.is_retryable() || !retry.should_retry(attempt) {
                    return Err(error);
                }
                let delay = retry.delay_for(attempt).unwrap_or_default();
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                attempt += 1;
            }
        }
    }
}
