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

/// 一个实例：一套独立的游戏目录，连同它自己的版本选择。
///
/// 名字同时是标识（不允许重名）。这样界面、配置、日志里都是同一个字符串，
/// 不必再维护一套用户看不见的 id——代价是重命名要当成「换标识」处理。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// 显示名，同时作为标识。
    pub name: String,
    /// 游戏目录。`None` 表示默认目录（`<安装目录>/game`）。
    ///
    /// 只有从旧版单目录配置迁移来的那个实例可能是 `None`——新建实例一律带显式目录，
    /// 否则多个实例会指向同一个目录，切换就失去意义。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
    /// 该实例上次选择的版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_version: Option<String>,
    /// 该实例上次选择的加载器版本（如 `0.20.1`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// 该实例用的加载器种类：`fabric` / `forge` / `neoforge`。
    ///
    /// 缺省视为 Fabric：这个字段是后加的，之前的实例全都是 Fabric。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_kind: Option<String>,
}

/// 配置文件内容。
///
/// 只存**用户显式指定**的值。默认值由运行时推导，不写进文件——
/// 否则用户把启动器挪到别的位置后，配置里还留着旧路径。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 离线模式玩家名。`None` 表示用 [`DEFAULT_PLAYER_NAME`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_name: Option<String>,
    /// 最大堆内存（MB）。`None` 表示采用官方 `default-user-jvm` 的值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// 显式指定的 Java 可执行文件。`None` 表示自动选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    /// 下载源：`None` 表示官方，`Some("bmclapi")` 表示国内镜像。
    ///
    /// 存字符串而不是枚举：配置文件是用户能改的，认不出的值要能回落，
    /// 而不是让启动器起不来。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_source: Option<String>,
    /// 代理地址（`host:port`），给**官方安装器**与 processor 子进程用。
    ///
    /// 1.13 起的 Forge / NeoForge 走自研 processor 链（`qcl_install::forge_processor`），
    /// 除 Mojang mappings 一步外全部本地执行，那一步也走下载器与镜像，所以不需要代理；
    /// 只有 1.12.2 及更早仍要起官方安装器，而它会探测 Mojang 主机并校验证书。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer_proxy: Option<String>,
    /// 已知实例。至少一个——[`Settings::normalize`] 保证。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<Instance>,
    /// 当前实例名。指向不存在的名字时由 [`Settings::normalize`] 修正。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_instance: Option<String>,

    /// **旧版字段**：单目录配置里的游戏目录。
    ///
    /// 只为读得懂 0.1.x 的文件而保留，[`Settings::normalize`] 会把它搬进
    /// `instances` 并清空。留着不清就是两份真相。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,
    /// **旧版字段**：单目录配置里的版本选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_version: Option<String>,
    /// **旧版字段**：单目录配置里的加载器选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
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

/// 实例名的最大长度。
pub const MAX_INSTANCE_NAME_LEN: usize = 32;

/// 从旧版配置迁移出来的实例名。
pub const DEFAULT_INSTANCE_NAME: &str = "默认";

impl Settings {
    /// 把旧版「单目录 + 单一选择」的配置搬进实例列表。**幂等**。
    ///
    /// 在 [`load`] 里调用，因此下游拿到的设置一定满足「至少一个实例、
    /// 当前实例指向存在的名字」这两个不变量，不必各自判空。
    #[must_use]
    pub fn normalize(mut self) -> Self {
        if self.instances.is_empty() {
            self.instances.push(Instance {
                name: DEFAULT_INSTANCE_NAME.to_owned(),
                dir: self.game_dir.take(),
                selected_version: self.selected_version.take(),
                loader_version: self.loader_version.take(),
                loader_kind: None,
            });
        }

        // 已经是新格式时旧字段只可能是残留。不清掉，下一次保存就会写出两份真相，
        // 而且没人知道该信哪一份。
        self.game_dir = None;
        self.selected_version = None;
        self.loader_version = None;

        // 当前实例指向一个不存在的名字（手改配置、或删掉了正在用的实例）时
        // 回落到第一个，而不是让界面空着。
        let valid = self
            .active_instance
            .as_ref()
            .is_some_and(|name| self.instances.iter().any(|instance| &instance.name == name));
        if !valid {
            self.active_instance = self.instances.first().map(|instance| instance.name.clone());
        }

        self
    }

