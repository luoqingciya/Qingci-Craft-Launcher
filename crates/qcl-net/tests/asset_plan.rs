//! 资产下载计划。
//!
//! 夹具直接复用 `qcl-core` 的官方原件，不重复入库——两份索引合计 656 KB，
//! 复制一份只会让两份数据逐渐不一致。

use std::path::PathBuf;

use qcl_core::assets::AssetObjects;
use qcl_core::version::VersionMeta;
use qcl_net::{DownloadPlan, asset_index_task, plan_assets};

fn fixture(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "qcl-core",
        "tests",
        "fixtures",
    ]
    .iter()
    .collect::<PathBuf>()
    .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取夹具 {} 失败：{error}", path.display()))
}

fn index(name: &str) -> AssetObjects {
    serde_json::from_str(&fixture(name)).expect("官方索引应可解析")
}

fn assert_path_matches_hash(task: &qcl_net::DownloadTask) {
    let expected_suffix = format!("{}/{}", &task.sha1[..2], task.sha1);
    assert!(
        task.path.ends_with(&expected_suffix),
        "落地路径应由哈希推导：{} vs {expected_suffix}",
        task.path
    );
    assert_eq!(
        task.url,
        format!("https://resources.download.minecraft.net/{expected_suffix}")
    );
}

#[test]
fn modern_index_plan_covers_every_object() {
    let plan = plan_assets(&index("asset-index-34.json"), "assets");

    assert_eq!(plan.len(), 5147);
    assert_eq!(plan.total_bytes(), 484_894_038);

    for task in &plan.tasks {
        assert!(
            task.path.starts_with("assets/objects/"),
            "对象应落在 assets/objects 下：{}",
            task.path
        );
        assert_eq!(task.sha1.len(), 40);
        assert_path_matches_hash(task);
    }
}

#[test]
fn legacy_index_plan_dedupes_repeated_content() {
    let index = index("asset-index-1.7.10.json");
    let plan = plan_assets(&index, "assets");

    // 686 个逻辑路径，674 个对象：12 个路径指向已存在的内容。
    assert_eq!(plan.len(), 674);
    // 计划的字节数等于去重后的实际下载量，比 totalSize 少 176 KB。
    assert_eq!(plan.total_bytes(), index.deduplicated_size());
    assert_eq!(plan.total_bytes(), 112_220_327);
    assert!(
        plan.total_bytes() < index.total_size(),
        "去重必须真的省下字节，否则进度分母会虚高"
    );

    let mut paths: Vec<&str> = plan.tasks.iter().map(|task| task.path.as_str()).collect();
    let total = paths.len();
    paths.sort_unstable();
    paths.dedup();
    assert_eq!(paths.len(), total, "计划中不应出现重复落地路径");
}

#[test]
fn plan_order_is_deterministic() {
    let first = plan_assets(&index("asset-index-1.7.10.json"), "assets");
    let second = plan_assets(&index("asset-index-1.7.10.json"), "assets");

    assert_eq!(first, second, "同一份索引两次生成的计划必须完全一致");
}

#[test]
fn invalid_hash_entries_are_skipped_not_fatal() {
    // 一个坏条目不该让整份索引失效，但也不能被写入磁盘。
    let raw = r#"{
        "objects": {
            "good": { "hash": "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356", "size": 10 },
            "traversal": { "hash": "../../../../tmp/evil", "size": 10 },
            "uppercase": { "hash": "B62CA8EC10D07E6BF5AC8DAE0C8C1D2E6A1E3356", "size": 10 },
            "short": { "hash": "abc", "size": 10 }
        }
    }"#;
    let index: AssetObjects = serde_json::from_str(raw).expect("应可解析");

    let plan = plan_assets(&index, "assets");

    assert_eq!(plan.len(), 1, "只有合法哈希能进入计划");
    assert_eq!(plan.total_bytes(), 10);
    assert_eq!(
        plan.tasks[0].path,
        "assets/objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
    );
}

#[test]
fn assets_dir_is_configurable() {
    let index: AssetObjects = serde_json::from_str(
        r#"{"objects":{"a":{"hash":"b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356","size":1}}}"#,
    )
    .expect("应可解析");

    let plan = plan_assets(&index, "instances/alpha/assets");

    assert_eq!(
        plan.tasks[0].path,
        "instances/alpha/assets/objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
    );
}

#[test]
fn empty_index_produces_empty_plan() {
    let index: AssetObjects = serde_json::from_str("{\"objects\":{}}").expect("应可解析");
    let plan = plan_assets(&index, "assets");

    assert!(plan.is_empty());
    assert_eq!(plan.total_bytes(), 0);
}

#[test]
fn asset_index_task_points_at_official_url() {
    let meta: VersionMeta =
        serde_json::from_str(&fixture("26.3.json")).expect("版本元数据应可解析");

    let task = asset_index_task(&meta, "assets").expect("26.3 应有资产索引");

    assert_eq!(task.path, "assets/indexes/34.json");
    assert_eq!(task.size, 597_035);
    assert_eq!(task.sha1, "0c8f43b28f837869c1e54a6aeb22c396a1548946");
    assert!(task.url.starts_with("https://piston-meta.mojang.com/"));
}

#[test]
fn asset_index_task_is_absent_without_index() {
    // 直接用真实元数据去掉索引指针，避免为凑齐必填字段而编造数据。
    let mut meta: VersionMeta =
        serde_json::from_str(&fixture("legacy-1.7.10.json")).expect("版本元数据应可解析");
    meta.asset_index = None;

    assert!(asset_index_task(&meta, "assets").is_none());
}

#[test]
fn index_task_and_objects_plan_do_not_overlap() {
    // 索引文件与对象仓库互不重叠，两阶段流程可以安全地分两次执行。
    let meta: VersionMeta =
        serde_json::from_str(&fixture("26.3.json")).expect("版本元数据应可解析");
    let index_task = asset_index_task(&meta, "assets").expect("应有索引任务");
    let plan: DownloadPlan = plan_assets(&index("asset-index-34.json"), "assets");

    assert!(!plan.contains_path(&index_task.path));
    assert!(
        !plan.tasks.iter().any(|task| task.sha1 == index_task.sha1),
        "索引文件不应出现在对象计划中"
    );
}
