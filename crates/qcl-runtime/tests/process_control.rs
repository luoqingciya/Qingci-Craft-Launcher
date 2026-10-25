// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

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

    // 打印当前工作目录的命令在两个平台上不同：
    // Windows 的 `cd` 无参数时打印路径，Unix 的 `cd` 只切换目录、不输出任何东西。
    #[cfg(windows)]
    let command = shell("cd");
    #[cfg(not(windows))]
    let command = shell("pwd");

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
    // 用 exec 让 shell 直接被目标进程替换，这样 kill 打到的就是真正在等的那个进程。
    #[cfg(windows)]
    let command = shell("ping -n 30 127.0.0.1 > nul");
    #[cfg(not(windows))]
    let command = shell("exec sleep 30");

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
async fn wait_does_not_hang_when_a_grandchild_holds_the_pipes() {
    // 不带 exec：shell 会派生子进程，子进程继承 stdout/stderr 的写端。
    // 杀掉 shell 后写端仍然打开，EOF 永远等不到——收尾阶段必须有时间上限，
    // 否则「等待退出」会永久阻塞。
    #[cfg(windows)]
    let command = shell("ping -n 30 127.0.0.1 > nul");
    #[cfg(not(windows))]
    let command = shell("sleep 30");

    let dir = temp_dir();
    let mut process = spawn(&command, dir.path()).expect("应能启动");

    process.kill().await.expect("应能终止");

    let outcome = tokio::time::timeout(Duration::from_secs(10), process.wait(|_| {}))
        .await
        .expect("即使孙进程还持有管道，wait 也必须有界返回")
        .expect("等待应成功");

    assert!(!outcome.is_success());
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

#[tokio::test]
async fn cancel_terminates_a_long_running_process() {
    #[cfg(windows)]
    let command = shell("ping -n 30 127.0.0.1 > nul");
    #[cfg(not(windows))]
    let command = shell("sleep 30");

    let dir = temp_dir();
    let mut process = spawn(&command, dir.path()).expect("应能启动");

    // 模拟「用户点了停止 / 按了 Ctrl+C」。
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        process.wait_or_cancel(cancel, |_| {}),
    )
    .await
    .expect("取消后应迅速返回，而不是等满 30 秒")
    .expect("等待应成功");

    assert!(!outcome.is_success(), "被取消的进程不应报告成功退出");
}

#[tokio::test]
async fn cancel_that_never_fires_behaves_like_wait() {
    let dir = temp_dir();
    let mut process = spawn(&shell("echo done"), dir.path()).expect("应能启动");

    let mut lines = Vec::new();
    let outcome = process
        .wait_or_cancel(std::future::pending(), |line| lines.push(line))
        .await
        .expect("应能等待");

    assert!(outcome.is_success());
    assert!(
        lines.iter().any(|line| line.text.contains("done")),
        "不取消时日志仍应正常转发"
    );
}
