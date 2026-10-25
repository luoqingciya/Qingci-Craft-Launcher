//! 下载执行器的测试。
//!
//! 全部使用假实现：失败次数、耗时、并发峰值都可精确控制，
//! 因此断言是确定性的，不依赖真实网络与真实磁盘。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use qcl_net::{
    DownloadError, DownloadOps, DownloadOptions, DownloadPlan, DownloadTask, Progress, RetryPolicy,
    execute_plan,
};

fn task(path: &str, size: u64) -> DownloadTask {
    DownloadTask {
        url: format!("https://example.invalid/{path}"),
        path: path.to_owned(),
        sha1: "0".repeat(40),
        size,
    }
}

fn plan_of(tasks: Vec<DownloadTask>) -> DownloadPlan {
    DownloadPlan { tasks }
}

/// 可编程的假实现：能指定哪些文件已存在、哪些先失败几次、每次尝试耗时多少。
#[derive(Default)]
struct FakeOps {
    present: HashSet<String>,
    /// 前 N 次尝试返回可重试错误。
    transient: Mutex<HashMap<String, usize>>,
    /// 始终返回不可重试错误。
    fatal: HashSet<String>,
    delay: Duration,
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    attempts: Mutex<HashMap<String, usize>>,
}

impl FakeOps {
    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn attempt_count(&self, path: &str) -> usize {
        self.attempts
            .lock()
            .expect("尝试计数锁被污染")
            .get(path)
            .copied()
            .unwrap_or(0)
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DownloadOps for FakeOps {
    async fn is_present(&self, task: &DownloadTask) -> bool {
        self.present.contains(&task.path)
    }

    async fn fetch_once(&self, task: &DownloadTask) -> Result<(), DownloadError> {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(current, Ordering::SeqCst);

        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }

        *self
            .attempts
            .lock()
            .expect("尝试计数锁被污染")
            .entry(task.path.clone())
            .or_insert(0) += 1;

        let result = if self.fatal.contains(&task.path) {
            Err(DownloadError::Other("HTTP 404".to_owned()))
        } else {
            let mut remaining = self.transient.lock().expect("失败注入锁被污染");
            match remaining.get_mut(&task.path) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    Err(DownloadError::Network("连接超时".to_owned()))
                }
                _ => Ok(()),
            }
        };

        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

fn fast_retry(attempts: u32) -> RetryPolicy {
    RetryPolicy::new(attempts, Duration::from_millis(1), Duration::from_millis(5))
}

/// 把具体类型的 Arc 转成 trait object，同时保留原 Arc 以便断言调用次数与并发峰值。
fn as_ops(ops: &Arc<FakeOps>) -> Arc<dyn DownloadOps> {
    let concrete: Arc<FakeOps> = Arc::clone(ops);
    concrete
}

fn collect_progress() -> (
    Arc<Mutex<Vec<Progress>>>,
    impl Fn(Progress) + Send + Sync + 'static,
) {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let handle = Arc::clone(&sink);
    let callback = move |progress: Progress| {
        handle.lock().expect("进度锁被污染").push(progress);
    };
    (sink, callback)
}

#[tokio::test]
async fn already_valid_files_are_skipped() {
    let mut ops = FakeOps::default();
    ops.present.insert("a.jar".to_owned());
    ops.present.insert("b.jar".to_owned());

    let plan = plan_of(vec![task("a.jar", 100), task("b.jar", 200)]);
    let (progress, callback) = collect_progress();

    let report = execute_plan(&plan, &DownloadOptions::new(4), Arc::new(ops), callback).await;

    assert!(report.is_success());
    assert_eq!(report.skipped, 2);
    assert_eq!(report.downloaded, 0);

    let snapshots = progress.lock().expect("进度锁被污染");
    let last = snapshots.last().expect("应有进度回调");
    assert!(last.is_finished());
    assert_eq!(last.completed_bytes, 300);
}

#[tokio::test]
async fn missing_files_are_downloaded() {
    let ops = FakeOps::default();
    let plan = plan_of(vec![
        task("a.jar", 10),
        task("b.jar", 20),
        task("c.jar", 30),
    ]);

    let report = execute_plan(&plan, &DownloadOptions::new(2), Arc::new(ops), |_| {}).await;

    assert!(report.is_success());
    assert_eq!(report.downloaded, 3);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.handled(), 3);
}

