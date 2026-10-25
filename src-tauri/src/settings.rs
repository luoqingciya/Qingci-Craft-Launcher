// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 启动器设置：持久化与目录解析。
//!
//! 这里刻意不依赖 Tauri 类型：函数只接收路径，因此可以在无 GUI 的测试里
//! 完整覆盖「读坏文件不崩」「默认目录怎么算」「不可写怎么退」这些分支。
//!
//! 默认游戏目录放在**安装目录**下（`<安装目录>/game`），走便携式布局：
//! 用户把整个目录拷到 U 盘就能带走存档与模组。代价是安装到 Program Files
//! 这类受保护位置时不可写，因此需要 [`ensure_writable`] 探测并回退。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 配置文件内容。
///
/// 只存**用户显式指定**的值。默认值由运行时推导，不写进文件——
/// 否则用户把启动器挪到别的位置后，配置里还留着旧路径。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 自定义游戏目录。`None` 表示使用默认目录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
    /// 离线模式玩家名。`None` 表示用 [`DEFAULT_PLAYER_NAME`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_name: Option<String>,
    /// 最大堆内存（MB）。`None` 表示采用官方 `default-user-jvm` 的值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// 显式指定的 Java 可执行文件。`None` 表示自动选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
}

/// 未设置玩家名时使用的默认值。
pub const DEFAULT_PLAYER_NAME: &str = "Player";

/// 允许的最小堆内存（MB）。
///
/// 512 是刻意设的下限：更小的堆连原版都跑不起来，与其让用户在游戏启动后
/// 看到 OOM 崩溃，不如在设置页就拒绝。
pub const MIN_MEMORY_MB: u32 = 512;

/// 允许的最大堆内存（MB）。
pub const MAX_MEMORY_MB: u32 = 65_536;

/// 玩家名的最大长度。
pub const MAX_PLAYER_NAME_LEN: usize = 32;

impl Settings {
    /// 校验设置是否可用。
    ///
    /// 返回给用户看的错误文案：这些值都来自界面输入，报错必须能指导修改。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(name) = &self.player_name {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err("玩家名不能为空".to_owned());
            }
            if trimmed.chars().count() > MAX_PLAYER_NAME_LEN {
                return Err(format!("玩家名不能超过 {MAX_PLAYER_NAME_LEN} 个字符"));
            }
        }

        if let Some(mb) = self.memory_mb
            && !(MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&mb)
        {
            return Err(format!(
                "内存必须在 {MIN_MEMORY_MB}–{MAX_MEMORY_MB} MB 之间"
            ));
        }

        if let Some(path) = &self.java_path
            && !path.is_file()
        {
            return Err(format!("指定的 Java 不存在：{}", path.display()));
        }

        Ok(())
    }

    /// 实际使用的玩家名（去空白后回退到默认值）。
    #[must_use]
    pub fn player_name_or_default(&self) -> String {
        self.player_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(DEFAULT_PLAYER_NAME)
            .to_owned()
    }
}

/// 默认游戏目录：`<安装目录>/game`。
#[must_use]
pub fn default_game_dir(install_dir: &Path) -> PathBuf {
    install_dir.join("game")
}

/// 从配置文件读取设置。
///
/// 文件不存在、无法读取、内容损坏时一律返回默认值——配置坏掉不该让启动器
/// 起不来。真正的写入错误会在 [`save`] 里暴露。
#[must_use]
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// 写入配置文件。
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(settings)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path, raw)
}

