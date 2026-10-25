//! 资产索引内容：`assets/indexes/<id>.json`。
//!
//! 索引把**逻辑路径**（如 `minecraft/sounds/block/stone/break1.ogg`）映射到**内容哈希**。
//! 文件本体按哈希存放在对象仓库 `assets/objects/<哈希前两位>/<完整哈希>`，因此同一份
//! 内容在磁盘上只存一份——这是 1.7.2 起引入的存储方式，取代了 1.6 及更早的
//! 「按逻辑路径铺开目录树」。
//!
//! 注意与 [`crate::version::AssetIndex`] 区分：那个是 `version.json` 里指向本文件的
//! **指针**（含 url / sha1 / 大小），本模块是索引文件**本身的内容**。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 资产对象仓库的下载根地址。
pub const ASSET_OBJECTS_BASE: &str = "https://resources.download.minecraft.net";

/// 需要展开成目录树的旧版索引 id。
///
/// 这两个 id 对应不理解索引格式的游戏版本：`legacy` 是 1.6.x，`pre-1.6` 是 1.5.x 及更早。
/// 启动器必须把对象仓库还原成 `assets/virtual/<id>/<逻辑路径>` 的目录树，
/// 游戏才能按老路径找到资源。
pub const LEGACY_INDEX_IDS: [&str; 2] = ["legacy", "pre-1.6"];

/// 除虚拟目录外，还需要把资源复制到 `<游戏目录>/resources/` 的索引 id。
///
/// 只有 `pre-1.6` 需要：那之前的版本直接从 `resources/` 读取声音等资源。
pub const MAP_TO_RESOURCES_INDEX_ID: &str = "pre-1.6";

/// SHA-1 十六进制串长度。
const SHA1_HEX_LEN: usize = 40;

/// 资产索引文件的内容。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObjects {
    /// 逻辑路径 → 对象。用 `BTreeMap` 保证遍历顺序稳定，计划输出可复现。
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
}

/// 单个资产对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    /// 内容 SHA-1，同时是对象在仓库中的文件名。
    pub hash: String,
    /// 文件字节数。
    pub size: u64,
}

impl AssetObjects {
    /// 对象数量。
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// 全部对象的字节数合计，用于磁盘空间预检与进度分母。
    ///
    /// 按**逻辑路径**累加，与 `version.json` 的 `assetIndex.totalSize` 口径一致。
    /// 注意重复内容会被重复计入：1.7.10 有 12 个路径指向已存在的对象，
    /// 因此该值比实际下载量多 176 KB。真实下载量用 [`Self::deduplicated_size`]。
    #[must_use]
    pub fn total_size(&self) -> u64 {
        self.objects.values().map(|object| object.size).sum()
    }

    /// 去重后的字节数合计，即实际需要下载的数据量。
    ///
    /// 进度分母必须用这个值，否则永远不会到 100%——重复内容只下载一次。
    #[must_use]
    pub fn deduplicated_size(&self) -> u64 {
        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        self.objects
            .values()
            .filter(|object| seen.insert(object.hash.as_str()))
            .map(|object| object.size)
            .sum()
    }

    /// 去重后的对象数量。逻辑路径数可能多于对象数（同一内容多处引用）。
    #[must_use]
    pub fn unique_hash_count(&self) -> usize {
        self.objects
            .values()
            .map(|object| object.hash.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

/// 对象在仓库中的相对路径：`<哈希前两位>/<完整哈希>`。
///
/// 哈希会先做格式校验。这不只是卫生问题：路径直接由哈希拼出，若索引里出现
/// `../` 之类的哈希值而不加拦截，等于把任意写文件的能力交给了远端索引。
/// 返回 `None` 表示该条目不可用，调用方应跳过而非报错——一个坏条目不该
/// 让整份索引失效。
#[must_use]
pub fn object_relative_path(hash: &str) -> Option<String> {
    if !is_valid_sha1(hash) {
        return None;
    }
    Some(format!("{}/{hash}", &hash[..2]))
}

/// 对象的下载地址。
#[must_use]
pub fn object_url(hash: &str) -> Option<String> {
    object_relative_path(hash).map(|relative| format!("{ASSET_OBJECTS_BASE}/{relative}"))
}

/// 该索引 id 是否需要展开成虚拟目录树。
#[must_use]
pub fn is_legacy_index(id: &str) -> bool {
    LEGACY_INDEX_IDS.contains(&id)
}

/// 该索引 id 是否需要额外复制到 `resources/`。
#[must_use]
pub fn maps_to_resources(id: &str) -> bool {
    id == MAP_TO_RESOURCES_INDEX_ID
}

/// 是否为合法的 40 位小写十六进制 SHA-1。
fn is_valid_sha1(hash: &str) -> bool {
    hash.len() == SHA1_HEX_LEN
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
