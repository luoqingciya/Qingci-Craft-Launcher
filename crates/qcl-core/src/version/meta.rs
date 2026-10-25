//! 单个版本的 `version.json` 领域模型。
//!
//! 字段与官方 JSON 一一对应。两条贯穿全模块的约定：
//!
//! 1. 所有可缺失的字段都声明为 `Option` 或带 `#[serde(default)]`：
//!    1.6 之前的版本与现代版本的字段集合差异很大，不能假设字段必然存在。
//! 2. 所有 `Option` 字段都带 `skip_serializing_if = "Option::is_none"`：
//!    序列化时必须还原官方「缺省即省略」的形态，否则写出合并后的
//!    `version.json`（模组加载器场景）会混入大量 `null`，污染后续 diff。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::manifest::VersionType;
use super::rules::{LaunchContext, Rule, evaluate_rules};

/// 单个版本的完整元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMeta {
    /// 版本号，如 `26.3`。
    pub id: String,
    /// 版本类型。
    #[serde(rename = "type")]
    pub version_type: VersionType,
    /// 主类全名，如 `net.minecraft.client.main.Main`。
    #[serde(rename = "mainClass")]
    pub main_class: String,
    /// 启动器最低版本要求。
    #[serde(
        default,
        rename = "minimumLauncherVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub minimum_launcher_version: Option<u32>,
    /// 合规级别。`0` 表示版本过旧、不包含最新的玩家安全特性。
    #[serde(
        default,
        rename = "complianceLevel",
        skip_serializing_if = "Option::is_none"
    )]
    pub compliance_level: Option<u8>,
    /// 版本发布时间（ISO 8601）。
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    /// 元数据更新时间（ISO 8601）。
    pub time: String,
    /// 所需的 Java 运行时信息。
    #[serde(
        default,
        rename = "javaVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub java_version: Option<JavaVersion>,
    /// 资源索引。
    #[serde(
        default,
        rename = "assetIndex",
        skip_serializing_if = "Option::is_none"
    )]
    pub asset_index: Option<AssetIndex>,
    /// 资源索引代号。旧版本可能只提供该字段而不提供 `assetIndex`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    /// 客户端与服务端 jar 的下载信息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    /// 依赖库列表。
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// 1.13 起使用的结构化启动参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    /// 1.13 之前的启动参数字符串（以空格分隔）。
    #[serde(
        default,
        rename = "minecraftArguments",
        skip_serializing_if = "Option::is_none"
    )]
    pub minecraft_arguments: Option<String>,
    /// 日志配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
}

impl VersionMeta {
    /// 是否需要按 1.13 之前的 `minecraftArguments` 方式组装参数。
    ///
    /// 新旧两种形式互斥：同时存在时以结构化的 `arguments` 为准。
    #[must_use]
    pub fn uses_legacy_arguments(&self) -> bool {
        self.arguments.is_none() && self.minecraft_arguments.is_some()
    }

    /// 该版本声明的 Java 主版本号。
    #[must_use]
    pub fn required_java_major(&self) -> Option<u32> {
        self.java_version.as_ref().map(|java| java.major_version)
    }

    /// 解析 game 参数（已按环境求值，但**未做变量替换**）。
    ///
    /// 1.13 起走结构化 `arguments.game`；更早的版本只有空格分隔的
    /// `minecraftArguments` 字符串，按空白切分即可。
    #[must_use]
    pub fn game_arguments(&self, context: &LaunchContext) -> Vec<&str> {
        if let Some(arguments) = &self.arguments {
            return arguments.resolve_game(context);
        }
        self.minecraft_arguments
            .as_deref()
            .map(|raw| raw.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// 解析 JVM 参数（已按环境求值，但**未做变量替换**）。
    ///
    /// 1.13 之前的版本没有结构化 JVM 参数，返回空列表——那些版本所需的
    /// JVM 参数由启动器自行提供，不在 `version.json` 中。
    #[must_use]
    pub fn jvm_arguments(&self, context: &LaunchContext) -> Vec<&str> {
        self.arguments
            .as_ref()
            .map(|arguments| arguments.resolve_jvm(context))
            .unwrap_or_default()
    }
}

/// 版本所需的 Java 运行时。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    /// Mojang 运行时组件名，如 `java-runtime-epsilon`。旧版本无此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// 主版本号，如 `25`。
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

/// 资源索引。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    /// 索引代号，如 `34`。
    pub id: String,
    /// 索引文件的 SHA-1。
    pub sha1: String,
    /// 索引文件自身大小（字节）。
    pub size: u64,
    /// 索引指向的全部资源对象总大小（字节）。
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    /// 索引文件下载地址。
    pub url: String,
}

/// 版本的下载信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    /// 客户端 jar。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<Artifact>,
    /// 服务端 jar。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Artifact>,
}

