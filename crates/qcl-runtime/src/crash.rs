// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 崩溃报告（`crash-reports/crash-*.txt`）的定位与摘要。
//!
//! 为什么需要：游戏真正崩溃时会把完整堆栈、模组清单、系统信息写进
//! `crash-reports/`，比 stdout 上转发的那些行完整得多；而 `Description:` 那一行
//! 往往就是唯一的人话结论。启动器不读它，用户就得自己进目录翻文件。
//!
//! 只依赖 `std`：定位与解析都是纯逻辑，可在无 GUI 的 CI 里逐条测。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 最多读文件开头这么多字节。
///
/// 崩溃报告动辄几百 KB，末尾是完整模组清单与系统信息；有用部分全在头部。
/// 读整份既慢又没意义。
const MAX_BYTES: u64 = 64 * 1024;

/// 摘要最多保留几行——再多就不叫摘要了。
const MAX_EXCERPT: usize = 12;

/// 时间戳容差。
///
/// 文件系统时间戳粒度可能只有一秒（某些 FAT/exFAT 甚至两秒），而游戏是在退出前
/// 一瞬间写报告。不给容差，刚崩完的报告会被判成「上一局的」而漏掉。
const CLOCK_SLACK: Duration = Duration::from_secs(2);

/// 一份崩溃报告里值得看的部分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    /// 报告文件路径。
    pub path: PathBuf,
    /// `Description:` 那一行（没有则为 `None`）。
    pub description: Option<String>,
    /// 结论下方紧接着的若干行（异常类型 + 顶部堆栈帧）。
    pub excerpt: Vec<String>,
}

impl CrashReport {
    /// 拼成可复制的纯文本。
    pub fn to_text(&self) -> String {
        let mut text = match &self.description {
            Some(description) => format!("游戏崩溃：{description}"),
            None => "游戏崩溃（报告里没有 Description）".to_owned(),
        };
        if !self.excerpt.is_empty() {
            text.push('\n');
            text.push('\n');
            text.push_str(&self.excerpt.join("\n"));
        }
        text
    }
}

/// 报告目录。
pub fn directory(game_dir: &Path) -> PathBuf {
    game_dir.join("crash-reports")
}

/// 找 `since` 之后生成的最新一份崩溃报告。
///
/// 只认 `since` 之后的文件：上一局的旧报告还在目录里，把它的结论贴到这一次崩溃上
/// 比什么都不显示更糟——用户会去修一个根本不存在的问题。
pub fn find_since(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    let cutoff = since.checked_sub(CLOCK_SLACK).unwrap_or(since);

    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(directory(game_dir)).ok()? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !is_report(&path) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if modified < cutoff {
            continue;
        }
        // 同一秒内可能连写两份（客户端 + 服务端），用文件名兜底保证结果确定：
        // 游戏自己的命名带时间戳，字典序即时间序。
        let better = match &best {
            None => true,
            Some((best_time, best_path)) => (modified, &path) > (*best_time, best_path),
        };
        if better {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

/// 文件名形如 `crash-2026-10-25_12.34.56-client.txt`。
fn is_report(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("crash-") && name.ends_with(".txt"))
}

/// 读取并解析一份报告。
///
/// 读失败、或文件不是文本，都返回 `None`：崩溃摘要只是锦上添花，
/// 不能因为读不到报告就让「游戏退出了」这个事实都报不出来。
pub fn read(path: &Path) -> Option<CrashReport> {
    let mut buffer = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_BYTES)
        .read_to_end(&mut buffer)
        .ok()?;

    // 截断可能切在多字节字符中间，用 lossy 而不是 UTF-8 严格校验。
    let text = String::from_utf8_lossy(&buffer);
    let (description, excerpt) = parse(&text);
    if description.is_none() && excerpt.is_empty() {
        return None;
    }

    Some(CrashReport {
        path: path.to_path_buf(),
        description,
        excerpt,
    })
}

