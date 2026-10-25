// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 极简参数解析。
//!
//! 不引入 clap：本程序只有四个选项，而 clap 的派生宏会带来数百 KB 的依赖。
//! 手写解析也让「参数错了该报什么」完全可控。

use std::path::PathBuf;

/// 用法说明。
pub const USAGE: &str = "\
用法：qcl <版本|latest> [选项]

选项：
  --dir <路径>       游戏目录（默认 .minecraft）
  --name <玩家名>    离线模式玩家名（默认 Player）
  --memory <MB>      最大堆内存，覆盖官方默认值
  --dry-run          只安装并打印启动命令，不启动游戏
  --help             显示本说明
";

/// 解析后的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// 目标版本，`latest` 表示最新正式版。
    pub version: String,
    /// 游戏目录。
    pub game_dir: PathBuf,
    /// 离线玩家名。
    pub player_name: String,
    /// 最大堆内存（MB）。
    pub memory_mb: Option<u32>,
    /// 只打印命令，不启动。
    pub dry_run: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            version: "latest".to_owned(),
            game_dir: PathBuf::from(".minecraft"),
            player_name: "Player".to_owned(),
            memory_mb: None,
            dry_run: false,
        }
    }
}

/// 解析失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsError {
    /// 请求显示帮助。
    Help,
    /// 缺少版本号。
    MissingVersion,
    /// 选项缺少取值。
    MissingValue(String),
    /// 取值非法。
    InvalidValue(String, String),
    /// 未知选项。
    Unknown(String),
}

impl std::fmt::Display for ArgsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help => write!(formatter, "{USAGE}"),
            Self::MissingVersion => write!(formatter, "缺少版本号。\n\n{USAGE}"),
            Self::MissingValue(option) => write!(formatter, "`{option}` 缺少取值。\n\n{USAGE}"),
            Self::InvalidValue(option, value) => {
                write!(formatter, "`{option}` 的取值 `{value}` 不合法。\n\n{USAGE}")
            }
            Self::Unknown(argument) => write!(formatter, "未知参数 `{argument}`。\n\n{USAGE}"),
        }
    }
}

/// 解析命令行（不含程序名）。
pub fn parse<I, S>(arguments: I) -> Result<Args, ArgsError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = Args::default();
    let mut version: Option<String> = None;
    let mut arguments = arguments.into_iter().map(Into::into).peekable();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--help" | "-h" => return Err(ArgsError::Help),
            "--dry-run" => args.dry_run = true,
            "--dir" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| ArgsError::MissingValue(argument.clone()))?;
                args.game_dir = PathBuf::from(value);
            }
            "--name" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| ArgsError::MissingValue(argument.clone()))?;
                if value.trim().is_empty() {
                    return Err(ArgsError::InvalidValue(argument, value));
                }
                args.player_name = value;
            }
            "--memory" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| ArgsError::MissingValue(argument.clone()))?;
                let parsed = value
                    .parse::<u32>()
                    .ok()
                    .filter(|mb| *mb >= 512)
                    .ok_or(ArgsError::InvalidValue(argument, value))?;
                args.memory_mb = Some(parsed);
            }
            other if other.starts_with('-') => return Err(ArgsError::Unknown(argument)),
            other => {
                if version.is_some() {
                    return Err(ArgsError::Unknown(argument));
                }
                version = Some(other.to_owned());
            }
        }
    }

    args.version = version.ok_or(ArgsError::MissingVersion)?;
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::{Args, ArgsError, parse};

    #[test]
    fn defaults_are_applied() {
        let args = parse(["26.3"]).expect("应可解析");
        assert_eq!(
            args,
            Args {
                version: "26.3".to_owned(),
                ..Args::default()
            }
        );
    }

    #[test]
    fn all_options_are_parsed() {
        let args = parse([
            "26.3",
            "--dir",
            "/tmp/game",
            "--name",
            "Steve",
            "--memory",
            "4096",
            "--dry-run",
        ])
        .expect("应可解析");

        assert_eq!(args.version, "26.3");
        assert_eq!(args.game_dir.to_string_lossy(), "/tmp/game");
        assert_eq!(args.player_name, "Steve");
        assert_eq!(args.memory_mb, Some(4096));
        assert!(args.dry_run);
    }

    #[test]
    fn missing_version_is_reported() {
        assert_eq!(parse(Vec::<String>::new()), Err(ArgsError::MissingVersion));
        assert_eq!(parse(["--dry-run"]), Err(ArgsError::MissingVersion));
    }

    #[test]
    fn missing_values_are_reported() {
        assert_eq!(
            parse(["26.3", "--dir"]),
            Err(ArgsError::MissingValue("--dir".to_owned()))
        );
        assert_eq!(
            parse(["26.3", "--memory"]),
            Err(ArgsError::MissingValue("--memory".to_owned()))
        );
    }

    #[test]
    fn invalid_values_are_reported() {
        assert!(matches!(
            parse(["26.3", "--memory", "abc"]),
            Err(ArgsError::InvalidValue(_, _))
        ));
        // 低于 512 MB 直接拒绝：这种设置在真实游戏上必然 OOM，早拒绝比启动后崩溃好。
        assert!(matches!(
            parse(["26.3", "--memory", "128"]),
            Err(ArgsError::InvalidValue(_, _))
        ));
        assert!(matches!(
            parse(["26.3", "--name", "  "]),
            Err(ArgsError::InvalidValue(_, _))
        ));
    }

    #[test]
    fn unknown_arguments_are_rejected() {
        assert_eq!(
            parse(["26.3", "--bogus"]),
            Err(ArgsError::Unknown("--bogus".to_owned()))
        );
        // 两个位置参数也视为错误，避免「qcl 26.3 1.20.1」这种歧义。
        assert_eq!(
            parse(["26.3", "1.20.1"]),
            Err(ArgsError::Unknown("1.20.1".to_owned()))
        );
    }

    #[test]
    fn help_is_reported() {
        assert_eq!(parse(["--help"]), Err(ArgsError::Help));
        assert_eq!(parse(["26.3", "-h"]), Err(ArgsError::Help));
    }
}
