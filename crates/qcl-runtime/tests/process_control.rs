//! 进程层的测试。
//!
//! 用系统自带的 shell 作为被测进程，因此不依赖本机是否装了 Java，
//! 且能在三个平台上真实执行。

use std::path::PathBuf;
use std::time::Duration;

use qcl_core::launch::LaunchCommand;
use qcl_runtime::{LogLine, LogStream, spawn};
use tempfile::TempDir;

/// 构造一条在当前平台上执行 shell 命令的启动命令。
fn shell(script: &str) -> LaunchCommand {
    #[cfg(windows)]
    {
        LaunchCommand {
            executable: PathBuf::from("cmd.exe"),
            args: vec!["/C".to_owned(), script.to_owned()],
        }
    }
    #[cfg(not(windows))]
    {
        LaunchCommand {
            executable: PathBuf::from("sh"),
            args: vec!["-c".to_owned(), script.to_owned()],
        }
    }
}

fn temp_dir() -> TempDir {
    TempDir::new().expect("应能创建临时目录")
}

#[tokio::test]
async fn captures_stdout_and_exit_code() {
    let dir = temp_dir();
    let mut process = spawn(&shell("echo hello-qcl"), dir.path()).expect("应能启动");
    assert!(process.pid().is_some(), "应能拿到进程 ID");

    let mut lines: Vec<LogLine> = Vec::new();
    let outcome = process
        .wait(|line| lines.push(line))
        .await
        .expect("应能等待");

    assert!(outcome.is_success());
    assert_eq!(outcome.exit_code, Some(0));
    assert!(
        lines
            .iter()
            .any(|line| line.stream == LogStream::Stdout && line.text.contains("hello-qcl")),
        "应捕获到标准输出：{lines:?}"
    );
}

#[tokio::test]
async fn reports_non_zero_exit_code() {
    let dir = temp_dir();
    let mut process = spawn(&shell("exit 3"), dir.path()).expect("应能启动");
    let outcome = process.wait(|_| {}).await.expect("应能等待");

    assert_eq!(outcome.exit_code, Some(3));
    assert!(!outcome.is_success(), "非零退出码不应判为成功");
}

#[tokio::test]
async fn separates_stderr_from_stdout() {
    let dir = temp_dir();
    let mut process = spawn(&shell("echo to-stderr 1>&2"), dir.path()).expect("应能启动");

    let mut lines: Vec<LogLine> = Vec::new();
    process
        .wait(|line| lines.push(line))
        .await
        .expect("应能等待");

    assert!(
        lines
            .iter()
            .any(|line| line.stream == LogStream::Stderr && line.text.contains("to-stderr")),
        "标准错误应被单独标记：{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.stream == LogStream::Stdout && line.text.contains("to-stderr")),
        "不应把 stderr 混进 stdout"
    );
}

#[tokio::test]
async fn working_directory_is_applied() {
    let dir = temp_dir();
    // 输出当前工作目录，断言它就是我们传入的目录。
    let command = shell("cd");

    let mut process = spawn(&command, dir.path()).expect("应能启动");
    let mut lines: Vec<LogLine> = Vec::new();
    process
        .wait(|line| lines.push(line))
        .await
        .expect("应能等待");

    let marker = dir
        .path()
        .file_name()
        .expect("临时目录应有名字")
        .to_string_lossy()
        .into_owned();

    assert!(
        lines.iter().any(|line| line.text.contains(&marker)),
        "工作目录应为 {:?}，实际输出：{lines:?}",
        dir.path()
    );
}

#[tokio::test]
async fn kill_terminates_a_long_running_process() {
    #[cfg(windows)]
    let command = shell("ping -n 30 127.0.0.1 > nul");
    #[cfg(not(windows))]
    let command = shell("sleep 30");

    let dir = temp_dir();
    let mut process = spawn(&command, dir.path()).expect("应能启动");

    process.kill().await.expect("应能终止");

    let outcome = tokio::time::timeout(Duration::from_secs(10), process.wait(|_| {}))
        .await
        .expect("终止后应迅速退出，而不是等满 30 秒")
        .expect("等待应成功");

    assert!(
        !outcome.is_success(),
        "被终止的进程不应报告成功退出：{outcome:?}"
    );
}

#[tokio::test]
async fn missing_executable_is_reported() {
    let dir = temp_dir();
    let command = LaunchCommand {
        executable: PathBuf::from("qcl-definitely-not-a-real-binary"),
        args: Vec::new(),
    };

    let error = spawn(&command, dir.path()).expect_err("不存在的可执行文件应失败");
    let message = error.to_string();
    assert!(
        message.contains("无法启动游戏进程"),
        "错误信息应说明启动失败：{message}"
    );
}
