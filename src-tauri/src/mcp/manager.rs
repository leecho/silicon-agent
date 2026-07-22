//! McpService：server 连接生命周期、状态事件、工具代理供给与调用转发。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::mcp::auth;
use crate::mcp::client::McpClient;
use crate::mcp::proxy::{exposed_name, McpToolProxy};
use crate::mcp::store::McpStore;
use crate::mcp::transport_http::HttpTransport;
use crate::mcp::transport_stdio::StdioTransport;
use crate::mcp::types::{McpServerConfig, McpServerStatus, McpToolDef, McpTransportConfig};
use crate::tools::Tool;

pub struct McpService {
    pub store: McpStore,
    /// server_id -> 活跃连接（Mutex 串行化该 server 上的并发调用）。
    /// 锁中毒时取回内层值继续——单次调用 panic 不应永久废掉该 server。
    conns: Mutex<HashMap<String, Arc<Mutex<McpClient>>>>,
    /// server_id -> 最近一次 tools/list 结果。
    tools: Mutex<HashMap<String, Vec<McpToolDef>>>,
    status: Mutex<HashMap<String, McpServerStatus>>,
    /// 推状态事件；测试场景为 None。
    app: Mutex<Option<tauri::AppHandle>>,
}

impl McpService {
    pub fn new(store: McpStore) -> Arc<Self> {
        Arc::new(Self {
            store,
            conns: Mutex::new(HashMap::new()),
            tools: Mutex::new(HashMap::new()),
            status: Mutex::new(HashMap::new()),
            app: Mutex::new(None),
        })
    }

