//! 重试策略：指数退避与错误可重试性判定。

use std::time::Duration;

/// 指数退避重试策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
        }
    }
}

impl RetryPolicy {
    /// 构造策略。
    ///
    /// `max_attempts` **包含首次尝试**：`3` 表示最多发起 3 次请求（1 次首发 + 2 次重试）。
    /// 传入 `0` 会被提升为 `1`，避免出现「一次都不试」的退化配置。
    #[must_use]
    pub fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            base_delay,
            max_delay,
        }
    }

    /// 最大尝试次数（含首次）。
    #[must_use]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// 第 `attempt` 次尝试（从 1 开始）失败后，是否还值得再试。
    #[must_use]
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    /// 第 `attempt` 次失败后的等待时长；不再重试时返回 `None`。
    ///
    /// 退避序列为 `base × 2^(attempt-1)`，并以 `max_delay` 封顶。
    #[must_use]
    pub fn delay_for(&self, attempt: u32) -> Option<Duration> {
        if !self.should_retry(attempt) {
            return None;
        }

        // 位移上限 16 位，避免 attempt 很大时溢出；实际取值早已被 max_delay 封顶。
        let exponent = attempt.saturating_sub(1).min(16);
        let factor = 1u32 << exponent;
        let delay = self
            .base_delay
            .checked_mul(factor)
            .unwrap_or(self.max_delay);

        Some(delay.min(self.max_delay))
    }
}

/// HTTP 状态码是否值得重试。
///
/// 只重试服务端错误与限流：4xx 中除 `408`（超时）与 `429`（限流）外，
/// 重试没有意义——`404` 再试一百次仍然是 `404`，只会浪费时间和配额。
#[must_use]
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429) || (500..=599).contains(&status)
}
