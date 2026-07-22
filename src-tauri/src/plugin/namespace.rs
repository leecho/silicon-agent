//! plugin 组件的**命名空间前缀**（T108 §6）。
//!
//! 标准规定 plugin 提供的组件在用户面与调用面都带 plugin 名前缀：
//! - skill → `/my-plugin:skill-name`
//! - agent → `my-plugin:code-reviewer`
//!
//! silicon 此前只在内部靠 `(plugin_id, name)` 唯一索引区分，**展示名与调用名都是裸名** ——
//! 装两个都带 `code-reviewer` 的 plugin，用户面完全无法区分，模型按名调用也无从消歧。
//!
//! **限定名只用于 plugin 提供的公开组件。** 散装组件（owner 空）用裸名；expert/team 的私有
//! 组件也不加前缀 —— 它们不进全局池，只在选中该专家/激活该团队时入池，作用域内本就唯一。
//!
//! **前缀不落库。** 存的仍是裸 `name` + `plugin_id`；限定名在呈现/解析层由 join `plugins` 表
//! 现算 —— 冗余 `plugin_name` 进组件表的话，plugin 一改名就漂移。

/// 分隔符（标准形态 `plugin:name`）。
pub const SEP: char = ':';

/// 拼限定名：`{plugin_name}:{name}`。
pub fn qualify(plugin_name: &str, name: &str) -> String {
    format!("{plugin_name}{SEP}{name}")
}

/// 拆限定名 → `(Some(plugin_name), name)`；无前缀 → `(None, name)`。
///
/// 只在**第一个**分隔符处切：技能名本身理论上可含 `:`，前缀归前缀。
pub fn split_qualified(s: &str) -> (Option<&str>, &str) {
    match s.split_once(SEP) {
        // 前缀为空（如 `":foo"`）不视为限定名，避免把裸名误拆。
        Some((prefix, rest)) if !prefix.is_empty() && !rest.is_empty() => (Some(prefix), rest),
        _ => (None, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualify_and_split_roundtrip() {
        assert_eq!(qualify("figma", "get-file"), "figma:get-file");
        assert_eq!(
            split_qualified("figma:get-file"),
            (Some("figma"), "get-file")
        );
    }

    #[test]
    fn bare_name_is_not_qualified() {
        assert_eq!(split_qualified("code-reviewer"), (None, "code-reviewer"));
    }

    #[test]
    fn degenerate_forms_are_treated_as_bare() {
        // 空前缀 / 空名字都不算限定名，按裸名处理（否则会把 `:foo` 拆出一个空 plugin 名）。
        assert_eq!(split_qualified(":foo"), (None, ":foo"));
        assert_eq!(split_qualified("foo:"), (None, "foo:"));
    }

    #[test]
    fn splits_on_first_separator_only() {
        assert_eq!(split_qualified("p:a:b"), (Some("p"), "a:b"));
    }
}
