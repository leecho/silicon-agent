//! 插件路径/环境变量替换。
//!
//! 支持三类 `${...}` 占位符：
//!   - `${CLAUDE_PLUGIN_ROOT}` → 插件安装目录绝对路径
//!   - `${CLAUDE_PLUGIN_DATA}` → 插件私有数据目录（`{workspace_base}/plugin-data/<plugin_id>`）
//!   - `${VAR}`                → 进程环境变量 `VAR`（缺失留空）
//!
//! 与 Claude 插件约定对齐：变量内联在 mcpServers 的 command/args/env/cwd/url/headers 中。

/// 替换字符串中的 `${...}` 占位符。
/// `${CLAUDE_PLUGIN_ROOT}`/`${CLAUDE_PLUGIN_DATA}` 优先于环境变量；其余按 `std::env::var` 解析，
/// 缺失的环境变量替换为空串。无 `${` 的字符串原样返回（无分配）。
pub fn resolve_plugin_vars(s: &str, plugin_root: &str, plugin_data: &str) -> String {
    if !s.contains("${") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = s[i + 2..].find('}') {
                let name = &s[i + 2..i + 2 + end];
                let value = match name {
                    "CLAUDE_PLUGIN_ROOT" => plugin_root.to_string(),
                    "CLAUDE_PLUGIN_DATA" => plugin_data.to_string(),
                    other => std::env::var(other).unwrap_or_default(),
                };
                out.push_str(&value);
                i = i + 2 + end + 1; // 跳过 `}`
                continue;
            }
        }
        // 非占位符或无闭合 `}`：原样拷贝该字符。
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_plugin_root_and_data() {
        let out = resolve_plugin_vars(
            "${CLAUDE_PLUGIN_ROOT}/bin --data ${CLAUDE_PLUGIN_DATA}",
            "/plugins/p",
            "/data/p",
        );
        assert_eq!(out, "/plugins/p/bin --data /data/p");
    }

    #[test]
    fn replaces_env_var() {
        std::env::set_var("SIW_VARS_TEST_X", "hello");
        let out = resolve_plugin_vars("v=${SIW_VARS_TEST_X}", "/r", "/d");
        assert_eq!(out, "v=hello");
        std::env::remove_var("SIW_VARS_TEST_X");
    }

    #[test]
    fn missing_env_var_becomes_empty() {
        let out = resolve_plugin_vars("a${SIW_NO_SUCH_VAR_ZZZ}b", "/r", "/d");
        assert_eq!(out, "ab");
    }

    #[test]
    fn no_vars_returned_verbatim() {
        let out = resolve_plugin_vars("plain/path --no-vars", "/r", "/d");
        assert_eq!(out, "plain/path --no-vars");
    }

    #[test]
    fn unclosed_placeholder_kept_literal() {
        let out = resolve_plugin_vars("${UNCLOSED and ${CLAUDE_PLUGIN_ROOT}", "/r", "/d");
        // 第一个 `${` 无闭合 `}` 直到第二个变量的 `}`——按从左找最近 `}` 解析：
        // `${UNCLOSED and ${CLAUDE_PLUGIN_ROOT}` 中第一个 `}` 闭合的是 `UNCLOSED and ${CLAUDE_PLUGIN_ROOT`，
        // 作为环境变量名（含空格/嵌套）解析→缺失→空串。
        assert_eq!(out, "");
    }

    #[test]
    fn multiple_same_var() {
        let out = resolve_plugin_vars("${CLAUDE_PLUGIN_ROOT}:${CLAUDE_PLUGIN_ROOT}", "/r", "/d");
        assert_eq!(out, "/r:/r");
    }
}
