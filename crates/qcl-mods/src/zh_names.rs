// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 模组中文译名库。
//!
//! 为什么需要它：Modrinth 的搜索索引只有英文（实测中文关键词 `total_hits = 0`），
//! 结果里的标题与描述也都是英文。译名库让「钠 / 锂 / 机械动力」这些中文习惯叫法
//! 至少在结果里能认出来。
//!
//! # 数据从哪来
//!
//! 三份 JSON 托管在制品仓库（`qcl-mod` 文件夹），**启动时拉一份到本地缓存**，
//! 不再随二进制打包。理由是这类数据要长期增补：跟着版本走意味着「加一个译名
//! 就得发一次版」，而它本来只是数据，不该绑定代码的发布节奏。
//!
//! # 代价，以及为什么接受
//!
//! - **首次启动要联网**：没网、或仓库不可达时界面上只有英文名。刻意**不内置**一份兜底——
//!   内置之后远端更新不生效时，会变成「我明明改了网站，为什么还是旧的」这种更难查的问题。
//! - **拉取失败不清缓存**：沿用上次成功的那份，只是可能旧。清空比旧更糟。
//! - **每份文件按仓库给的 SHA-256 校验**：不匹配就整次作废，不写缓存。
//! - **三份要么全成、要么全不动**：只更新一半会出现「名字是新的、别名是旧的」，
//!   排查时几乎不可能想到是这个原因。
//!
//! 维护方式：在制品仓库更新 `qcl-mod` 下的三个文件即可，客户端下次启动生效
//! （24 小时内不重复拉取）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use qcl_net::{HttpDownloader, verify_sha256};
use serde::Deserialize;

use crate::ModError;

/// 制品仓库根地址。
pub const DEFAULT_BASE: &str = "https://pypi.luoqingciya.cn";

/// 缓存目录名，位于应用数据目录下。
const CACHE_DIR: &str = "mod-data";

/// 缓存多久算过期。到期前不再打扰仓库。
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// 制品仓库的下载令牌。
///
/// 权限范围**只有** `qcl-mod` 文件夹的 download。泄露的后果是别人能下到这三份公开的
/// 译名数据，所以随客户端分发可以接受；**不要**把它扩权成别的权限，也不要用它访问
/// 仓库里的其它东西。
///
/// 需要轮换时改这里；CI 也可以注入 `QCL_PYPI_TOKEN` 覆盖，那样源码里就不必再留值。
const FALLBACK_TOKEN: &str = "pypi-GJUljub1ovn0f93vvKOhdLJTx4vnpuwSbzbcNh1aiA0";

fn token() -> &'static str {
    option_env!("QCL_PYPI_TOKEN").unwrap_or(FALLBACK_TOKEN)
}

/// 一份数据的用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Names,
    Aliases,
    Categories,
}

/// 仓库里的一个制品。
struct Package {
    /// 制品名（`/api/v1/packages/{name}`）。
    name: &'static str,
    /// 缓存文件名。
    file: &'static str,
    kind: Kind,
}

const PACKAGES: [Package; 3] = [
    Package {
        name: "mod-zh",
        file: "mod-zh.json",
        kind: Kind::Names,
    },
    Package {
        name: "mod-search-aliases",
        file: "mod-search-aliases.json",
        kind: Kind::Aliases,
    },
    Package {
        name: "mod-categories-zh",
        file: "mod-categories-zh.json",
        kind: Kind::Categories,
    },
];

/// 三张表。用 `RwLock` 而不是 `OnceLock`：刷新成功后要整组替换。
#[derive(Debug, Default)]
struct Tables {
    names: HashMap<String, String>,
    aliases: HashMap<String, String>,
    categories: HashMap<String, String>,
}

fn tables() -> &'static RwLock<Tables> {
    static TABLES: OnceLock<RwLock<Tables>> = OnceLock::new();
    TABLES.get_or_init(|| RwLock::new(Tables::default()))
}

/// 一次刷新的结果，用于日志与界面提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefreshOutcome {
    /// 译名条目数。
    pub names: usize,
    /// 搜索别名条目数。
    pub aliases: usize,
    /// 分类标签数。
    pub categories: usize,
}

/// 缓存目录的完整路径。
#[must_use]
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_DIR)
}

/// 从缓存目录加载三张表。
///
/// 启动时调一次。任何一个文件缺失或损坏都只是让对应那张表为空——不 panic，
/// 也不清掉其它文件：译名缺失只会退回英文名，不值得让启动失败。
pub fn load_cache(data_dir: &Path) {
    let dir = cache_dir(data_dir);
    let mut next = Tables::default();

    for package in &PACKAGES {
        let Ok(raw) = std::fs::read_to_string(dir.join(package.file)) else {
            continue;
        };
        let parsed = parse(&raw, package.file);
        match package.kind {
            Kind::Names => next.names = parsed,
            Kind::Aliases => next.aliases = parsed,
            Kind::Categories => next.categories = parsed,
        }
    }

    if let Ok(mut guard) = tables().write() {
        *guard = next;
    }
}

