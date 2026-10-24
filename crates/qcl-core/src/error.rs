//! 领域层错误类型。

use thiserror::Error;

/// 领域层错误。
///
/// 库层使用具体错误类型而非 `anyhow`，便于调用方按 `code` 分类处理
/// （例如 UI 层需要把「版本不存在」与「网络失败」区分成不同提示）。
#[derive(Debug, Error)]
pub enum CoreError {
    /// JSON 解析或序列化失败。
    #[error("JSON 处理失败：{0}")]
    Json(#[from] serde_json::Error),

    /// 版本清单中不存在指定版本。
    #[error("版本清单中不存在版本 `{0}`")]
    VersionNotFound(String),
}

/// 领域层结果类型别名。
pub type Result<T> = std::result::Result<T, CoreError>;
