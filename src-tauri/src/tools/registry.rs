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
    pub fn filter_by_names(&self, names: &[String]) -> ToolRegistry {
        let mut out = ToolRegistry::new();
        for n in names {
            if let Some(t) = self.get(n) {
                out.register(t);
            }
        }
        out
    }

    /// 复制本 registry，但排除指定名字的工具。
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

    /// 批量执行一回合的工具调用。
    ///
    /// 入参 `(tool_call_id, name, args)`，出参 `(tool_call_id, name, result_text)`，
    /// 顺序与入参对齐。连续的并发安全工具用线程并行，其余串行。
    /// 工具 Err / 未知工具一律转成结果文本（不 panic、不中断），每条结果过 `cap_result`。
    pub fn execute_batch(
        &self,
        calls: &[(String, String, serde_json::Value)],
    ) -> Vec<(String, String, String)> {
        let mut results: Vec<(String, String, String)> = Vec::with_capacity(calls.len());
        let mut i = 0;
        while i < calls.len() {
            let safe = self
                .get(&calls[i].1)
                .map(|t| t.concurrency_safe())
                .unwrap_or(false);
            if safe {
                // 收集一段连续的并发安全工具，线程并行执行。
                let mut j = i;
                while j < calls.len()
                    && self
                        .get(&calls[j].1)
                        .map(|t| t.concurrency_safe())
                        .unwrap_or(false)
                {
                    j += 1;
                }
                let mut handles = Vec::new();
                for call in &calls[i..j] {
                    let registry = self.clone();
                    let (id, name, args) = (call.0.clone(), call.1.clone(), call.2.clone());
                    handles.push(std::thread::spawn(move || {
                        let result = run_one(&registry, &name, &args);
                        (id, name, result)
                    }));
                }
                for handle in handles {
                    match handle.join() {
                        Ok(triple) => results.push(triple),
                        Err(_) => results.push((
                            String::new(),
                            String::new(),
                            "工具执行失败: 线程 panic".to_string(),
                        )),
                    }
                }
                i = j;
            } else {
                let (id, name, args) = &calls[i];
                let result = run_one(self, name, args);
                results.push((id.clone(), name.clone(), result));
                i += 1;
            }
        }
        results
    }
}

/// 执行单个工具，Err 转结果文本，结果过 `cap_result`。
fn run_one(registry: &ToolRegistry, name: &str, args: &serde_json::Value) -> String {
    match registry.execute(name, args) {
        Ok(text) => text,
        Err(err) => cap_result(&format!("工具执行失败: {err}"), 8000),
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