/// 缓存是否还在有效期内。
///
/// 任意一份缺失或过旧都算过期：否则会出现「译名是新的、别名还是三个月前的」。
#[must_use]
pub fn cache_is_fresh(data_dir: &Path) -> bool {
    let dir = cache_dir(data_dir);
    let now = SystemTime::now();

    PACKAGES.iter().all(|package| {
        std::fs::metadata(dir.join(package.file))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age < CACHE_TTL)
    })
}

/// 拉取最新数据，校验后写入缓存并替换内存表。
///
/// `base` 是仓库根地址，测试里可以指向 mock server。
pub async fn refresh(
    downloader: &HttpDownloader,
    base: &str,
    data_dir: &Path,
) -> Result<RefreshOutcome, ModError> {
    let base = base.trim_end_matches('/');

    // 先全部拉齐再落盘：只更新一半比不更新更难排查。
    let mut fetched: Vec<(&Package, String, HashMap<String, String>)> = Vec::new();

    for package in &PACKAGES {
        let detail = fetch_detail(downloader, base, package.name).await?;

        let bytes = downloader
            .fetch_bytes_with_headers(
                &format!("{base}{}", detail.download_url),
                crate::USER_AGENT,
                &[("Authorization", &format!("Bearer {}", token()))],
            )
            .await
            .map_err(|error| ModError::Network(error.to_string()))?;

        if !verify_sha256(&detail.sha256, &bytes) {
            // 摘要不符意味着传坏了或被改过，这一份不能信。
            return Err(ModError::Download(format!(
                "{} 的 SHA-256 与仓库公布的不一致",
                package.file
            )));
        }

        let raw = String::from_utf8(bytes).map_err(|error| {
            ModError::Download(format!("{} 不是合法的 UTF-8：{error}", package.file))
        })?;

        let parsed = parse(&raw, package.file);
        if parsed.is_empty() {
            // 空表等于「把已有译名全删了」，多半是发布时传错了文件。
            return Err(ModError::Download(format!(
                "{} 解析后是空的，拒绝覆盖本地缓存",
                package.file
            )));
        }

        fetched.push((package, raw, parsed));
    }

    // 落盘：先全部写临时文件，再统一改名。这样中途失败时，磁盘上不会出现
    // 「一半新一半旧」——`cache_is_fresh` 以最旧的那份为准，混着放会误判成新鲜。
    let dir = cache_dir(data_dir);
    std::fs::create_dir_all(&dir)
        .map_err(|error| ModError::Download(format!("无法创建缓存目录：{error}")))?;

    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (package, raw, _) in &fetched {
        let target = dir.join(package.file);
        let temp = target.with_extension("json.tmp");
        std::fs::write(&temp, raw)
            .map_err(|error| ModError::Download(format!("写入缓存失败：{error}")))?;
        staged.push((temp, target));
    }
    for (temp, target) in staged {
        std::fs::rename(&temp, &target)
            .map_err(|error| ModError::Download(format!("替换缓存失败：{error}")))?;
    }

    // 内存表跟着换，界面不必重启就能用上新译名。
    let mut next = Tables::default();
    for (package, _, parsed) in fetched {
        match package.kind {
            Kind::Names => next.names = parsed,
            Kind::Aliases => next.aliases = parsed,
            Kind::Categories => next.categories = parsed,
        }
    }

    let outcome = RefreshOutcome {
        names: next.names.len(),
        aliases: next.aliases.len(),
        categories: next.categories.len(),
    };

    if let Ok(mut guard) = tables().write() {
        *guard = next;
    }

    Ok(outcome)
}

/// 仓库返回的制品详情（只取用得到的两个字段）。
#[derive(Debug, Deserialize)]
struct PackageDetail {
    files: Vec<PackageFile>,
}

#[derive(Debug, Deserialize)]
struct PackageFile {
    download_url: String,
    sha256: String,
}

/// 查一个制品的最新文件地址与摘要。
///
/// 这个接口**不需要令牌**（实测匿名可读），所以刻意不带 `Authorization`：
/// 令牌只在真正下载文件时出现，能少发一次就少一次。
async fn fetch_detail(
    downloader: &HttpDownloader,
    base: &str,
    name: &str,
) -> Result<PackageFile, ModError> {
    let raw = downloader
        .fetch_text_with_user_agent(&format!("{base}/api/v1/packages/{name}"), crate::USER_AGENT)
        .await
        .map_err(|error| ModError::Network(error.to_string()))?;

    let detail: PackageDetail = serde_json::from_str(&raw)?;
    detail
        .files
        .into_iter()
        .next()
        .ok_or_else(|| ModError::Download(format!("{name} 没有任何文件")))
}

fn parse(raw: &str, name: &str) -> HashMap<String, String> {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        // 坏数据不该让调用方崩：对应那张表当作空，界面退回英文名。
        eprintln!("{name} 解析失败：{error}");
        HashMap::new()
    })
}

/// 查一个项目的 slug 对应的中文名。
#[must_use]
pub fn zh_name(slug: &str) -> Option<String> {
    tables()
        .read()
        .ok()
        .and_then(|guard| guard.names.get(slug).cloned())
}

