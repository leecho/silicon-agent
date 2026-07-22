use crate::tools::{Disclosure, Tool, ToolSpec};

/// 控制工具：模型据 system prompt 的「可用工具目录」按需激活 Deferred 工具。
/// 引擎按名拦截、不真 execute——把命中工具写入会话「已激活集」、回灌精简确认（T83）。
pub struct FindTools;

pub const FIND_TOOLS_TOOL: &str = "find_tools";

/// 纯匹配：在 Deferred 工具目录里按 query 子串（name+description，大小写不敏感）
/// 与 select 精确名命中，返回去重后的工具名。Core 工具不参与（已常驻、无需激活）。
pub fn match_deferred_tools(
    specs: &[ToolSpec],
    query: Option<&str>,
    select: Option<&[String]>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    fn push(name: &str, out: &mut Vec<String>) {
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    let deferred: Vec<&ToolSpec> = specs
        .iter()
        .filter(|s| s.disclosure == Disclosure::Deferred)
        .collect();
    if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
        let needle = q.to_lowercase();
        for s in &deferred {
            let hay = format!("{} {}", s.name, s.description).to_lowercase();
            if hay.contains(&needle) {
                push(&s.name, &mut out);
            }
        }
    }
    if let Some(names) = select {
        for want in names {
            if deferred.iter().any(|s| &s.name == want) {
                push(want, &mut out);
            }
        }
    }
    out
}

impl Tool for FindTools {
    fn name(&self) -> &str {
        FIND_TOOLS_TOOL
    }
    fn label(&self) -> &str {
        "加载工具"
    }
    fn description(&self) -> &str {
        "按关键词或精确名加载「可用工具目录」里未默认启用的工具。query 做名称/描述子串匹配，select 按精确工具名加载；命中后这些工具在本会话后续可直接调用。"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "关键词，子串匹配工具名与描述（可选）"},
                "select": {"type": "array", "items": {"type": "string"}, "description": "精确工具名列表，直接加载（可选）"}
            }
        })
    }
    fn disclosure(&self) -> Disclosure {
        Disclosure::Core
    }
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn execute(&self, _args: &serde_json::Value) -> Result<String, String> {
        // 引擎按名拦截，正常不会走到这里。
        Err("find_tools 由引擎处理，不应直接执行".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Disclosure, ToolSpec};

    fn spec(name: &str, desc: &str, d: Disclosure) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            label: name.into(),
            description: desc.into(),
            parameters: serde_json::json!({}),
            disclosure: d,
        }
    }

    fn catalog() -> Vec<ToolSpec> {
        vec![
            spec("read_file", "读文件", Disclosure::Core),
            spec("web_fetch", "抓取网页内容", Disclosure::Deferred),
            spec("mcp__pan__upload", "上传文件到网盘", Disclosure::Deferred),
        ]
    }

    #[test]
    fn query_substring_matches_name_and_desc_case_insensitive() {
        let hits = match_deferred_tools(&catalog(), Some("网盘"), None);
        assert_eq!(hits, vec!["mcp__pan__upload".to_string()]);
        let hits2 = match_deferred_tools(&catalog(), Some("WEB"), None);
        assert_eq!(hits2, vec!["web_fetch".to_string()]);
    }

    #[test]
    fn select_exact_names_only_deferred() {
        let hits = match_deferred_tools(
            &catalog(),
            None,
            Some(&["web_fetch".into(), "read_file".into()]),
        );
        assert_eq!(hits, vec!["web_fetch".to_string()]);
    }

    #[test]
    fn query_and_select_union_dedup() {
        let hits = match_deferred_tools(&catalog(), Some("网盘"), Some(&["web_fetch".into()]));
        let mut sorted = hits.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["mcp__pan__upload".to_string(), "web_fetch".to_string()]
        );
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(match_deferred_tools(&catalog(), Some("zzz"), None).is_empty());
    }

    #[test]
    fn find_tools_is_core_and_safe() {
        let t = FindTools;
        assert_eq!(t.disclosure(), Disclosure::Core);
        assert!(!t.requires_confirmation());
        assert_eq!(t.name(), FIND_TOOLS_TOOL);
    }
}