/// 确认目录存在且可写，返回规范化后的路径。
///
/// 用一个临时文件探测而不是只看 `metadata().permissions()`：Windows 上
/// Program Files 的 ACL、macOS 的沙盒、以及只读挂载都不是权限位能表达的。
pub fn ensure_writable(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;

    let probe = dir.join(".qcl-write-test");
    std::fs::write(&probe, b"")?;
    // 探测文件残留会污染用户目录，失败也不影响结论（目录本身可写）。
    let _ = std::fs::remove_file(&probe);

    // 规范化：用户在对话框里选的路径可能带 `..` 或符号链接。
    //
    // 用 `dunce` 而不是 `std::fs::canonicalize`：后者在 Windows 上返回
    // `\\?\C:\...` 这种 verbatim 前缀，它会原样显示在界面上，也会被塞进
    // `${game_directory}` 传给游戏。多数程序能接受，但日志与报错里极难辨认，
    // 且一旦路径里出现 `..` 或相对段，部分工具会直接拒绝。
    Ok(dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_PLAYER_NAME, MAX_MEMORY_MB, MAX_PLAYER_NAME_LEN, MIN_MEMORY_MB, Settings,
        default_game_dir, ensure_writable, load, save,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn default_directory_is_inside_the_install_dir() {
        let install = PathBuf::from("/opt/qcl");
        assert_eq!(default_game_dir(&install), PathBuf::from("/opt/qcl/game"));
    }

    #[test]
    fn missing_config_yields_defaults() {
        let dir = TempDir::new().expect("临时目录");
        let settings = load(&dir.path().join("nope.json"));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn corrupt_config_yields_defaults_instead_of_panicking() {
        let dir = TempDir::new().expect("临时目录");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ this is not json").expect("应能写入");

        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn settings_roundtrip() {
        let dir = TempDir::new().expect("临时目录");
        let path = dir.path().join("nested/settings.json");
        let settings = Settings {
            game_dir: Some(PathBuf::from("/games/qcl")),
            player_name: Some("清辞".to_owned()),
            memory_mb: Some(4096),
            java_path: None,
        };

        save(&path, &settings).expect("应能保存");
        assert_eq!(load(&path), settings);
    }

    #[test]
    fn default_settings_serialise_without_the_field() {
        // 默认目录不进配置文件：启动器挪位置后不应还指向旧路径。
        let raw = serde_json::to_string(&Settings::default()).expect("应能序列化");
        assert_eq!(raw, "{}");
    }

    #[test]
    fn ensure_writable_creates_the_directory() {
        let dir = TempDir::new().expect("临时目录");
        let target = dir.path().join("a/b/game");

        let resolved = ensure_writable(&target).expect("应可写");

        assert!(target.is_dir());
        assert!(
            !target.join(".qcl-write-test").exists(),
            "探测文件必须被清理"
        );
        // 规范化后的路径必须仍指向同一目录，但不应带上 Windows 的 verbatim 前缀。
        assert_eq!(
            dunce::canonicalize(&resolved).expect("应能规范化"),
            dunce::canonicalize(&target).expect("应能规范化")
        );
    }

    #[test]
    fn resolved_path_has_no_verbatim_prefix() {
        // `\\?\C:\...` 会原样显示在界面上，也会进到 `${game_directory}`。
        // 这条在非 Windows 平台上恒真，但保留它能让回归发生在三平台 CI 上。
        let dir = TempDir::new().expect("临时目录");
        let resolved = ensure_writable(dir.path()).expect("应可写");
        let rendered = resolved.to_string_lossy();

        assert!(
            !rendered.starts_with(r"\\?\"),
            "不应返回 verbatim 路径：{rendered}"
        );
    }

    #[test]
    fn player_name_falls_back_to_default() {
        assert_eq!(
            Settings::default().player_name_or_default(),
            DEFAULT_PLAYER_NAME
        );
        // 只有空白也算未设置：用户误输入空格不该让游戏里出现空名字。
        assert_eq!(
            Settings {
                player_name: Some("   ".to_owned()),
                ..Settings::default()
            }
            .player_name_or_default(),
            DEFAULT_PLAYER_NAME
        );
        assert_eq!(
            Settings {
                player_name: Some("  清辞  ".to_owned()),
                ..Settings::default()
            }
            .player_name_or_default(),
            "清辞",
            "两端空白应被去掉"
        );
    }

    #[test]
    fn validate_rejects_empty_or_too_long_names() {
        let empty = Settings {
            player_name: Some("   ".to_owned()),
            ..Settings::default()
        };
        assert!(empty.validate().is_err());

        let long = Settings {
            player_name: Some("a".repeat(MAX_PLAYER_NAME_LEN + 1)),
            ..Settings::default()
        };
        assert!(long.validate().is_err());

        // 中文按字符数算，不是字节数——32 个汉字应当通过。
        let chinese = Settings {
            player_name: Some("清".repeat(MAX_PLAYER_NAME_LEN)),
            ..Settings::default()
        };
        assert!(chinese.validate().is_ok());
    }

    #[test]
    fn validate_bounds_memory() {
        let too_small = Settings {
            memory_mb: Some(MIN_MEMORY_MB - 1),
            ..Settings::default()
        };
        assert!(too_small.validate().is_err(), "过小的堆会让游戏 OOM");

        let too_large = Settings {
            memory_mb: Some(MAX_MEMORY_MB + 1),
            ..Settings::default()
        };
        assert!(too_large.validate().is_err());

        assert!(
            Settings {
                memory_mb: Some(4096),
                ..Settings::default()
            }
            .validate()
            .is_ok()
        );
        assert!(
            Settings::default().validate().is_ok(),
            "全部为默认值时必须通过"
        );
    }

    #[test]
    fn validate_rejects_missing_java() {
        let missing = Settings {
            java_path: Some(PathBuf::from("/definitely/not/a/java")),
            ..Settings::default()
        };
        assert!(missing.validate().is_err());
    }
}