/// 从报告正文里抽出结论与紧随其后的几行。
///
/// 官方格式：
///
/// ```text
/// ---- Minecraft Crash Report ----
/// Time: 2026-10-25 12:34:56
/// Description: Rendering overlay
///
/// java.lang.NullPointerException: Cannot invoke ...
///     at net.minecraft.client.renderer...
/// -- Head --
/// ...
/// ```
///
/// `Description` 之后、第一个 `-- 分节 --` 之前的那一段，正是「为什么崩」。
pub fn parse(text: &str) -> (Option<String>, Vec<String>) {
    let mut description: Option<String> = None;
    let mut excerpt: Vec<String> = Vec::new();
    // 没找到 Description 时（少数崩溃不写），退化成「第一段异常行」。
    let mut fallback: Vec<String> = Vec::new();
    let mut collecting_fallback = false;

    for raw in text.lines() {
        let line = raw.trim_end();

        if description.is_none() {
            if let Some(rest) = line.trim().strip_prefix("Description:") {
                let value = rest.trim();
                if !value.is_empty() {
                    description = Some(value.to_owned());
                }
                continue;
            }
            if !collecting_fallback && looks_like_exception(line) {
                collecting_fallback = true;
            }
            if collecting_fallback {
                let trimmed = line.trim();
                if !trimmed.is_empty() && fallback.len() < MAX_EXCERPT {
                    fallback.push(trimmed.to_owned());
                }
            }
            continue;
        }

        // 分节标题之后是「Affected level」「System Details」「模组清单」，
        // 与原因无关，看到就停。
        if is_section_header(line) {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        excerpt.push(trimmed.to_owned());
        if excerpt.len() >= MAX_EXCERPT {
            break;
        }
    }

    if description.is_some() {
        (description, excerpt)
    } else {
        (None, fallback)
    }
}

/// `-- Head --` / `-- System Details --` 这类分节标题。
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() > 4 && trimmed.starts_with("--") && trimmed.ends_with("--")
}

/// 形如 `java.lang.NullPointerException: ...` 或 `Exception in thread ...`。
fn looks_like_exception(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("Exception in thread") {
        return true;
    }
    let head = trimmed.split(':').next().unwrap_or("");
    head.contains('.') && (head.ends_with("Exception") || head.ends_with("Error"))
}

#[cfg(test)]
mod tests {
    use super::{MAX_EXCERPT, find_since, parse, read};
    use std::fs::{File, create_dir_all, write};
    use std::time::{Duration, SystemTime};

    const SAMPLE: &str = "\
---- Minecraft Crash Report ----
// Don't be sad, have a hug! <3

Time: 2026-10-25 12:34:56
Description: Rendering overlay

java.lang.NullPointerException: Cannot invoke \"Foo.bar()\" because \"foo\" is null
\tat net.minecraft.client.renderer.LevelRenderer.render(LevelRenderer.java:123)
\tat net.minecraft.client.Minecraft.run(Minecraft.java:456)
-- Head --
Thread: Render thread
Stacktrace:
\tat net.minecraft.client.Minecraft.run(Minecraft.java:456)
-- System Details --
Minecraft Version: 26.3
Fabric Mods:
\tfabric-api: 0.1.0
";

    #[test]
    fn description_is_extracted() {
        let (description, _) = parse(SAMPLE);
        assert_eq!(description.as_deref(), Some("Rendering overlay"));
    }

    #[test]
    fn excerpt_stops_at_section_header() {
        let (_, excerpt) = parse(SAMPLE);
        assert_eq!(excerpt.len(), 3);
        assert!(excerpt[0].starts_with("java.lang.NullPointerException"));
        // 分节标题之后的内容不能混进来。
        assert!(
            !excerpt
                .iter()
                .any(|line| line.contains("Minecraft Version"))
        );
        assert!(!excerpt.iter().any(|line| line.contains("fabric-api")));
    }

    #[test]
    fn blank_lines_are_skipped() {
        let (_, excerpt) = parse(SAMPLE);
        assert!(excerpt.iter().all(|line| !line.trim().is_empty()));
    }

    #[test]
    fn excerpt_is_capped() {
        let mut text = String::from("Description: 崩了\n");
        for index in 0..50 {
            text.push_str(&format!("\tat frame{index}\n"));
        }
        let (_, excerpt) = parse(&text);
        assert_eq!(excerpt.len(), MAX_EXCERPT);
    }

    #[test]
    fn missing_description_falls_back_to_exception() {
        // 少数崩溃没有 Description 行，此时不能返回空摘要。
        let text = "\
---- Minecraft Crash Report ----
Time: 2026-10-25 12:34:56

java.lang.OutOfMemoryError: Java heap space
\tat net.minecraft.Foo.bar(Foo.java:1)
-- Head --
";
        let (description, excerpt) = parse(text);
        assert!(description.is_none());
        assert_eq!(excerpt[0], "java.lang.OutOfMemoryError: Java heap space");
        assert!(excerpt[1].contains("net.minecraft.Foo.bar"));
    }

    #[test]
    fn unrelated_text_yields_nothing() {
        let (description, excerpt) = parse("just some text\nnothing to see\n");
        assert!(description.is_none());
        assert!(excerpt.is_empty());
    }