/// 单个可下载文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// 文件标识。仅日志配置等少数场景提供，依赖库与客户端 jar 不提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 相对于游戏目录的存放路径。仅依赖库条目提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 文件的 SHA-1。
    pub sha1: String,
    /// 文件大小（字节）。
    pub size: u64,
    /// 下载地址。
    pub url: String,
}

/// 依赖库条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// Maven 坐标 `group:artifact:version`。
    pub name: String,
    /// 下载信息。部分条目仅有 `name`（由其他库或启动器自行提供）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// 平台规则。缺省表示全平台适用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    /// natives 映射，如 `{"windows": "natives-windows"}`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<BTreeMap<String, String>>,
    /// natives 解压规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
}

impl Library {
    /// 该依赖库是否适用于给定环境。
    ///
    /// 无 `rules` 的库全平台适用；有 `rules` 的按「默认拒绝、按序匹配」语义求值。
    #[must_use]
    pub fn applies_to(&self, context: &LaunchContext) -> bool {
        evaluate_rules(self.rules.as_deref(), context)
    }
}

/// 依赖库的下载信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    /// 主构件。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    /// natives 等分类构件，键为 classifier 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

/// natives 解压规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extract {
    /// 解压时需要排除的路径前缀，如 `META-INF/`。
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// 结构化启动参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arguments {
    /// 传给主类的参数。
    #[serde(default)]
    pub game: Vec<Argument>,
    /// 传给 JVM 的参数。
    #[serde(default)]
    pub jvm: Vec<Argument>,
    /// 启动器提供的默认用户 JVM 参数。
    ///
    /// 26.3 起出现，含默认堆内存（`-Xms2G -Xmx4G`）与按平台选择的 GC 参数。
    /// 启动器应在用户未显式覆盖内存设置时采用，而非无视。
    #[serde(
        default,
        rename = "default-user-jvm",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_user_jvm: Option<Vec<Argument>>,
}

/// 单个启动参数：要么是无条件值，要么是带规则的条件值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    /// 无条件参数。
    Plain(String),
    /// 带规则的条件参数。
    Conditional {
        /// 平台规则。缺省表示无条件生效（`default-user-jvm` 中存在该形态）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rules: Option<Vec<Rule>>,
        /// 规则命中时追加的参数值。
        value: ArgumentValue,
    },
}

/// 条件参数的值：单个字符串或字符串数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    /// 单个参数。
    Single(String),
    /// 多个参数。
    Many(Vec<String>),
}

impl ArgumentValue {
    /// 以切片形式返回全部参数值。
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::Single(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }
}

impl Argument {
    /// 解析该参数在给定环境下产生的值；返回空切片表示不生效。
    #[must_use]
    pub fn resolve<'a>(&'a self, context: &LaunchContext) -> &'a [String] {
        match self {
            Self::Plain(value) => std::slice::from_ref(value),
            Self::Conditional { rules, value } => {
                if evaluate_rules(rules.as_deref(), context) {
                    value.as_slice()
                } else {
                    &[]
                }
            }
        }
    }
}

impl Arguments {
    /// 解析 game 参数，仅保留当前环境生效的部分。
    ///
    /// 顺序与官方 JSON 一致——参数顺序会影响 JVM 与主类的解析，不能重排。
    #[must_use]
    pub fn resolve_game(&self, context: &LaunchContext) -> Vec<&str> {
        resolve_arguments(&self.game, context)
    }

    /// 解析 jvm 参数，仅保留当前环境生效的部分。
    #[must_use]
    pub fn resolve_jvm(&self, context: &LaunchContext) -> Vec<&str> {
        resolve_arguments(&self.jvm, context)
    }
}

fn resolve_arguments<'a>(arguments: &'a [Argument], context: &LaunchContext) -> Vec<&'a str> {
    arguments
        .iter()
        .flat_map(|argument| argument.resolve(context))
        .map(String::as_str)
        .collect()
}

/// 日志配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    /// 客户端日志配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingClient>,
}

/// 客户端日志配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    /// 参数模板，含 `${path}` 占位符。
    pub argument: String,
    /// 日志配置文件。
    pub file: Artifact,
    /// 配置格式，如 `log4j2-xml`。
    #[serde(rename = "type")]
    pub kind: String,
}