    pub fn attach_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap_or_else(|e| e.into_inner()) = Some(app);
    }

    /// 启动：后台逐个连接启用的 server，不阻塞调用方。
    pub fn startup_connect_all(self: &Arc<Self>) {
        let Ok(servers) = self.store.list() else {
            return;
        };
        for s in servers.into_iter().filter(|s| s.enabled) {
            let me = self.clone();
            std::thread::spawn(move || {
                if let Err(e) = me.connect_one(&s) {
                    eprintln!("[mcp] 连接 {} 失败：{e}", s.name);
                }
            });
        }
    }

    /// 建立连接 + 握手 + 拉工具列表；状态全程经事件外发。
    pub fn connect_one(&self, cfg: &McpServerConfig) -> Result<(), String> {
        self.set_status(&cfg.id, "connecting", None, 0);
        let result = self.do_connect(cfg);
        match &result {
            Ok(count) => self.set_status(&cfg.id, "connected", None, *count),
            Err(e) if e.starts_with("[unauthorized]") => {
                self.set_status(&cfg.id, "unauthorized", Some(e), 0)
            }
            Err(e) => self.set_status(&cfg.id, "failed", Some(e), 0),
        }
        result.map(|_| ())
    }

    fn do_connect(&self, cfg: &McpServerConfig) -> Result<usize, String> {
        let transport = self.build_transport(cfg)?;
        let mut client = McpClient::connect(transport)?;
        let tools = client.list_tools()?;
        let count = tools.len();
        self.conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cfg.id.clone(), Arc::new(Mutex::new(client)));
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cfg.id.clone(), tools);
        Ok(count)
    }

    fn build_transport(
        &self,
        cfg: &McpServerConfig,
    ) -> Result<Box<dyn crate::mcp::transport::McpTransport>, String> {
        match &cfg.transport {
            McpTransportConfig::Stdio {
                command,
                args,
                env,
                cwd,
            } => Ok(Box::new(StdioTransport::spawn(
                command,
                args,
                env,
                cwd.as_deref(),
            )?)),
            McpTransportConfig::Http { url, headers } => {
                let hs = self.http_headers(cfg, headers)?;
                Ok(Box::new(HttpTransport::new(url.clone(), hs)))
            }
            McpTransportConfig::Sse { url, headers } => {
                let hs = self.http_headers(cfg, headers)?;
                Ok(Box::new(crate::mcp::transport_sse::SseTransport::connect(
                    url, hs,
                )?))
            }
        }
    }

    /// 组装 http/sse 请求头：用户头 + 必要时注入 OAuth Bearer（用户手填 Authorization 优先）。
    fn http_headers(
        &self,
        cfg: &McpServerConfig,
        headers: &std::collections::BTreeMap<String, String>,
    ) -> Result<Vec<(String, String)>, String> {
        let mut hs: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let has_manual_auth = hs
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"));
        if !has_manual_auth && auth::load_tokens(&cfg.id, &self.store.secrets).is_some() {
            let token = auth::ensure_fresh_token(&cfg.id, &self.store.secrets)
                .map_err(|e| format!("[unauthorized] {e}"))?;
            hs.push(("Authorization".into(), format!("Bearer {token}")));
        }
        Ok(hs)
    }

    /// 断开并清理某 server（禁用/删除时调用）。
    pub fn disconnect(&self, server_id: &str) {
        self.conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id);
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(server_id);
        self.set_status(server_id, "disconnected", None, 0);
    }

    /// 为 build_registry 供给全部已连接 server 的代理工具。
    pub fn tool_proxies(self: &Arc<Self>) -> Vec<Arc<dyn Tool>> {
        // 每次构建 registry 都会读一次 SQLite；当前 run 频率低可接受，热路径若成瓶颈再缓存启用清单。
        let Ok(servers) = self.store.list() else {
            return Vec::new();
        };
        let tools = self.tools.lock().unwrap_or_else(|e| e.into_inner());
        let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<Arc<dyn Tool>> = Vec::new();
        for s in servers.iter().filter(|s| s.enabled) {
            let Some(defs) = tools.get(&s.id) else {
                continue;
            };
            for def in defs {
                let name = exposed_name(&s.name, &def.name, &taken);
                taken.insert(name.clone());
                out.push(Arc::new(McpToolProxy {
                    server_id: s.id.clone(),
                    remote_name: def.name.clone(),
                    exposed_name: name,
                    label: format!("MCP·{}", s.name),
                    description: def.description.clone(),
                    schema: def.input_schema.clone(),
                    auto_approve: s.auto_approve,
                    service: self.clone(),
                }));
            }
        }
        out
    }

    /// 工具调用转发。连接断开时按需重连一次（覆盖 401 刷新 token 的场景）。
    pub fn call_tool(
        &self,
        server_id: &str,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let conn = self
            .conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .cloned();
        if let Some(conn) = conn {
            match conn
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .call_tool(tool, args)
            {
                Err(e) if e.starts_with("[unauthorized]") => { /* 落到重连 */ }
                other => return other,
            }
        }
        // 无连接或鉴权失效：重连一次再试。
        let cfg = self
            .store
            .get(server_id)?
            .ok_or_else(|| format!("MCP server 不存在：{server_id}"))?;
        // 已禁用的 server 不允许重连复活。
        if !cfg.enabled {
            return Err(format!("MCP server 已禁用：{server_id}"));
        }
        self.connect_one(&cfg)?;
        let conn = self
            .conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .cloned()
            .ok_or("MCP server 未连接")?;
        let result = conn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .call_tool(tool, args);
        result
    }

    /// 即时验证一份配置（不要求已保存）：握手 + 列表，返回工具清单。
    pub fn test_connection(&self, cfg: &McpServerConfig) -> Result<Vec<McpToolDef>, String> {
        let transport = self.build_transport(cfg)?;
        let mut client = McpClient::connect(transport)?;
        client.list_tools()
    }

    /// 发起 OAuth 授权：同步准备并返回 auth_url（供前端展示/复制），后台等待回调换 token 后重连。
    pub fn oauth_authorize(self: &Arc<Self>, server_id: String) -> Result<String, String> {
        let cfg = self
            .store
            .get(&server_id)?
            .ok_or_else(|| format!("MCP server 不存在：{server_id}"))?;
        let url = match &cfg.transport {
            McpTransportConfig::Http { url, .. } | McpTransportConfig::Sse { url, .. } => {
                url.clone()
            }
            _ => return Err("仅 HTTP/SSE 传输支持 OAuth".into()),
        };
        let pending = auth::prepare_authorization(
            &url,
            cfg.oauth_client_id.clone(),
            cfg.oauth_resource.clone(),
        )?;
        let auth_url = pending.auth_url.clone();
        self.set_status(&server_id, "connecting", None, 0);
        // 自动开浏览器（失败不吞：前端有「复制链接」兜底；记一条日志）。
        if let Some(app) = self.app.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            use tauri_plugin_opener::OpenerExt;
            if let Err(e) = app.opener().open_url(auth_url.clone(), None::<String>) {
                eprintln!("[mcp][oauth] 打开浏览器失败（可手动复制链接）：{e}");
            }
        }
        let me = self.clone();
        std::thread::spawn(move || {
            match auth::finish_authorization(pending, &server_id, &me.store.secrets) {
                Ok(()) => {
                    if let Ok(Some(cfg)) = me.store.get(&server_id) {
                        let _ = me.connect_one(&cfg);
                    }
                }
                Err(e) => me.set_status(&server_id, "unauthorized", Some(&e), 0),
            }
        });
        Ok(auth_url)
    }

    /// 撤销授权：清 token + 断开 + 重置状态。
    /// 设置/清除某 server 的 OAuth `client_id`（`None`/空串 = 清除）。
    ///
    /// **对插件提供的 server 同样开放**：client_id 属于**连接凭证**，不是「包的构成」，
    /// 与「只读」原则不冲突（只读锁的是启停/编辑/删除，不是你的凭证）。
    /// 若不给这条通路，遇到不支持动态注册（DCR）的服务时插件 MCP 就是死路——
    /// 报错让你「在配置里填 clientId」，却没有任何地方能填。
    pub fn set_oauth_client_id(
        &self,
        server_id: &str,
        client_id: Option<String>,
    ) -> Result<(), String> {
        let mut cfg = self
            .store
            .get(server_id)?
            .ok_or_else(|| format!("MCP server 不存在：{server_id}"))?;
        cfg.oauth_client_id = client_id.filter(|s| !s.trim().is_empty());
        self.store.upsert(cfg)?;
        Ok(())
    }

    pub fn oauth_revoke(&self, server_id: &str) {
        let _ = self.store.secrets.clear(&format!("{server_id}:oauth"));
        self.disconnect(server_id);
    }

    /// 返回某 server 最近一次 tools/list 结果（未连接 → 空）。供详情页展示工具清单。
    pub fn tools_for(&self, server_id: &str) -> Vec<McpToolDef> {
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(server_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn statuses(&self) -> Vec<McpServerStatus> {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    /// 列出某插件提供的 MCP server 配置（owner=plugin_id）。供插件详情展示。
    pub fn list_by_plugin(&self, plugin_id: &str) -> Vec<McpServerConfig> {
        self.store.list_by_plugin(plugin_id).unwrap_or_default()
    }

    fn set_status(&self, server_id: &str, state: &str, error: Option<&String>, tool_count: usize) {
        let st = McpServerStatus {
            server_id: server_id.to_string(),
            state: state.to_string(),
            error: error.map(|e| e.replace("[unauthorized] ", "")),
            tool_count,
        };
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server_id.to_string(), st.clone());
        if let Some(app) = self.app.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            use tauri::Emitter;
            let _ = app.emit("mcp_status_event", st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::McpTransportConfig;
    use crate::storage::AppDatabase;

    // ── 测试辅助 ────────────────────────────────────────────────────────────────

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "siw-mgr-{tag}_{}_{}_{nanos}",
            std::process::id(),
            seq,
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn open_store(dir: &std::path::Path) -> McpStore {
        let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("open db"));
        McpStore::new(db, dir.join("mcp.secrets.json")).expect("open mcp store")
    }

    fn make_http_config(id: &str, name: &str, enabled: bool) -> crate::mcp::types::McpServerConfig {
        crate::mcp::types::McpServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            preset_id: None,
            plugin_id: String::new(),
            oauth_client_id: None,
            oauth_resource: None,
            transport: McpTransportConfig::Http {
                url: "https://mcp.example.com/mcp".into(),
                headers: Default::default(),
            },
            auto_approve: false,
            enabled,
        }
    }

    // ── 编译期约束（保留原有保证）────────────────────────────────────────────────

    #[test]
    fn proxies_compile_as_dyn_tool() {
        fn assert_tool(_: &[Arc<dyn Tool>]) {}
        let v: Vec<Arc<dyn Tool>> = Vec::new();
        assert_tool(&v);
    }

    // ── 1. 已禁用 server 不得被 call_tool 重连复活 ──────────────────────────────

    #[test]
    fn call_tool_refuses_disabled_server() {
        let dir = temp_dir("disabled");
        let store = open_store(&dir);

        // upsert 一个 enabled=false 的 http server
        let cfg = make_http_config("mcp-dis-001", "disabled-server", false);
        store.upsert(cfg).expect("upsert");

        let svc = McpService::new(store);
        let err = svc
            .call_tool("mcp-dis-001", "any_tool", &serde_json::json!({}))
            .expect_err("已禁用 server 应返回 Err");
        assert!(err.contains("已禁用"), "错误应含「已禁用」，实际: {err}");
    }

    // ── 2. tool_proxies 对同名工具做去重（exposed_name 互不相同）──────────────

    #[test]
    fn tool_proxies_dedupes_across_servers() {
        let dir = temp_dir("dedup");
        let store = open_store(&dir);

        // upsert 两个 enabled server，名字不同（name 唯一约束）
        let cfg1 = make_http_config("mcp-a-001", "alpha", true);
        let cfg2 = make_http_config("mcp-a-002", "alpha2", true);
        store.upsert(cfg1).expect("upsert alpha");
        store.upsert(cfg2).expect("upsert alpha2");

        let svc = McpService::new(store);

        // 手动往 tools 锁里为两个 server 各塞一条同名工具定义
        {
            let mut tools = svc.tools.lock().unwrap_or_else(|e| e.into_inner());
            let search_def = crate::mcp::types::McpToolDef {
                name: "search".to_string(),
                description: "search tool".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            };
            tools.insert("mcp-a-001".to_string(), vec![search_def.clone()]);
            tools.insert("mcp-a-002".to_string(), vec![search_def]);
        }

        let proxies = svc.tool_proxies();
        assert_eq!(proxies.len(), 2, "两个 server 各一个工具，应产出 2 个代理");

        let name0 = proxies[0].name().to_string();
        let name1 = proxies[1].name().to_string();
        assert_ne!(name0, name1, "两个代理的 exposed_name 应互不相同");
    }

    // ── 3. set_status 在无 AppHandle 时不 panic ──────────────────────────────

    #[test]
    fn set_status_without_app_records() {
        let dir = temp_dir("setstatus");
        let store = open_store(&dir);
        let svc = McpService::new(store);

        // set_status 是私有方法，同模块可直接调用
        svc.set_status("x", "failed", Some(&"e".to_string()), 0);

        let statuses = svc.statuses();
        let found = statuses
            .iter()
            .any(|s| s.server_id == "x" && s.state == "failed");
        assert!(found, "statuses() 应含刚设置的项");
    }
}
