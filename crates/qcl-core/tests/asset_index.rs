//! 资产索引内容的解析与派生规则。
//!
//! 两份夹具都是官方原件，未作任何修改：
//! - `asset-index-34.json` 是 26.3 使用的索引（id `34`，584 KB，5147 个对象）
//! - `asset-index-1.7.10.json` 是 1.7.10 的索引（72 KB，686 个逻辑路径 / 674 个对象）

use qcl_core::assets::{
    ASSET_OBJECTS_BASE, AssetObjects, LEGACY_INDEX_IDS, is_legacy_index, is_safe_logical_path,
    maps_to_resources, object_relative_path, object_url,
};
use qcl_core::version::VersionMeta;

const INDEX_26_3: &str = include_str!("fixtures/asset-index-34.json");
const INDEX_1_7_10: &str = include_str!("fixtures/asset-index-1.7.10.json");
const META_26_3: &str = include_str!("fixtures/26.3.json");
const META_1_7_10: &str = include_str!("fixtures/legacy-1.7.10.json");

fn parse(raw: &str) -> AssetObjects {
    serde_json::from_str(raw).expect("官方索引应可解析")
}

#[test]
fn parses_official_26_3_index() {
    let index = parse(INDEX_26_3);

    assert_eq!(index.object_count(), 5147);
    // 与 version.json 里 assetIndex.totalSize 完全一致，是两个独立来源的交叉验证。
    assert_eq!(index.total_size(), 484_894_038);
    assert_eq!(index.unique_hash_count(), 5147, "现代索引不应有重复内容");

    let icon = index
        .objects
        .get("icons/icon_128x128.png")
        .expect("应包含启动器图标");
    assert_eq!(icon.hash, "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356");
    assert_eq!(icon.size, 9101);
}

#[test]
fn legacy_index_reuses_content_across_paths() {
    let index = parse(INDEX_1_7_10);

    assert_eq!(index.object_count(), 686);
    assert_eq!(
        index.unique_hash_count(),
        674,
        "1.7.10 有 12 个逻辑路径指向已存在的对象，去重不是可选项"
    );
    assert_eq!(index.total_size(), 112_396_854);

    // 12 个重复路径带来 176 KB 的重复计入。若拿 total_size 当进度分母，
    // 进度条会永远停在 99.8% 左右。
    assert_eq!(index.deduplicated_size(), 112_220_327);
    assert_eq!(index.total_size() - index.deduplicated_size(), 176_527);
}

#[test]
fn modern_index_has_nothing_to_deduplicate() {
    let index = parse(INDEX_26_3);
    assert_eq!(index.deduplicated_size(), index.total_size());
}

#[test]
fn index_totals_match_version_metadata() {
    // 索引自身的合计必须等于 version.json 声明的 totalSize，
    // 否则说明索引文件与版本元数据不同步（上游改档或被中间人替换）。
    for (index_raw, meta_raw) in [(INDEX_26_3, META_26_3), (INDEX_1_7_10, META_1_7_10)] {
        let index = parse(index_raw);
        let meta: VersionMeta = serde_json::from_str(meta_raw).expect("版本元数据应可解析");
        let declared = meta.asset_index.as_ref().expect("应有资产索引指针");

        assert_eq!(
            index.total_size(),
            declared.total_size,
            "版本 {} 的索引合计与 totalSize 不一致",
            meta.id
        );
    }
}

#[test]
fn official_index_roundtrips_without_field_loss() {
    for (name, raw) in [("34", INDEX_26_3), ("1.7.10", INDEX_1_7_10)] {
        let original: serde_json::Value = serde_json::from_str(raw).expect("应是合法 JSON");
        let index = parse(raw);
        let roundtripped = serde_json::to_value(&index).expect("应可序列化");

        assert_eq!(
            roundtripped, original,
            "索引 {name} 往返后与原始 JSON 不一致"
        );
    }
}

#[test]
fn object_path_uses_first_two_hash_characters() {
    let hash = "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356";
    assert_eq!(
        object_relative_path(hash).as_deref(),
        Some("b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356")
    );
    assert_eq!(
        object_url(hash).as_deref(),
        Some(
            "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        )
    );
    assert_eq!(
        object_url(hash).expect("应有 URL").split('/').nth(2),
        Some("resources.download.minecraft.net")
    );
    assert!(ASSET_OBJECTS_BASE.starts_with("https://"));
}

#[test]
fn malformed_hashes_are_rejected() {
    // 空串、过短、过长、含大写、含非十六进制字符、路径穿越。
    for bad in [
        "",
        "abc",
        "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e335",
        "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e33560",
        "B62CA8EC10D07E6BF5AC8DAE0C8C1D2E6A1E3356",
        "../../etc/passwd",
        "../../../../../../../../../../tmp/evil",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e335 ",
    ] {
        assert!(
            object_relative_path(bad).is_none(),
            "非法哈希 `{bad}` 不应生成路径"
        );
        assert!(object_url(bad).is_none(), "非法哈希 `{bad}` 不应生成 URL");
    }
}

#[test]
fn legacy_index_ids_are_enumerated() {
    // 这两个 id 对应不理解索引格式的游戏版本，需要展开成目录树。
    assert_eq!(LEGACY_INDEX_IDS, ["legacy", "pre-1.6"]);
    assert!(is_legacy_index("legacy"));
    assert!(is_legacy_index("pre-1.6"));

    for modern in ["34", "1.7.10", "1.21", ""] {
        assert!(!is_legacy_index(modern), "`{modern}` 不需要虚拟目录");
    }

    // 只有 pre-1.6 额外需要 resources/ 副本。
    assert!(maps_to_resources("pre-1.6"));
    assert!(!maps_to_resources("legacy"));
    assert!(!maps_to_resources("34"));
}

#[test]
fn empty_index_is_not_an_error() {
    let index = parse("{\"objects\":{}}");
    assert!(index.is_empty());
    assert_eq!(index.total_size(), 0);
    assert_eq!(index.unique_hash_count(), 0);
}

#[test]
fn logical_paths_must_stay_inside_the_target_directory() {
    for good in [
        "icons/icon_16x16.png",
        "minecraft/sounds/block/stone/break1.ogg",
        "minecraft/lang/zh_cn.json",
        "a.txt",
    ] {
        assert!(is_safe_logical_path(good), "`{good}` 应被接受");
    }

    for bad in [
        "",
        "/etc/passwd",
        "../evil.txt",
        "minecraft/../../evil.txt",
        "a/./b",
        "a//b",
        "C:/Windows/system32/evil.dll",
        "minecraft\\..\\..\\evil.txt",
        "minecraft/sounds/a\0.ogg",
        "trailing/",
    ] {
        assert!(!is_safe_logical_path(bad), "`{bad}` 必须被拒绝");
    }
}

#[test]
fn official_logical_paths_are_all_safe() {
    // 官方索引里不应出现任何会被拒绝的键——这条同时是对校验规则的反向约束：
    // 若规则过严，这里会失败。
    for raw in [INDEX_26_3, INDEX_1_7_10] {
        let index = parse(raw);
        let unsafe_keys: Vec<&str> = index
            .objects
            .keys()
            .map(String::as_str)
            .filter(|key| !is_safe_logical_path(key))
            .collect();
        assert!(
            unsafe_keys.is_empty(),
            "存在不安全的官方键：{unsafe_keys:?}"
        );
    }
}