    /// 当前实例。`normalize` 之后必定存在，返回 `Option` 只是为了不 panic。
    #[must_use]
    pub fn active(&self) -> Option<&Instance> {
        let name = self.active_instance.as_ref()?;
        self.instances
            .iter()
            .find(|instance| &instance.name == name)
    }

    /// 当前实例的可变引用。
    pub fn active_mut(&mut self) -> Option<&mut Instance> {
        let name = self.active_instance.clone()?;
        self.instances
            .iter_mut()
            .find(|instance| instance.name == name)
    }

    /// 实例的实际目录：没指定就用默认目录。
    #[must_use]
    pub fn instance_dir(install_dir: &Path, instance: &Instance) -> PathBuf {
        instance
            .dir
            .clone()
            .unwrap_or_else(|| default_game_dir(install_dir))
    }

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

        let mut seen = std::collections::HashSet::new();
        let mut using_default_dir = 0;
        for instance in &self.instances {
            let name = instance.name.trim();
            if name.is_empty() {
                return Err("实例名不能为空".to_owned());
            }
            if name.chars().count() > MAX_INSTANCE_NAME_LEN {
                return Err(format!("实例名不能超过 {MAX_INSTANCE_NAME_LEN} 个字符"));
            }
            if !seen.insert(name) {
                return Err(format!("实例名重复：{name}"));
            }
            if instance.dir.is_none() {
                using_default_dir += 1;
            }
        }

        // 两个实例都用默认目录 = 两个实例其实是同一个，切换看起来毫无反应。
        if using_default_dir > 1 {
            return Err("最多只能有一个实例使用默认目录".to_owned());
        }

