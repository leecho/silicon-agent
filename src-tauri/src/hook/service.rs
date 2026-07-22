//! HookService：进程内 hook 注册表（`plugin_id -> Vec<HookRule>`），随插件启停刷新。
//!
//! 非持久化——hooks 由插件目录解析得到，启动/装/启停时由门面摄取进来。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 一条可执行 hook 规则：解析自 `ParsedHook` + 插件路径，供 runner 在执行时做变量替换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRule {
    /// 生命周期事件名（PreToolUse/PostToolUse/SessionStart/Stop）。
    pub event: String,
    /// 工具名匹配（空=匹配全部；仅 Pre/PostToolUse 有意义）。
    pub matcher: Option<String>,
    /// 待执行的 shell 命令（含 `${CLAUDE_PLUGIN_ROOT}` 等占位符，执行时替换）。
    pub command: String,
    /// 插件安装目录（`${CLAUDE_PLUGIN_ROOT}`）。
    pub plugin_root: PathBuf,
    /// 插件私有数据目录（`${CLAUDE_PLUGIN_DATA}`）。
    pub plugin_data: PathBuf,
}

/// hook 注册表：按 plugin_id 存其规则集，可整组 set/remove；查询按事件 + 工具名匹配。
#[derive(Default)]
pub struct HookService {
    by_plugin: Mutex<HashMap<String, Vec<HookRule>>>,
}

impl HookService {
    pub fn new() -> Self {
        Self {
            by_plugin: Mutex::new(HashMap::new()),
        }
    }

    /// 设置某插件的全部 hook 规则（整组替换）。空集合即清除该插件。
    pub fn set_plugin(&self, plugin_id: &str, rules: Vec<HookRule>) {
        let mut map = self.by_plugin.lock().unwrap();
        if rules.is_empty() {
            map.remove(plugin_id);
        } else {
            map.insert(plugin_id.to_string(), rules);
        }
    }

    /// 移除某插件的全部 hook 规则（卸载/禁用时）。
    pub fn remove_plugin(&self, plugin_id: &str) {
        self.by_plugin.lock().unwrap().remove(plugin_id);
    }

    /// 是否一条 hook 都没有（None 短路的快路径判断）。
    pub fn is_empty(&self) -> bool {
        self.by_plugin
            .lock()
            .unwrap()
            .values()
            .all(|v| v.is_empty())
    }

    /// 取匹配 `event` 与（可选）`tool_name` 的规则克隆集。
    /// - Pre/PostToolUse：matcher 为空匹配全部工具；非空则对 `tool_name` 做正则匹配，
    ///   正则非法时退化为子串包含匹配。
    /// - SessionStart/Stop（`tool_name=None`）：仅取 matcher 为空者。
    pub fn rules_for(&self, event: &str, tool_name: &Option<String>) -> Vec<HookRule> {
        let map = self.by_plugin.lock().unwrap();
        let mut out = Vec::new();
        for rules in map.values() {
            for r in rules {
                if r.event != event {
                    continue;
                }
                let matched = match (tool_name, &r.matcher) {
                    // 会话级事件：仅取无 matcher 的规则。
                    (None, None) => true,
                    (None, Some(_)) => false,
                    // 工具级事件：matcher 空=全部；否则按子串包含匹配工具名（完整精确名亦命中）。
                    (Some(_), None) => true,
                    (Some(tool), Some(m)) => matcher_matches(m, tool),
                };
                if matched {
                    out.push(r.clone());
                }
            }
        }
        out
    }
}

/// matcher 匹配工具名：子串包含匹配（无 regex 依赖；常见 matcher 为精确工具名或前缀/片段）。
/// 锚定写法 `^name$` 去除锚点后退化为精确匹配，兼容 Claude 常见的精确 matcher 写法。
fn matcher_matches(matcher: &str, tool_name: &str) -> bool {
    let trimmed = matcher.trim_start_matches('^').trim_end_matches('$');
    if trimmed != matcher {
        // 带锚点：按精确名匹配。
        return trimmed == tool_name;
    }
    tool_name.contains(matcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(event: &str, matcher: Option<&str>, command: &str) -> HookRule {
        HookRule {
            event: event.into(),
            matcher: matcher.map(|s| s.to_string()),
            command: command.into(),
            plugin_root: PathBuf::from("/r"),
            plugin_data: PathBuf::from("/d"),
        }
    }

    #[test]
    fn set_and_remove_plugin() {
        let svc = HookService::new();
        assert!(svc.is_empty());
        svc.set_plugin("p1", vec![rule("Stop", None, "echo a")]);
        assert!(!svc.is_empty());
        svc.remove_plugin("p1");
        assert!(svc.is_empty());
        // set 空集合即清除。
        svc.set_plugin("p2", vec![rule("Stop", None, "x")]);
        svc.set_plugin("p2", vec![]);
        assert!(svc.is_empty());
    }

    #[test]
    fn session_events_take_empty_matcher_only() {
        let svc = HookService::new();
        svc.set_plugin(
            "p",
            vec![
                rule("SessionStart", None, "echo start"),
                rule("SessionStart", Some("write_file"), "echo ignored"),
            ],
        );
        let got = svc.rules_for("SessionStart", &None);
        assert_eq!(got.len(), 1, "会话级仅取无 matcher 者");
        assert_eq!(got[0].command, "echo start");
    }

    #[test]
    fn tool_events_match_by_substring_or_regex() {
        let svc = HookService::new();
        svc.set_plugin(
            "p",
            vec![
                rule("PreToolUse", None, "all"),
                rule("PreToolUse", Some("write"), "substr"),
                rule("PreToolUse", Some("^command_execute$"), "regex"),
            ],
        );
        let got = svc.rules_for("PreToolUse", &Some("write_file".into()));
        let cmds: Vec<&str> = got.iter().map(|r| r.command.as_str()).collect();
        assert!(cmds.contains(&"all"), "空 matcher 匹配全部");
        assert!(cmds.contains(&"substr"), "子串匹配 write");
        assert!(!cmds.contains(&"regex"), "正则不匹配 write_file");

        let got2 = svc.rules_for("PreToolUse", &Some("command_execute".into()));
        let cmds2: Vec<&str> = got2.iter().map(|r| r.command.as_str()).collect();
        assert!(cmds2.contains(&"regex"));
        assert!(cmds2.contains(&"all"));
        assert!(!cmds2.contains(&"substr"));
    }

    #[test]
    fn event_must_match() {
        let svc = HookService::new();
        svc.set_plugin("p", vec![rule("PostToolUse", None, "post")]);
        assert!(svc.rules_for("PreToolUse", &Some("x".into())).is_empty());
        assert_eq!(svc.rules_for("PostToolUse", &Some("x".into())).len(), 1);
    }
}
