//! 启动游戏进程并把日志逐行转发出来。
//!
//! 这一层刻意保持薄：命令行由 `qcl-core` 组装好，这里只负责「按数组传参、起进程、
//! 读日志、等退出、必要时杀掉」。把进程管理单独成 crate 而不是塞进 `qcl-core`，
//! 是为了让核心逻辑保持无 IO、可在任意平台完整测试。

use std::future::Future;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use qcl_core::launch::LaunchCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// 日志来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    /// 标准输出。
    Stdout,
    /// 标准错误。Minecraft 的大部分运行日志走这里。
    Stderr,
}

/// 一行日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// 来源流。
    pub stream: LogStream,
    /// 行内容（不含换行符）。
    pub text: String,
}

/// 进程结束后的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOutcome {
    /// 退出码。被信号终止时为 `None`。
    pub exit_code: Option<i32>,
    /// 从启动到退出的耗时。
    pub duration: Duration,
}

impl LaunchOutcome {
    /// 是否正常退出（退出码为 0）。
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// 进程管理错误。
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    /// 无法启动进程（可执行文件不存在、权限不足等）。
    #[error("无法启动游戏进程：{0}")]
    Spawn(#[source] std::io::Error),
}

/// 一个正在运行的游戏进程。
pub struct GameProcess {
    child: Child,
    pid: Option<u32>,
    logs: mpsc::Receiver<LogLine>,
}

impl std::fmt::Debug for GameProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不打印子进程句柄：它没有稳定表示，日志里只会是噪音。
        f.debug_struct("GameProcess")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl GameProcess {
    /// 进程 ID。用于日志展示与「结束进程」。
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// 强制结束进程。
    pub async fn kill(&mut self) -> Result<(), LaunchError> {
        self.child.kill().await.map_err(LaunchError::Spawn)
    }

    /// 等待进程结束，期间把日志逐行交给回调。
    ///
    /// 用 `select!` 同时推进「读日志」与「等退出」：如果先等退出再读日志，
    /// 管道缓冲区写满后子进程会被阻塞，表现为游戏卡死。
    pub async fn wait<F>(&mut self, mut on_line: F) -> Result<LaunchOutcome, LaunchError>
    where
        F: FnMut(LogLine),
    {
        let started = Instant::now();

        let status = loop {
            tokio::select! {
                line = self.logs.recv() => {
                    if let Some(line) = line {
                        on_line(line);
                    }
                }
                status = self.child.wait() => {
                    break status.map_err(LaunchError::Spawn)?;
                }
            }
        };

        // 进程已退出，但管道里可能还有未读内容；读取端在 EOF 后会自行结束。
        //
        // 这里必须设时间上限：如果游戏派生了子进程并让它继承了 stdout/stderr
        // （或者进程被 kill 而孙进程还活着），写端不会关闭，EOF 永远等不到，
        // 收尾就会永久阻塞。宁可丢掉最后几行日志，也不能让「等待退出」卡死。
        let drain = async {
            while let Some(line) = self.logs.recv().await {
                on_line(line);
            }
        };
        let _ = tokio::time::timeout(LOG_DRAIN_TIMEOUT, drain).await;

        Ok(LaunchOutcome {
            exit_code: status.code(),
            duration: started.elapsed(),
        })
    }

    /// 等待进程结束，但允许外部取消。
    ///
    /// `cancel` 完成时终止游戏进程，然后照常收尾并返回结果（退出码为终止后的实际值，
    /// 通常非 0）。用于界面的「停止」按钮与终端的 Ctrl+C。
    ///
    /// 为什么必须有这个入口：不处理取消的话，调用方进程退出时子进程会成为孤儿——
    /// Windows 上子进程被 `CREATE_NO_WINDOW` 放进了独立控制台，终端的 Ctrl+C
    /// 传不到它，用户会看到「启动器关了但游戏还在跑」。
    pub async fn wait_or_cancel<F, C>(
        &mut self,
        cancel: C,
        mut on_line: F,
    ) -> Result<LaunchOutcome, LaunchError>
    where
        F: FnMut(LogLine),
        C: Future<Output = ()>,
    {
        enum Step {
            Done(Result<LaunchOutcome, LaunchError>),
            Cancel,
        }

        let step = tokio::select! {
            outcome = self.wait(&mut on_line) => Step::Done(outcome),
            () = cancel => Step::Cancel,
        };

        match step {
            Step::Done(outcome) => outcome,
            Step::Cancel => {
                self.kill().await?;
                // 终止之后仍要走完 `wait`：日志通道里可能还有最后几行，
                // 退出码也需要从子进程取回，而不是假定成功或失败。
                self.wait(&mut on_line).await
            }
        }
    }
}

/// 启动游戏进程。
///
/// `working_dir` 会成为进程的工作目录——游戏按相对路径找 `assets`、`mods` 等目录，
/// 工作目录错了会表现为「资源全部缺失」。
pub fn spawn(command: &LaunchCommand, working_dir: &Path) -> Result<GameProcess, LaunchError> {
    let mut builder = Command::new(&command.executable);
    builder
        .args(&command.args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Windows 上如果启动的是 java.exe（而非 javaw.exe），会额外弹出一个控制台窗口。
    // 这里统一隐藏，避免用户看到两个窗口。
    // tokio 的 Command 在 Windows 上自带 creation_flags，无需引入 std 的 CommandExt。
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        builder.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = builder.spawn().map_err(LaunchError::Spawn)?;
    let pid = child.id();

    // 两路日志汇入同一通道：调用方不需要分别处理，也天然保持相对顺序。
    let (sender, logs) = mpsc::channel(LOG_CHANNEL_CAPACITY);
    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, LogStream::Stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, LogStream::Stderr, sender);
    }

    Ok(GameProcess { child, pid, logs })
}

/// 日志通道容量。设上限而不是无限缓冲：游戏刷日志的速度可以远超消费速度，
/// 无界通道会把内存吃光。
const LOG_CHANNEL_CAPACITY: usize = 4096;

/// 进程退出后等待剩余日志的最长时间。
const LOG_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

fn spawn_reader<R>(reader: R, stream: LogStream, sender: mpsc::Sender<LogLine>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(text)) => {
                    if sender.send(LogLine { stream, text }).await.is_err() {
                        // 接收端已关闭，无需继续读取。
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
}