    #[test]
    fn finds_report_written_after_launch() {
        let dir = tempfile::tempdir().expect("临时目录");
        let reports = dir.path().join("crash-reports");
        create_dir_all(&reports).expect("建目录");
        let path = reports.join("crash-2026-10-25_12.34.56-client.txt");
        write(&path, SAMPLE).expect("写报告");

        let found = find_since(dir.path(), SystemTime::now() - Duration::from_secs(60));
        assert_eq!(found.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn ignores_report_older_than_launch() {
        // 上一局的旧报告不能拿来解释这一次的退出。
        let dir = tempfile::tempdir().expect("临时目录");
        let reports = dir.path().join("crash-reports");
        create_dir_all(&reports).expect("建目录");
        let path = reports.join("crash-2026-10-25_12.34.56-client.txt");
        write(&path, SAMPLE).expect("写报告");
        let old = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(old))
            .expect("改时间戳");

        assert!(find_since(dir.path(), SystemTime::now() - Duration::from_secs(60)).is_none());
    }

    #[test]
    fn ignores_other_files() {
        // `-server.txt` 是集成服务端线程崩溃时写的，同样算崩溃报告；
        // 真正要排除的是同目录里别的东西。
        let dir = tempfile::tempdir().expect("临时目录");
        let reports = dir.path().join("crash-reports");
        create_dir_all(&reports).expect("建目录");
        write(reports.join("notes.txt"), "x").expect("写文件");
        write(reports.join("crash-2026-10-25_12.34.56-client.md"), "x").expect("写文件");
        write(reports.join("latest.log"), "x").expect("写文件");

        assert!(find_since(dir.path(), SystemTime::now() - Duration::from_secs(60)).is_none());
    }

    #[test]
    fn accepts_server_side_report() {
        // 单人游戏里集成服务端崩溃也会写 `-server.txt`，它同样解释了这次退出。
        let dir = tempfile::tempdir().expect("临时目录");
        let reports = dir.path().join("crash-reports");
        create_dir_all(&reports).expect("建目录");
        let path = reports.join("crash-2026-10-25_12.34.56-server.txt");
        write(&path, SAMPLE).expect("写报告");

        let found = find_since(dir.path(), SystemTime::now() - Duration::from_secs(60));
        assert_eq!(found.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn picks_the_newest_of_several() {
        let dir = tempfile::tempdir().expect("临时目录");
        let reports = dir.path().join("crash-reports");
        create_dir_all(&reports).expect("建目录");
        let older = reports.join("crash-2026-10-25_12.00.00-client.txt");
        let newer = reports.join("crash-2026-10-25_13.00.00-client.txt");
        write(&older, SAMPLE).expect("写报告");
        write(&newer, SAMPLE).expect("写报告");

        let found = find_since(dir.path(), SystemTime::now() - Duration::from_secs(60));
        assert_eq!(found.as_deref(), Some(newer.as_path()));
    }

    #[test]
    fn missing_directory_is_not_an_error() {
        let dir = tempfile::tempdir().expect("临时目录");
        assert!(find_since(dir.path(), SystemTime::now()).is_none());
    }

    #[test]
    fn read_parses_file() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("crash-2026-10-25_12.34.56-client.txt");
        write(&path, SAMPLE).expect("写报告");

        let report = read(&path).expect("应能读出报告");
        assert_eq!(report.description.as_deref(), Some("Rendering overlay"));
        assert_eq!(report.path, path);
    }

    #[test]
    fn read_tolerates_invalid_utf8() {
        // 截断点可能落在多字节字符中间，不能让整份报告读不出来。
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("crash-2026-10-25_12.34.56-client.txt");
        let mut bytes = b"Description: \xe5\xa4\xb1\xe8\xb4\xa5\n".to_vec();
        bytes.push(0xFF);
        bytes.extend_from_slice(b"\njava.lang.Error: boom\n");
        write(&path, &bytes).expect("写报告");

        let report = read(&path).expect("应能读出报告");
        assert_eq!(report.description.as_deref(), Some("失败"));
    }

    #[test]
    fn read_missing_file_is_none() {
        assert!(read(std::path::Path::new("不存在的报告.txt")).is_none());
    }

    #[test]
    fn text_includes_description_and_excerpt() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("crash-2026-10-25_12.34.56-client.txt");
        write(&path, SAMPLE).expect("写报告");

        let text = read(&path).expect("应能读出报告").to_text();
        assert!(text.contains("Rendering overlay"));
        assert!(text.contains("NullPointerException"));
    }
}
