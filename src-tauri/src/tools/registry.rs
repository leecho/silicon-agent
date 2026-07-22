use std::collections::HashMap;
use std::sync::Arc;

use crate::tools::{Tool, ToolSpec};

#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    /// 子集 registry：仅保留白名单内、且本 registry 已注册的工具（按 Arc 复用，不重建工具实例）。
    /// 供 child（子运行）按 agent 工具白名单构造受限工具集。
    pub fn filter_by_names(&self, names: &[String]) -> ToolRegistry {
        let mut out = ToolRegistry::new();
        for n in names {
            if let Some(t) = self.get(n) {
                out.register(t);
            }
        }
        out
    }

    /// 复制本 registry，但排除指定名字的工具。供「未声明 tools→全部开放」时仍剔除 dispatch_agent（禁递归）。
    pub fn without_name(&self, exclude: &str) -> ToolRegistry {
        let mut out = ToolRegistry::new();
        for (n, t) in &self.tools {
            if n != exclude {
                out.register(t.clone());
            }
        }
        out
    }

    /// 单工具执行 + 8KB 结果截断护栏。
    pub fn execute(&self, name: &str, args: &serde_json::Value) -> Result<String, String> {
        let tool = self.get(name).ok_or_else(|| format!("未知工具: {name}"))?;
        let out = tool.execute(args)?;
        Ok(cap_result(&out, 8000))
    }

    /// 同 execute，但把 run 级取消标记穿入工具（进程类工具据此 kill 子进程）。
    pub fn execute_cancellable(
        &self,
        name: &str,
        args: &serde_json::Value,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<String, String> {
        let tool = self.get(name).ok_or_else(|| format!("未知工具: {name}"))?;
        let out = tool.execute_cancellable(args, cancel)?;
        Ok(cap_result(&out, 8000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fs_tools::{ReadFile, WriteFile};
    use std::path::PathBuf;

    #[test]
    fn filter_by_names_keeps_only_whitelisted() {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(ReadFile {
            workspace: PathBuf::from("/tmp"),
        }));
        r.register(Arc::new(WriteFile {
            workspace: PathBuf::from("/tmp"),
        }));
        let f = r.filter_by_names(&["read_file".to_string()]);
        assert!(f.get("read_file").is_some());
        assert!(f.get("write_file").is_none());
    }
}

/// 单条工具结果硬截断(头+尾保留)，防撑爆上下文。
pub fn cap_result(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let head: String = chars[..limit.saturating_sub(200)].iter().collect();
    let tail: String = chars[chars.len() - 200..].iter().collect();
    format!("{head}\n...[已截断, 共 {} 字符]...\n{tail}", chars.len())
}
