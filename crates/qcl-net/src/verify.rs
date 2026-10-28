// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 文件完整性校验。
//!
//! Mojang 全部使用 SHA-1 而非更现代的摘要算法，因此这里必须跟着用 SHA-1。
//! 这不是安全机制，而是**传输完整性**校验——防的是下载截断与代理注入，
//! 因此不涉及算法强度的取舍。

use sha1::{Digest, Sha1};
use sha2::Sha256;

/// 计算字节内容的 SHA-1，返回小写十六进制字符串。
#[must_use]
pub fn hex_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// 计算字节内容的 SHA-256，返回小写十六进制字符串。
///
/// 用于我们自己的制品（译名库），不用于 Mojang 的文件——那些只提供 SHA-1。
#[must_use]
pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// 校验字节内容是否匹配期望的 SHA-256（大小写不敏感）。
///
/// 期望值为空时返回 `false`，理由同 [`verify_sha1`]。
#[must_use]
pub fn verify_sha256(expected: &str, bytes: &[u8]) -> bool {
    if expected.is_empty() {
        return false;
    }
    hex_sha256(bytes).eq_ignore_ascii_case(expected)
}

/// 校验字节内容是否匹配期望的 SHA-1（大小写不敏感）。
///
/// 期望值为空时返回 `false`：空摘要不构成任何有效校验，不能当作通过。
#[must_use]
pub fn verify_sha1(expected: &str, bytes: &[u8]) -> bool {
    if expected.is_empty() {
        return false;
    }
    hex_sha1(bytes).eq_ignore_ascii_case(expected)
}

/// 分块计算 SHA-1。
///
/// 大文件（客户端 jar 约 40 MB、运行时压缩包约 50 MB）不应整体读入内存，
/// 执行器按块喂入后调用 [`Sha1Hasher::finish`] 取摘要。
#[derive(Debug)]
pub struct Sha1Hasher {
    inner: Sha1,
}

impl Default for Sha1Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha1Hasher {
    /// 创建新的哈希器。
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Sha1::new() }
    }

    /// 喂入一块数据。
    pub fn update(&mut self, chunk: &[u8]) {
        self.inner.update(chunk);
    }

    /// 结束计算并返回小写十六进制摘要。
    #[must_use]
    pub fn finish(self) -> String {
        to_hex(&self.inner.finalize())
    }

    /// 校验已喂入的全部数据是否匹配期望摘要。
    #[must_use]
    pub fn finish_and_verify(self, expected: &str) -> bool {
        if expected.is_empty() {
            return false;
        }
        self.finish().eq_ignore_ascii_case(expected)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{hex_sha1, hex_sha256, verify_sha256};

    #[test]
    fn sha256_matches_known_vector() {
        // 空串的 SHA-256，公认值。
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_differs_from_sha1_for_the_same_input() {
        // 防止把两个函数接错：同样是空串，两种摘要完全不同。
        assert_ne!(hex_sha256(b"qcl"), hex_sha1(b"qcl"));
        assert_eq!(hex_sha256(b"qcl").len(), 64);
    }

    #[test]
    fn verify_sha256_is_case_insensitive() {
        let digest = hex_sha256(b"hello");
        assert!(verify_sha256(&digest, b"hello"));
        assert!(verify_sha256(&digest.to_uppercase(), b"hello"));
        assert!(!verify_sha256(&digest, b"hellp"));
    }

    #[test]
    fn empty_expectation_never_passes() {
        // 空摘要不构成校验，不能当作通过。
        assert!(!verify_sha256("", b"anything"));
    }
}
