// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 令牌持久化。
//!
//! 只存**微软的刷新令牌**：Minecraft 访问令牌活不过一小时，存它没有意义；
//! 刷新令牌能换出全套新令牌，是唯一值得落盘的东西。
//!
//! 存在系统凭据库里（Windows Credential Manager / macOS Keychain / Linux Secret
//! Service），不写配置文件——配置文件是明文，谁都能读。

use keyring::Entry;

/// 凭据库里的服务名，用应用标识符。
const SERVICE: &str = "cn.luoqingciya.qcl";
/// 凭据库里的用户名。同一个服务下可以有多条，这条专放刷新令牌。
const ACCOUNT: &str = "microsoft-refresh-token";

/// 令牌存储。
///
/// 抽成 trait 是为了能测：CI 的 Linux runner 上没有 Secret Service，测试也不该往
/// 用户真实的凭据库里写东西。
pub trait TokenStore: Send + Sync {
    /// 读出刷新令牌。没有或读不到都返回 `None`。
    fn load(&self) -> Option<String>;
    /// 写入刷新令牌。
    fn save(&self, token: &str) -> Result<(), String>;
    /// 删除刷新令牌。本来就没有也算成功。
    fn clear(&self);
}

/// 系统凭据库。
pub struct KeyringStore;

impl KeyringStore {
    /// 凭据库可用时返回 `Some`。
    ///
    /// Linux 上没有 Secret Service（比如精简容器、没登录桌面会话）时会不可用，
    /// 此时调用方应当退化成「不持久化」而不是报错——登录本身仍然能用。
    pub fn new() -> Option<Self> {
        Entry::store_status().as_ref().ok().map(|()| Self)
    }
}

impl TokenStore for KeyringStore {
    fn load(&self) -> Option<String> {
        // `Entry::new` 失败说明凭据库不可用；`get_password` 失败多半是这条凭据
        // 还不存在（首次登录）。两者对调用方的意义一样：没有可用的刷新令牌。
        let entry = Entry::new(SERVICE, ACCOUNT).ok()?;
        entry.get_password().ok().filter(|value| !value.is_empty())
    }

    fn save(&self, token: &str) -> Result<(), String> {
        let entry =
            Entry::new(SERVICE, ACCOUNT).map_err(|error| format!("凭据库不可用：{error}"))?;
        entry
            .set_password(token)
            .map_err(|error| format!("写入凭据库失败：{error}"))
    }

    fn clear(&self) {
        if let Ok(entry) = Entry::new(SERVICE, ACCOUNT) {
            // 删不存在的凭据会报错，这里不关心。
            let _ = entry.delete_credential();
        }
    }
}

/// 凭据库不可用时的替身。
///
/// 让上层不用到处判空：存不进去就是「下次要重新登录」，其余行为完全一致。
pub struct NullStore;

impl TokenStore for NullStore {
    fn load(&self) -> Option<String> {
        None
    }

    fn save(&self, _token: &str) -> Result<(), String> {
        Err("此环境没有可用的系统凭据库".to_owned())
    }

    fn clear(&self) {}
}

#[cfg(test)]
mod tests {
    use super::{NullStore, TokenStore};

    #[test]
    fn 没有凭据库时不报错只是存不了() {
        // 这个替身的意义就是「静默降级」：登录照常可用，只是下次要重新登录。
        let store = NullStore;
        assert_eq!(store.load(), None);
        assert!(store.save("token").is_err());
        store.clear();
    }
}