#[tokio::test]
async fn transient_failures_are_retried_until_success() {
    let ops = FakeOps::default();
    ops.transient
        .lock()
        .expect("失败注入锁被污染")
        .insert("a.jar".to_owned(), 2);

    let plan = plan_of(vec![task("a.jar", 10)]);
    let options = DownloadOptions::new(1).with_retry(fast_retry(3));

    let report = execute_plan(&plan, &options, Arc::new(ops), |_| {}).await;

    assert!(report.is_success(), "第 3 次应成功");
    assert_eq!(report.downloaded, 1);
}

#[tokio::test]
async fn retries_stop_when_attempts_are_exhausted() {
    let ops = FakeOps::default();
    ops.transient
        .lock()
        .expect("失败注入锁被污染")
        .insert("a.jar".to_owned(), 10);

    let ops = Arc::new(ops);
    let plan = plan_of(vec![task("a.jar", 10)]);
    let options = DownloadOptions::new(1).with_retry(fast_retry(2));

    let report = execute_plan(&plan, &options, as_ops(&ops), |_| {}).await;

    assert!(!report.is_success());
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, "a.jar");
    assert_eq!(
        ops.attempt_count("a.jar"),
        2,
        "max_attempts = 2 表示最多尝试 2 次，不能多试"
    );
}

#[tokio::test]
async fn fatal_errors_are_not_retried() {
    let mut ops = FakeOps::default();
    ops.fatal.insert("missing.jar".to_owned());

    let ops = Arc::new(ops);
    let plan = plan_of(vec![task("missing.jar", 10)]);
    let options = DownloadOptions::new(1).with_retry(fast_retry(5));

    let report = execute_plan(&plan, &options, as_ops(&ops), |_| {}).await;

    assert!(!report.is_success());
    assert_eq!(
        ops.attempt_count("missing.jar"),
        1,
        "404 之类的错误重试没有意义，只应尝试一次"
    );
}

#[tokio::test]
async fn concurrency_never_exceeds_the_limit() {
    let ops = FakeOps::default().with_delay(Duration::from_millis(5));
    let ops = Arc::new(ops);

    let tasks: Vec<DownloadTask> = (0..10)
        .map(|index| task(&format!("file-{index}.jar"), 10))
        .collect();
    let plan = plan_of(tasks);

    let report = execute_plan(&plan, &DownloadOptions::new(3), as_ops(&ops), |_| {}).await;

    assert!(report.is_success());
    assert_eq!(report.downloaded, 10);
    assert_eq!(
        ops.peak(),
        3,
        "并发峰值应恰好等于上限：既不能超，也要真的跑满"
    );
}

#[tokio::test]
async fn progress_snapshots_are_monotonic() {
    let ops = FakeOps::default().with_delay(Duration::from_millis(2));
    let plan = plan_of(
        (0..6)
            .map(|index| task(&format!("file-{index}.jar"), 10))
            .collect(),
    );
    let (progress, callback) = collect_progress();

    let report = execute_plan(&plan, &DownloadOptions::new(3), Arc::new(ops), callback).await;
    assert!(report.is_success());

    let snapshots = progress.lock().expect("进度锁被污染");
    assert!(!snapshots.is_empty());
    assert!(
        snapshots
            .windows(2)
            .all(|pair| pair[0].completed_files <= pair[1].completed_files
                && pair[0].completed_bytes <= pair[1].completed_bytes),
        "进度必须单调递增，否则界面会出现回退"
    );

    let last = snapshots.last().expect("应有进度回调");
    assert!(last.is_finished());
    assert_eq!(last.completed_files, 6);
    assert_eq!(last.total_bytes, 60);
    assert!((last.ratio() - 1.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn empty_plan_finishes_immediately() {
    let (progress, callback) = collect_progress();
    let report = execute_plan(
        &DownloadPlan::default(),
        &DownloadOptions::new(4),
        Arc::new(FakeOps::default()),
        callback,
    )
    .await;

    assert!(report.is_success());
    assert_eq!(report.handled(), 0);

    let snapshots = progress.lock().expect("进度锁被污染");
    assert_eq!(snapshots.len(), 1, "空计划也应回调一次，便于界面收尾");
    assert!(snapshots[0].is_finished());
    assert!((snapshots[0].ratio() - 1.0).abs() < f32::EPSILON);
}
