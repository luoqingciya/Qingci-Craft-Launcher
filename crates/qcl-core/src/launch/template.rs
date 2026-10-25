//! 启动参数模板的变量替换。

use std::collections::BTreeMap;

use crate::error::{CoreError, Result};

/// 启动参数变量表。
///
/// 官方 `version.json` 里的参数是模板，如 `${auth_player_name}`；具体值由启动器在
/// 组装命令行时提供。变量名与官方保持一一对应，不做重命名——否则排查问题时
/// 无法与官方文档对齐。
#[derive(Debug, Clone, Default)]
pub struct LaunchVariables {
    values: BTreeMap<String, String>,
}

impl LaunchVariables {
    /// 创建空变量表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或覆盖一个变量。
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    /// 链式插入，便于构造。
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(name, value);
        self
    }

    /// 已声明的变量名，按字典序。
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// 替换模板中的全部 `${name}` 占位符。
    ///
    /// 未声明的变量返回 [`CoreError::UnknownVariable`]，而不是原样保留。
    /// 理由：把 `${auth_access_token}` 这类占位符原样塞进命令行，游戏会以错误参数启动，
    /// 症状（登录失败、版本号异常）比直接报错难排查得多。
    ///
    /// 替换是**单趟**的：变量值中若含有 `${...}`，不会被再次展开。
    pub fn substitute(&self, template: &str) -> Result<String> {
        let mut output = String::with_capacity(template.len());
        let mut rest = template;

        while let Some(start) = rest.find("${") {
            output.push_str(&rest[..start]);

            let after_open = &rest[start + 2..];
            let Some(end) = after_open.find('}') else {
                // 未闭合的花括号，按普通文本处理。
                output.push_str(&rest[start..]);
                return Ok(output);
            };

            let name = &after_open[..end];
            let value = self
                .values
                .get(name)
                .ok_or_else(|| CoreError::UnknownVariable(name.to_owned()))?;

            output.push_str(value);
            rest = &after_open[end + 1..];
        }

        output.push_str(rest);
        Ok(output)
    }

    /// 批量替换。
    pub fn substitute_all<'a, I>(&self, templates: I) -> Result<Vec<String>>
    where
        I: IntoIterator<Item = &'a str>,
    {
        templates
            .into_iter()
            .map(|template| self.substitute(template))
            .collect()
    }
}
