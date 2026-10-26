// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 模组中文译名库。
//!
//! 为什么需要它：Modrinth 的搜索索引只有英文（实测中文关键词 `total_hits = 0`），
//! 结果里的标题与描述也都是英文。译名库让「钠 / 锂 / 机械动力」这些中文习惯叫法
//! 至少在结果里能认出来。
//!
//! 设计取舍：
//!
//! - **键用 slug 而不是项目 id**：slug 可读，贡献者改 JSON 时不用去查 `AANobbMI`
//!   这种 id。代价是项目改 slug 时映射会失效——那种情况极少，且失效只是退回英文名。
//! - **随二进制打包**（`include_str!`）而不是运行时下载：离线可用、无额外请求，
//!   也不会因为上游挂了而显示不出中文。更新随版本发布走。
//! - **只译名字，不译描述**：描述是长文本、术语密集，机翻常常更难懂；
//!   名字是短词，人工维护的质量稳定。
//!
//! 维护方式：直接改 `assets/data/mod-zh.json` 提 PR。格式是一行一条，
//! 便于 review 与冲突合并；`cargo test -p qcl-mods` 会校验格式与重复键。

use std::collections::HashMap;
use std::sync::OnceLock;

/// 译名库原文。编译期嵌入，运行时零 IO。
const RAW: &str = include_str!("../../../assets/data/mod-zh.json");

/// 中文搜索别名 → 英文关键词。
const ALIASES_RAW: &str = include_str!("../../../assets/data/mod-search-aliases.json");

/// Modrinth 分类 id → 中文标签。
const CATEGORIES_RAW: &str = include_str!("../../../assets/data/mod-categories-zh.json");

fn table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(RAW).unwrap_or_else(|error| {
            // 库文件是编译期嵌入的，解析失败说明仓库里的数据坏了——
            // 但为了不让启动器直接崩，这里退化成空表（界面上只显示英文名）。
            eprintln!("模组译名库解析失败：{error}");
            HashMap::new()
        })
    })
}

/// 查一个项目的 slug 对应的中文名。
#[must_use]
pub fn zh_name(slug: &str) -> Option<&'static str> {
    table().get(slug).map(String::as_str)
}

fn parse(raw: &'static str, name: &str) -> HashMap<String, String> {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        eprintln!("{name} 解析失败：{error}");
        HashMap::new()
    })
}

fn aliases() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| parse(ALIASES_RAW, "中文搜索别名表"))
}

fn categories() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| parse(CATEGORIES_RAW, "分类译名表"))
}

/// 把中文搜索词换成英文关键词。
///
/// Modrinth 的索引只有英文（「钠」返回 0 条），所以输入中文时先查这张表。
/// 查不到就原样返回——用户可能本来就在搜英文。
#[must_use]
pub fn resolve_query(input: &str) -> String {
    let trimmed = input.trim();
    aliases()
        .get(trimmed)
        .cloned()
        .unwrap_or_else(|| trimmed.to_owned())
}

/// 取分类的中文标签。没有译名时返回 `None`，界面退回原始 id。
#[must_use]
pub fn category_label(id: &str) -> Option<&'static str> {
    categories().get(id).map(String::as_str)
}

/// 译名库的条目数，用于自检与日志。
#[must_use]
pub fn entry_count() -> usize {
    table().len()
}

#[cfg(test)]
mod tests {
    use super::{RAW, entry_count, zh_name};

    #[test]
    fn known_mods_resolve_to_chinese() {
        assert_eq!(zh_name("sodium"), Some("钠"));
        assert_eq!(zh_name("lithium"), Some("锂"));
        assert_eq!(zh_name("create"), Some("机械动力"));
    }

    #[test]
    fn unknown_slug_returns_none() {
        // 查不到就退回英文名，不该 panic 或返回空串。
        assert_eq!(zh_name("some-unknown-mod"), None);
        assert_eq!(zh_name(""), None);
    }

    #[test]
    fn library_is_not_empty() {
        assert!(entry_count() > 0, "译名库不该是空的");
    }

    #[test]
    fn every_entry_has_a_value() {
        let map: std::collections::HashMap<String, String> =
            serde_json::from_str(RAW).expect("译名库必须是合法 JSON");

        for (slug, name) in &map {
            assert!(!slug.trim().is_empty(), "slug 不能为空");
            assert!(!name.trim().is_empty(), "{slug} 的中文名为空");
        }
    }

    #[test]
    fn no_duplicate_keys() {
        // serde 遇到重复键会静默取后者，所以单独扫一遍原文：
        // 重复键通常意味着合并冲突没处理干净，必须在 CI 拦住。
        let mut seen: Vec<&str> = Vec::new();
        for line in RAW.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix('"') else {
                continue;
            };
            let Some((key, _)) = rest.split_once("\":") else {
                continue;
            };
            assert!(
                !seen.contains(&key),
                "译名库里 {key} 出现了多次——多半是合并冲突没清干净"
            );
            seen.push(key);
        }

        assert_eq!(seen.len(), entry_count(), "行数应与条目数一致");
    }
}

#[cfg(test)]
mod search_tests {
    use super::{ALIASES_RAW, CATEGORIES_RAW, category_label, resolve_query};

    #[test]
    fn chinese_alias_resolves_to_english() {
        // Modrinth 只认英文关键词，中文输入必须先过这层。
        assert_eq!(resolve_query("钠"), "sodium");
        assert_eq!(resolve_query("  锂  "), "lithium");
        assert_eq!(resolve_query("光影"), "shaders");
    }

    #[test]
    fn english_input_passes_through() {
        // 用户本来就在搜英文时不能被改写。
        assert_eq!(resolve_query("sodium"), "sodium");
        assert_eq!(resolve_query("create"), "create");
        assert_eq!(resolve_query("   "), "");
    }

    #[test]
    fn unknown_chinese_passes_through_unchanged() {
        // 表里没有就原样搜——总比悄悄换成别的词好。
        assert_eq!(resolve_query("某个没收录的模组"), "某个没收录的模组");
    }

    #[test]
    fn category_labels_are_translated() {
        assert_eq!(category_label("optimization"), Some("优化"));
        assert_eq!(category_label("technology"), Some("科技"));
        assert_eq!(category_label("unknown-category"), None);
    }

    #[test]
    fn tables_are_valid_and_non_empty() {
        let aliases: std::collections::HashMap<String, String> =
            serde_json::from_str(ALIASES_RAW).expect("别名表必须是合法 JSON");
        let categories: std::collections::HashMap<String, String> =
            serde_json::from_str(CATEGORIES_RAW).expect("分类表必须是合法 JSON");

        assert!(!aliases.is_empty());
        assert_eq!(categories.len(), 19, "Modrinth 的模组分类有 19 个");

        for (key, value) in aliases.iter().chain(categories.iter()) {
            assert!(!key.trim().is_empty(), "键不能为空");
            assert!(!value.trim().is_empty(), "{key} 的值不能为空");
        }
    }
}