        Ok(())
    }

    /// 返回一份「只改选择」的设置副本。
    ///
    /// 选择是高频、低风险的写入（点一下就存），而保存设置是表单提交。
    /// 两者分开写，避免「保存设置」把上次的选择清空、或「切换版本」把
    /// 表单里还没提交的改动一起写进去。
    ///
    /// 选择记在**当前实例**上：两个实例可以各玩各的版本，切换过去不会互相覆盖。
    #[must_use]
    pub fn with_selection(
        &self,
        version: Option<String>,
        loader: Option<String>,
        loader_kind: Option<String>,
    ) -> Self {
        let mut next = self.clone();
        if let Some(active) = next.active_mut() {
            active.selected_version = version.filter(|value| !value.trim().is_empty());
            active.loader_version = loader.filter(|value| !value.trim().is_empty());
            // 没选加载器时连带把种类清掉，否则会留下「无加载器 + forge」这种自相矛盾的组合。
            active.loader_kind = if active.loader_version.is_none() {
                None
            } else {
                loader_kind.filter(|value| !value.trim().is_empty())
            };
        }
        next
    }

    /// 下载源（无法识别的值回落官方）。
    #[must_use]
    pub fn download_source(&self) -> qcl_net::DownloadSource {
        self.download_source
            .as_deref()
            .map(qcl_net::DownloadSource::parse)
            .unwrap_or_default()
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
///
/// 返回值一定经过 [`Settings::normalize`]：调用方不必再判断「有没有实例」。
#[must_use]
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
        .unwrap_or_default()
        .normalize()
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

/// 目录是否可写，**不创建目录**。
///
/// 与 [`ensure_writable`] 的区别：那个是「准备好这个目录」，这个只回答能不能写。
/// 判断便携版该把设置放在 exe 旁边还是退回用户目录时用它——探测过程本身不能
/// 在 Program Files 这类地方留下东西。
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".qcl-write-test");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 把旧位置的用户偏好搬到新位置。
///
/// 便携版过去和安装版共用 `%APPDATA%\<标识符>\settings.json`，这正是「一边选的版本
/// 出现在另一边」的原因。分开之后，玩家名、内存、Java 路径属于**用户偏好**，值得带过来；
/// 游戏目录与版本选择属于**这一份安装的状态**，不迁移——带过来就等于把串味的问题
/// 再固化一次。
///
/// 目标已存在时什么都不做，所以重复调用是安全的。
pub fn migrate_preferences(from: &Path, to: &Path) {
    if to.exists() || !from.exists() {
        return;
    }

    let legacy = load(from);
    let carried = Settings {
        game_dir: None,
        player_name: legacy.player_name,
        memory_mb: legacy.memory_mb,
        java_path: legacy.java_path,
        selected_version: None,
        loader_version: None,
        // 下载源是网络环境决定的偏好，跟机器走，带过来。
        download_source: legacy.download_source,
        installer_proxy: legacy.installer_proxy,
        // 实例是「这一份安装的状态」，不迁移——见上面 `selected_version` 的理由。
        instances: Vec::new(),
        active_instance: None,
    };

    // 没有值得带的东西就别写文件，免得凭空造出一个「用户配置过」的假象。
    if carried == Settings::default() {
        return;
    }

    let _ = save(to, &carried);
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_PLAYER_NAME, Instance, MAX_MEMORY_MB, MAX_PLAYER_NAME_LEN, MIN_MEMORY_MB, Settings,
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
        // 读回来一定带一个默认实例——这是 normalize 保证的不变量。
        assert_eq!(settings, Settings::default().normalize());
        assert_eq!(settings.active_instance.as_deref(), Some("默认"));
    }

    #[test]
    fn corrupt_config_yields_defaults_instead_of_panicking() {
        let dir = TempDir::new().expect("临时目录");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ this is not json").expect("应能写入");

        assert_eq!(load(&path), Settings::default().normalize());
    }

    #[test]
    fn settings_roundtrip() {
        let dir = TempDir::new().expect("临时目录");
        let path = dir.path().join("nested/settings.json");
        let settings = Settings {
            player_name: Some("清辞".to_owned()),
            memory_mb: Some(4096),
            java_path: None,
            download_source: Some("bmclapi".to_owned()),
            instances: vec![Instance {
                name: "默认".to_owned(),
                dir: Some(PathBuf::from("/games/qcl")),
                selected_version: Some("26.3".to_owned()),
                loader_version: Some("0.20.1".to_owned()),
                loader_kind: Some("fabric".to_owned()),
            }],
            active_instance: Some("默认".to_owned()),
            ..Settings::default()
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

#[cfg(test)]
mod selection_tests {
    use super::{Instance, Settings};
    use std::path::PathBuf;

    fn populated() -> Settings {
        Settings {
            player_name: Some("清辞".to_owned()),
            memory_mb: Some(6144),
            java_path: Some(PathBuf::from("/usr/bin/java")),
            instances: vec![Instance {
                name: "默认".to_owned(),
                dir: Some(PathBuf::from("/games/qcl")),
                ..Instance::default()
            }],
            active_instance: Some("默认".to_owned()),
            ..Settings::default()
        }
    }

    #[test]
    fn with_selection_only_touches_selection() {
        // 切换版本不该顺手把设置表单里的值改掉——两边是独立的写入路径。
        let next =
            populated().with_selection(Some("26.3".to_owned()), Some("0.20.1".to_owned()), None);
        let active = next.active().expect("必有当前实例");

        assert_eq!(active.selected_version.as_deref(), Some("26.3"));
        assert_eq!(active.loader_version.as_deref(), Some("0.20.1"));
        assert_eq!(active.dir, Some(PathBuf::from("/games/qcl")));
        assert_eq!(next.player_name.as_deref(), Some("清辞"));
        assert_eq!(next.memory_mb, Some(6144));
        assert_eq!(next.java_path, Some(PathBuf::from("/usr/bin/java")));
    }

    #[test]
    fn empty_selection_is_stored_as_none() {
        // 空串来自界面（加载器选「无」），不该写进配置文件。
        let next = populated().with_selection(Some("  ".to_owned()), Some(String::new()), None);
        let active = next.active().expect("必有当前实例");

        assert!(active.selected_version.is_none());
        assert!(active.loader_version.is_none());
    }

    #[test]
    fn 选择记在各自实例上() {
        // 两个实例各玩各的版本，切过去不该互相覆盖。
        let mut settings = populated();
        settings.instances.push(Instance {
            name: "整合包".to_owned(),
            dir: Some(PathBuf::from("/games/pack")),
            ..Instance::default()
        });

        let first = settings.with_selection(Some("26.3".to_owned()), None, None);
        let mut second = first.clone();
        second.active_instance = Some("整合包".to_owned());
        let second = second.with_selection(Some("1.7.10".to_owned()), None, None);

        assert_eq!(
            second.instances[0].selected_version.as_deref(),
            Some("26.3"),
            "切换实例不该改到另一个实例的选择"
        );
        assert_eq!(
            second.instances[1].selected_version.as_deref(),
            Some("1.7.10")
        );
    }

    #[test]
    fn selection_survives_roundtrip() {
        let dir = tempfile::TempDir::new().expect("临时目录");
        let path = dir.path().join("settings.json");
        let settings =
            populated().with_selection(Some("26.3".to_owned()), Some("0.20.1".to_owned()), None);

        super::save(&path, &settings).expect("应能保存");

        assert_eq!(super::load(&path), settings);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{Settings, is_writable, load, migrate_preferences, save};

    #[test]
    fn 只带用户偏好不带版本选择() {
        // 版本选择正是「一边选了另一边也跟着选」的那个东西，不能带过去。
        let dir = tempfile::TempDir::new().expect("临时目录");
        let legacy = dir.path().join("legacy.json");
        let target = dir.path().join("portable").join("settings.json");

        save(
            &legacy,
            &Settings {
                game_dir: Some(dir.path().join("old-game")),
                player_name: Some("阿辞".to_owned()),
                memory_mb: Some(6144),
                java_path: Some(dir.path().join("java.exe")),
                selected_version: Some("26.3".to_owned()),
                loader_version: Some("0.20.1".to_owned()),
                download_source: Some("bmclapi".to_owned()),
                ..Settings::default()
            },
        )
        .expect("写旧配置");

        migrate_preferences(&legacy, &target);

        let migrated = load(&target);
        assert_eq!(migrated.player_name.as_deref(), Some("阿辞"));
        assert_eq!(migrated.memory_mb, Some(6144));
        assert!(migrated.java_path.is_some());
        // 这三个属于「这一份安装的状态」，迁移过来等于把串味问题固化一次。
        assert_eq!(migrated.game_dir, None);
        assert_eq!(migrated.selected_version, None);
        assert_eq!(migrated.loader_version, None);
    }

    #[test]
    fn 目标已存在时不动它() {
        // 用户已经在便携版里配置过，旧文件不该覆盖回去。
        let dir = tempfile::TempDir::new().expect("临时目录");
        let legacy = dir.path().join("legacy.json");
        let target = dir.path().join("settings.json");

        save(
            &legacy,
            &Settings {
                player_name: Some("旧的".to_owned()),
                ..Settings::default()
            },
        )
        .expect("写旧配置");
        save(
            &target,
            &Settings {
                player_name: Some("新的".to_owned()),
                ..Settings::default()
            },
        )
        .expect("写新配置");

        migrate_preferences(&legacy, &target);

        assert_eq!(load(&target).player_name.as_deref(), Some("新的"));
    }

    #[test]
    fn 没有旧文件时不凭空造一个() {
        let dir = tempfile::TempDir::new().expect("临时目录");
        let legacy = dir.path().join("missing.json");
        let target = dir.path().join("settings.json");

        migrate_preferences(&legacy, &target);

        assert!(!target.exists());
    }

    #[test]
    fn 旧文件没有值得带的东西时不写文件() {
        let dir = tempfile::TempDir::new().expect("临时目录");
        let legacy = dir.path().join("legacy.json");
        let target = dir.path().join("settings.json");

        save(
            &legacy,
            &Settings {
                selected_version: Some("26.3".to_owned()),
                ..Settings::default()
            },
        )
        .expect("写旧配置");

        migrate_preferences(&legacy, &target);

        // 只剩版本选择时迁移结果是空的，写出去只会造出「配置过」的假象。
        assert!(!target.exists());
    }

    #[test]
    fn 可写性判断不留下探测文件() {
        let dir = tempfile::TempDir::new().expect("临时目录");
        assert!(is_writable(dir.path()));
        assert!(!dir.path().join(".qcl-write-test").exists());

        // 不存在的目录不该被判成可写——探测不能顺手创建它。
        let missing = dir.path().join("nope");
        assert!(!is_writable(&missing));
        assert!(!missing.exists());
    }
}

#[cfg(test)]
mod instance_tests {
    use super::{Instance, Settings, load, save};
    use std::path::PathBuf;

    fn temp_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("临时目录");
        let path = dir.path().join("settings.json");
        (dir, path)
    }

    #[test]
    fn 旧版单目录配置迁移成一个实例() {
        let (_dir, path) = temp_path();
        // 0.1.x 的文件：只有一套目录与一个选择。
        std::fs::write(
            &path,
            r#"{"gameDir":"/games/qcl","selectedVersion":"26.3","loaderVersion":"0.20.1"}"#,
        )
        .expect("写入旧配置");

        let settings = load(&path);

        assert_eq!(settings.instances.len(), 1);
        assert_eq!(settings.instances[0].name, "默认");
        assert_eq!(settings.instances[0].dir, Some(PathBuf::from("/games/qcl")));
        assert_eq!(
            settings.instances[0].selected_version.as_deref(),
            Some("26.3")
        );
        assert_eq!(
            settings.instances[0].loader_version.as_deref(),
            Some("0.20.1")
        );
        assert_eq!(settings.active_instance.as_deref(), Some("默认"));

        // 旧字段必须清掉：留着就是两份真相，下次保存会写出互相矛盾的配置。
        assert_eq!(settings.game_dir, None);
        assert_eq!(settings.selected_version, None);
        assert_eq!(settings.loader_version, None);
    }

    #[test]
    fn 迁移是幂等的() {
        // load 里调 normalize，所以「读出来再存回去再读」不能越变越多实例。
        let once = Settings::default().normalize();
        let twice = once.clone().normalize();
        assert_eq!(once, twice);
        assert_eq!(twice.instances.len(), 1);
    }

    #[test]
    fn 当前实例指向不存在的名字时回落到第一个() {
        let settings = Settings {
            instances: vec![
                Instance {
                    name: "A".to_owned(),
                    dir: Some(PathBuf::from("/a")),
                    ..Instance::default()
                },
                Instance {
                    name: "B".to_owned(),
                    dir: Some(PathBuf::from("/b")),
                    ..Instance::default()
                },
            ],
            // 手改配置或删掉了正在用的实例。
            active_instance: Some("已删除".to_owned()),
            ..Settings::default()
        }
        .normalize();

        assert_eq!(settings.active_instance.as_deref(), Some("A"));
    }

    #[test]
    fn 保存后旧字段不出现在文件里() {
        let (_dir, path) = temp_path();
        let settings = Settings {
            game_dir: Some(PathBuf::from("/games/qcl")),
            ..Settings::default()
        }
        .normalize();

        save(&path, &settings).expect("应能保存");
        let raw = std::fs::read_to_string(&path).expect("应能读回");

        assert!(!raw.contains("gameDir"), "旧字段不该被写回：{raw}");
        assert!(!raw.contains("selectedVersion"), "旧字段不该被写回：{raw}");
        assert!(raw.contains("\"instances\""), "应写出实例列表：{raw}");
    }

    #[test]
    fn 实例名重复会被拒绝() {
        let settings = Settings {
            instances: vec![
                Instance {
                    name: "生存".to_owned(),
                    dir: Some(PathBuf::from("/a")),
                    ..Instance::default()
                },
                Instance {
                    name: "生存".to_owned(),
                    dir: Some(PathBuf::from("/b")),
                    ..Instance::default()
                },
            ],
            active_instance: Some("生存".to_owned()),
            ..Settings::default()
        };

        let error = settings.validate().expect_err("重名应被拒绝");
        assert!(error.contains("重复"), "{error}");
    }

    #[test]
    fn 两个实例都用默认目录会被拒绝() {
        // 都指向 <安装目录>/game 的话，两个实例其实是同一个，切换看起来毫无反应。
        let settings = Settings {
            instances: vec![
                Instance {
                    name: "A".to_owned(),
                    dir: None,
                    ..Instance::default()
                },
                Instance {
                    name: "B".to_owned(),
                    dir: None,
                    ..Instance::default()
                },
            ],
            active_instance: Some("A".to_owned()),
            ..Settings::default()
        };

        let error = settings.validate().expect_err("应被拒绝");
        assert!(error.contains("默认目录"), "{error}");
    }

    #[test]
    fn 实例名为空或过长会被拒绝() {
        let with_name = |name: &str| Settings {
            instances: vec![Instance {
                name: name.to_owned(),
                dir: Some(PathBuf::from("/a")),
                ..Instance::default()
            }],
            active_instance: Some(name.to_owned()),
            ..Settings::default()
        };

        assert!(with_name("   ").validate().is_err());
        assert!(with_name(&"名".repeat(33)).validate().is_err());
        assert!(with_name(&"名".repeat(32)).validate().is_ok());
    }

    #[test]
    fn 实例目录缺省时用安装目录下的_game() {
        let instance = Instance {
            name: "A".to_owned(),
            dir: None,
            ..Instance::default()
        };
        assert_eq!(
            Settings::instance_dir(&PathBuf::from("/opt/qcl"), &instance),
            PathBuf::from("/opt/qcl/game")
        );
    }
}