/// 把中文搜索词换成英文关键词。
///
/// Modrinth 的索引只有英文（「钠」返回 0 条），所以输入中文时先查这张表。
/// 查不到就原样返回——用户可能本来就在搜英文。
#[must_use]
pub fn resolve_query(input: &str) -> String {
    let trimmed = input.trim();
    tables()
        .read()
        .ok()
        .and_then(|guard| guard.aliases.get(trimmed).cloned())
        .unwrap_or_else(|| trimmed.to_owned())
}

/// 取分类的中文标签。没有译名时返回 `None`，界面退回原始 id。
#[must_use]
pub fn category_label(id: &str) -> Option<String> {
    tables()
        .read()
        .ok()
        .and_then(|guard| guard.categories.get(id).cloned())
}

/// 译名库的条目数，用于自检与日志。
#[must_use]
pub fn entry_count() -> usize {
    tables().read().map(|guard| guard.names.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// 三张表是进程级全局状态，而测试默认并行跑——不加锁会互相覆盖。
    static TABLE_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        TABLE_LOCK.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn set_tables(names: &str, aliases: &str, categories: &str) {
        let mut next = Tables::default();
        next.names = parse(names, "names");
        next.aliases = parse(aliases, "aliases");
        next.categories = parse(categories, "categories");
        *tables().write().expect("锁可用") = next;
    }

    #[test]
    fn lookups_read_from_the_current_tables() {
        let _guard = lock();
        set_tables(
            r#"{"sodium":"钠"}"#,
            r#"{"钠":"sodium"}"#,
            r#"{"optimization":"优化"}"#,
        );

        assert_eq!(zh_name("sodium").as_deref(), Some("钠"));
        assert_eq!(resolve_query(" 钠 "), "sodium");
        assert_eq!(category_label("optimization").as_deref(), Some("优化"));
        assert_eq!(entry_count(), 1);
    }

    #[test]
    fn unknown_keys_fall_back_gracefully() {
        let _guard = lock();
        set_tables(r#"{"sodium":"钠"}"#, "{}", "{}");

        // 查不到就退回英文，不该 panic 也不该返回空串。
        assert_eq!(zh_name("some-unknown-mod"), None);
        assert_eq!(zh_name(""), None);
        assert_eq!(resolve_query("unknown"), "unknown");
        assert_eq!(category_label("nope"), None);
    }

    #[test]
    fn empty_tables_are_the_normal_state_before_the_first_fetch() {
        let _guard = lock();
        // 不内置数据，所以「一张表都没有」是合法的启动状态。
        set_tables("{}", "{}", "{}");
        assert_eq!(entry_count(), 0);
        assert_eq!(zh_name("sodium"), None);
        assert_eq!(resolve_query("钠"), "钠", "别名表空时原样返回");
    }

    #[test]
    fn load_cache_reads_what_was_written() {
        let _guard = lock();
        let dir = TempDir::new().expect("临时目录");
        let cache = cache_dir(dir.path());
        std::fs::create_dir_all(&cache).expect("建目录");
        std::fs::write(cache.join("mod-zh.json"), r#"{"create":"机械动力"}"#).expect("写文件");
        std::fs::write(
            cache.join("mod-search-aliases.json"),
            r#"{"机械动力":"create"}"#,
        )
        .expect("写文件");
        std::fs::write(cache.join("mod-categories-zh.json"), r#"{"magic":"魔法"}"#)
            .expect("写文件");

        load_cache(dir.path());

        assert_eq!(zh_name("create").as_deref(), Some("机械动力"));
        assert_eq!(resolve_query("机械动力"), "create");
        assert_eq!(category_label("magic").as_deref(), Some("魔法"));
    }

    #[test]
    fn load_cache_tolerates_missing_and_broken_files() {
        let _guard = lock();
        let dir = TempDir::new().expect("临时目录");
        let cache = cache_dir(dir.path());
        std::fs::create_dir_all(&cache).expect("建目录");
        // 只有一份，而且是坏 JSON。
        std::fs::write(cache.join("mod-zh.json"), "{ 坏掉的").expect("写文件");

        load_cache(dir.path());

        assert_eq!(entry_count(), 0);
        assert_eq!(zh_name("sodium"), None);
    }

    #[test]
    fn cache_is_stale_when_any_file_is_missing() {
        let dir = TempDir::new().expect("临时目录");
        assert!(!cache_is_fresh(dir.path()), "什么都没有时当然要拉");

        let cache = cache_dir(dir.path());
        std::fs::create_dir_all(&cache).expect("建目录");
        for package in &PACKAGES {
            std::fs::write(cache.join(package.file), "{}").expect("写文件");
        }
        assert!(cache_is_fresh(dir.path()), "刚写完应该是新鲜的");

        // 删掉其中一份：缺一份就整体算过期。
        std::fs::remove_file(cache.join("mod-zh.json")).expect("删文件");
        assert!(!cache_is_fresh(dir.path()));
    }
}
