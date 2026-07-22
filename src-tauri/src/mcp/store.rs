//! mcp_servers 表 CRUD。敏感值入 FileSecretStore（mcp.secrets.json），不落 SQLite。
//!
//! 密钥键名约定：
//!   - `{server_id}:apikey`  — API Key 值
//!   - `{server_id}:oauth`   — OAuth token（JSON）
//!   - `{server_id}:env:{NAME}` — 预留：敏感环境变量

use std::sync::Arc;

use rusqlite::params;

use crate::mcp::types::{McpServerConfig, McpTransportConfig};
use crate::provider::secret::FileSecretStore;
use crate::storage::AppDatabase;

pub struct McpStore {
    db: Arc<AppDatabase>,
    pub secrets: FileSecretStore,
}

impl McpStore {
    pub fn new(db: Arc<AppDatabase>, secrets_path: std::path::PathBuf) -> Result<Self, String> {
        let store = Self {
            db,
            secrets: FileSecretStore::new(secrets_path),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "create table if not exists mcp_servers (
                        id           text primary key,
                        name         text not null unique,
                        preset_id    text,
                        transport    text not null,
                        config_json  text not null,
                        auto_approve integer not null default 0,
                        enabled      integer not null default 1,
                        created_at   text not null,
                        updated_at   text not null
                    );",
                )?;
                // 幂等迁移：旧库补 plugin_id 列（plugin MCP owner；空串=用户独立 server）。
                let has_plugin_id = c
                    .prepare("pragma table_info(mcp_servers)")?
                    .query_map([], |row| row.get::<_, String>(1))?
                    .filter_map(Result::ok)
                    .any(|col| col == "plugin_id");
                if !has_plugin_id {
                    c.execute_batch(
                        "alter table mcp_servers add column plugin_id text not null default '';",
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 一次性迁移旧 auth（api_key→header / oauth→删行）；失败不阻断启动。
        self.migrate_legacy_auth().ok();
        Ok(())
    }

    /// 一次性迁移：把旧 config_json 里的 auth 转成内联凭证或删除该行。
    /// - api_key → 从密钥库读 key，拼成 http header 写进 transport，再清密钥。
    /// - oauth   → 删除该行 + 清 token（原始 JSON 无法表示 OAuth）。
    /// - none/无 → 重写 config_json 去掉 auth。
    /// 幂等：迁移后 config_json 不再含 auth，自然不重复。
    fn migrate_legacy_auth(&self) -> Result<(), String> {
        let rows: Vec<(String, String)> = self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare("select id, config_json from mcp_servers")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .map_err(|e| e.to_string())?;
        for (id, config_json) in rows {
            let val: serde_json::Value = match serde_json::from_str(&config_json) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(auth) = val.get("auth") else {
                continue;
            };
            let auth_type = auth.get("type").and_then(|t| t.as_str()).unwrap_or("none");
            match auth_type {
                "oauth" => {
                    let _ = self.secrets.clear(&format!("{id}:oauth"));
                    let _ = self.db.with_connection(|c| {
                        c.execute("delete from mcp_servers where id = ?1", params![id])?;
                        Ok(())
                    });
                    eprintln!("[mcp][migrate] 删除 OAuth server {id}（原始 JSON 无法表示）");
                }
                "api_key" => {
                    let header_name = auth
                        .get("header_name")
                        .and_then(|h| h.as_str())
                        .unwrap_or("Authorization")
                        .to_string();
                    let prefix = auth
                        .get("value_prefix")
                        .and_then(|p| p.as_str())
                        .unwrap_or("");
                    let key = self
                        .secrets
                        .read(&format!("{id}:apikey"))
                        .unwrap_or_default();
                    if !key.is_empty() {
                        if let Some(t) = val.get("transport") {
                            if let Ok(McpTransportConfig::Http { url, mut headers }) =
                                serde_json::from_value::<McpTransportConfig>(t.clone())
                            {
                                headers.insert(header_name, format!("{prefix}{key}"));
                                let new_t = McpTransportConfig::Http { url, headers };
                                let new_json = serde_json::to_string(
                                    &serde_json::json!({ "transport": new_t }),
                                )
                                .unwrap_or_else(|_| config_json.clone());
                                let _ = self.db.with_connection(|c| {
                                    c.execute(
                                        "update mcp_servers set config_json = ?1 where id = ?2",
                                        params![new_json, id],
                                    )?;
                                    Ok(())
                                });
                            }
                        }
                    }
                    let _ = self.secrets.clear(&format!("{id}:apikey"));
                }
                _ => {
                    if let Some(t) = val.get("transport") {
                        if let Ok(new_json) =
                            serde_json::to_string(&serde_json::json!({ "transport": t }))
                        {
                            let _ = self.db.with_connection(|c| {
                                c.execute(
                                    "update mcp_servers set config_json = ?1 where id = ?2",
                                    params![new_json, id],
                                )?;
                                Ok(())
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 合并导入：parsed 内每条按 name 新增或更新；**绝不删除**不在 parsed 内的服务。
    /// - 已有同名手动行 → 复用 id 更新；新名 → 新建。
    /// - plugin_id 非空（插件提供）的行不受影响。
    /// 删除由调用方显式 `delete` 完成。返回导入后的全部手动服务配置。
    pub fn import_json(
        &self,
        mut parsed: Vec<McpServerConfig>,
    ) -> Result<Vec<McpServerConfig>, String> {
        let existing = self.list()?;
        // 按 name 复用已有手动服务的 id（实现「更新」而非新建重复）。
        let by_name: std::collections::HashMap<String, String> = existing
            .iter()
            .filter(|s| s.plugin_id.is_empty())
            .map(|s| (s.name.clone(), s.id.clone()))
            .collect();
        for cfg in parsed.iter_mut() {
            if let Some(prev_id) = by_name.get(&cfg.name) {
                cfg.id = prev_id.clone();
            }
            self.upsert(cfg.clone())?;
        }
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| s.plugin_id.is_empty())
            .collect())
    }

    /// 列出所有 MCP server 配置，按 created_at 升序。transport 解析失败的损坏行跳过。
    pub fn list(&self) -> Result<Vec<McpServerConfig>, String> {
        self.query_servers(None)
    }

    /// 列出某插件拥有的 MCP server（plugin_id 精确匹配），按 created_at 升序。
    pub fn list_by_plugin(&self, plugin_id: &str) -> Result<Vec<McpServerConfig>, String> {
        self.query_servers(Some(plugin_id))
    }

    /// 共享行映射：`plugin_filter=Some(id)` 仅取该插件的行；`None` 取全部。
    fn query_servers(&self, plugin_filter: Option<&str>) -> Result<Vec<McpServerConfig>, String> {
        self.db
            .with_connection(|c| {
                let base = "select id, name, preset_id, transport, config_json, auto_approve, \
                            enabled, plugin_id from mcp_servers";
                let map_row = |row: &rusqlite::Row| {
                    Ok((
                        row.get::<_, String>(0)?,         // id
                        row.get::<_, String>(1)?,         // name
                        row.get::<_, Option<String>>(2)?, // preset_id
                        row.get::<_, String>(3)?,         // transport tag
                        row.get::<_, String>(4)?,         // config_json
                        row.get::<_, i64>(5)?,            // auto_approve
                        row.get::<_, i64>(6)?,            // enabled
                        row.get::<_, String>(7)?,         // plugin_id
                    ))
                };
                let collected: Vec<_> = match plugin_filter {
                    Some(pid) => {
                        let sql = format!("{base} where plugin_id = ?1 order by created_at");
                        let mut stmt = c.prepare(&sql)?;
                        let rows: Vec<_> = stmt
                            .query_map(params![pid], map_row)?
                            .collect::<Result<_, _>>()?;
                        rows
                    }
                    None => {
                        let sql = format!("{base} order by created_at");
                        let mut stmt = c.prepare(&sql)?;
                        let rows: Vec<_> =
                            stmt.query_map([], map_row)?.collect::<Result<_, _>>()?;
                        rows
                    }
                };

                let mut out = Vec::new();
                for (
                    id,
                    name,
                    preset_id,
                    transport_tag,
                    config_json,
                    auto_approve,
                    enabled,
                    plugin_id,
                ) in collected
                {
                    // config_json 存 {"transport": ..., "auth": ...}
                    let config_val: serde_json::Value = match serde_json::from_str(&config_json) {
                        Ok(v) => v,
                        Err(_) => continue, // 损坏行跳过
                    };

                    let transport: McpTransportConfig = match config_val.get("transport") {
                        Some(t) => match serde_json::from_value(t.clone()) {
                            Ok(v) => v,
                            Err(_) => continue, // transport 解析失败跳过
                        },
                        None => {
                            // 旧格式兼容：整个 config_json 就是 transport
                            match serde_json::from_str(&config_json) {
                                Ok(v) => v,
                                Err(_) => continue,
                            }
                        }
                    };

                    // 校验 transport tag 与解析结果一致（宽松：不一致也接受，已解析成功）
                    let _ = transport_tag;

                    let oauth_resource = config_val
                        .get("oauthResource")
                        .and_then(|c| c.as_str())
                        .map(String::from);
                    let oauth_client_id = config_val
                        .get("oauthClientId")
                        .and_then(|c| c.as_str())
                        .map(String::from);

                    out.push(McpServerConfig {
                        id,
                        name,
                        preset_id,
                        plugin_id,
                        oauth_client_id,
                        oauth_resource,
                        transport,
                        auto_approve: auto_approve != 0,
                        enabled: enabled != 0,
                    });
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<McpServerConfig>, String> {
        Ok(self.list()?.into_iter().find(|s| s.id == id))
    }

    /// 新建或更新（按 id upsert）。返回归一化后的配置。
    /// - id 空则自动生成（格式 `mcp-{毫秒时间戳}-{8位随机hex}`）
    /// - name 空白报错「名称不能为空」
    /// - name 唯一约束冲突 → Err("名称已存在")
    pub fn upsert(&self, mut cfg: McpServerConfig) -> Result<McpServerConfig, String> {
        // 生成 id（若空）
        if cfg.id.trim().is_empty() {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let rnd: u32 = rand::random();
            cfg.id = format!("mcp-{ms}-{rnd:08x}");
        }

        // 校验名称
        if cfg.name.trim().is_empty() {
            return Err("名称不能为空".to_string());
        }

        // 序列化 config_json
        let transport_tag = match &cfg.transport {
            McpTransportConfig::Stdio { .. } => "stdio",
            McpTransportConfig::Http { .. } => "http",
            McpTransportConfig::Sse { .. } => "sse",
        };
        let config_json = serde_json::to_string(&serde_json::json!({
            "transport": cfg.transport,
            "oauthClientId": cfg.oauth_client_id,
            "oauthResource": cfg.oauth_resource,
        }))
        .map_err(|e| format!("序列化配置失败: {e}"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();

        let id = cfg.id.clone();
        let name = cfg.name.clone();
        let preset_id = cfg.preset_id.clone();
        let plugin_id = cfg.plugin_id.clone();
        let auto_approve = if cfg.auto_approve { 1i64 } else { 0i64 };
        let enabled = if cfg.enabled { 1i64 } else { 0i64 };

        let result = self.db.with_connection(|c| {
            c.execute(
                "insert into mcp_servers
                   (id, name, preset_id, transport, config_json, auto_approve, enabled,
                    plugin_id, created_at, updated_at)
                 values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
                 on conflict(id) do update set
                   name         = excluded.name,
                   preset_id    = excluded.preset_id,
                   transport    = excluded.transport,
                   config_json  = excluded.config_json,
                   auto_approve = excluded.auto_approve,
                   enabled      = excluded.enabled,
                   plugin_id    = excluded.plugin_id,
                   updated_at   = excluded.updated_at",
                params![
                    id,
                    name,
                    preset_id,
                    transport_tag,
                    config_json,
                    auto_approve,
                    enabled,
                    plugin_id,
                    now,
                ],
            )?;
            Ok(())
        });

        match result {
            Ok(()) => Ok(cfg),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed: mcp_servers.name") {
                    Err("名称已存在".to_string())
                } else {
                    Err(msg)
                }
            }
        }
    }

    /// 切换自动批准状态。
    pub fn set_auto_approve(&self, id: &str, v: bool) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();
        self.db
            .with_connection(|c| {
                c.execute(
                    "update mcp_servers set auto_approve = ?1, updated_at = ?2 where id = ?3",
                    params![if v { 1i64 } else { 0i64 }, now, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 切换启用状态。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            .to_string();
        self.db
            .with_connection(|c| {
                c.execute(
                    "update mcp_servers set enabled = ?1, updated_at = ?2 where id = ?3",
                    params![if enabled { 1i64 } else { 0i64 }, now, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除实例并清掉其全部密钥。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from mcp_servers where id = ?1", params![id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;

        // 清理密钥（失败忽略）
        let _ = self.secrets.clear(&format!("{id}:apikey"));
        let _ = self.secrets.clear(&format!("{id}:oauth"));
        // TODO: env:{NAME} 密钥槽位启用后需在此一并清理

        Ok(())
    }

    /// 删除某插件拥有的全部 MCP server（卸载插件时调用）。逐个清密钥。
    pub fn delete_by_plugin(&self, plugin_id: &str) -> Result<(), String> {
        for s in self.list_by_plugin(plugin_id)? {
            self.delete(&s.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::McpTransportConfig;

    fn temp_store(tag: &str) -> McpStore {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "siw-mcpstore-{tag}_{}_{}_{nanos}",
            std::process::id(),
            seq
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("open db"));
        McpStore::new(db, dir.join("mcp.secrets.json")).expect("open store")
    }

    fn stdio_cfg(id: &str, name: &str, plugin_id: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: name.into(),
            preset_id: None,
            plugin_id: plugin_id.into(),
            oauth_client_id: None,
            oauth_resource: None,
            transport: McpTransportConfig::Stdio {
                command: "node".into(),
                args: vec!["s.js".into()],
                env: Default::default(),
                cwd: Some("/tmp/root".into()),
            },
            auto_approve: false,
            enabled: true,
        }
    }

    #[test]
    fn upsert_roundtrips_plugin_id_and_cwd() {
        let store = temp_store("rt");
        store.upsert(stdio_cfg("s1", "ns:server", "plg-a")).unwrap();
        let got = store.get("s1").unwrap().expect("present");
        assert_eq!(got.plugin_id, "plg-a");
        match got.transport {
            McpTransportConfig::Stdio { cwd, command, .. } => {
                assert_eq!(command, "node");
                assert_eq!(cwd.as_deref(), Some("/tmp/root"));
            }
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn migrate_api_key_becomes_header_and_oauth_row_deleted() {
        let store = temp_store("mig");
        store
            .db
            .with_connection(|c| {
                c.execute(
                    "insert into mcp_servers (id,name,preset_id,transport,config_json,auto_approve,enabled,plugin_id,created_at,updated_at)
                     values ('k1','keysrv',null,'http',?1,0,1,'','1','1')",
                    rusqlite::params![r#"{"transport":{"type":"http","url":"https://h/mcp"},"auth":{"type":"api_key","header_name":"Authorization","value_prefix":"Bearer "}}"#],
                )?;
                c.execute(
                    "insert into mcp_servers (id,name,preset_id,transport,config_json,auto_approve,enabled,plugin_id,created_at,updated_at)
                     values ('o1','oauthsrv',null,'http',?1,0,1,'','1','1')",
                    rusqlite::params![r#"{"transport":{"type":"http","url":"https://h/mcp"},"auth":{"type":"oauth"}}"#],
                )?;
                Ok(())
            })
            .unwrap();
        store.secrets.set("k1:apikey", "TOK").unwrap();
        store.migrate_legacy_auth().unwrap();
        assert!(store.get("o1").unwrap().is_none());
        let k = store.get("k1").unwrap().expect("present");
        match k.transport {
            McpTransportConfig::Http { headers, .. } => {
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer TOK");
            }
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn upsert_roundtrips_oauth_resource() {
        // resource 覆盖必须能落库回读，否则重启后又退回 server_url（token audience 可能不匹配）。
        let store = temp_store("oauthres");
        let mut cfg = stdio_cfg("o2", "res-srv", "");
        cfg.oauth_resource = Some("https://canonical/mcp".into());
        store.upsert(cfg).unwrap();
        assert_eq!(
            store.get("o2").unwrap().unwrap().oauth_resource.as_deref(),
            Some("https://canonical/mcp")
        );
    }

    #[test]
    fn upsert_roundtrips_oauth_client_id() {
        let store = temp_store("oauthcid");
        let mut cfg = stdio_cfg("o1", "oauth-srv", "");
        cfg.oauth_client_id = Some("cid-9".into());
        store.upsert(cfg).unwrap();
        assert_eq!(
            store.get("o1").unwrap().unwrap().oauth_client_id.as_deref(),
            Some("cid-9")
        );
    }

    #[test]
    fn import_json_merges_without_deleting_existing() {
        let store = temp_store("imp");
        store.upsert(stdio_cfg("u1", "old-manual", "")).unwrap();
        store
            .upsert(stdio_cfg("p1", "plugin-srv", "plg-a"))
            .unwrap();
        // 导入只含一个新手动服务 → newsrv 新增；old-manual 与 plugin-srv 都保留（合并不删）。
        let parsed = vec![stdio_cfg("", "newsrv", "")];
        let saved = store.import_json(parsed).unwrap();
        assert_eq!(saved.len(), 2, "合并：old-manual + newsrv");
        let all = store.list().unwrap();
        assert!(
            all.iter().any(|s| s.name == "old-manual"),
            "已有手动服务不应被删除"
        );
        assert!(all.iter().any(|s| s.name == "newsrv"));
        assert!(all.iter().any(|s| s.name == "plugin-srv"));
    }

    #[test]
    fn import_json_updates_same_name_in_place() {
        let store = temp_store("upd");
        store.upsert(stdio_cfg("u1", "svc", "")).unwrap();
        let mut cfg = stdio_cfg("", "svc", "");
        cfg.enabled = false;
        store.import_json(vec![cfg]).unwrap();
        let manual: Vec<_> = store
            .list()
            .unwrap()
            .into_iter()
            .filter(|s| s.plugin_id.is_empty())
            .collect();
        assert_eq!(manual.len(), 1, "同名应更新不新增");
        assert!(!manual[0].enabled);
    }

    #[test]
    fn list_by_plugin_filters_and_delete_by_plugin_clears() {
        let store = temp_store("byplugin");
        store.upsert(stdio_cfg("a1", "plgA:one", "plg-a")).unwrap();
        store.upsert(stdio_cfg("a2", "plgA:two", "plg-a")).unwrap();
        store.upsert(stdio_cfg("b1", "plgB:one", "plg-b")).unwrap();
        // 用户独立 server（plugin_id 空）
        store.upsert(stdio_cfg("u1", "user:one", "")).unwrap();

        let a = store.list_by_plugin("plg-a").unwrap();
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|s| s.plugin_id == "plg-a"));

        // 全量含用户独立 server。
        assert_eq!(store.list().unwrap().len(), 4);

        store.delete_by_plugin("plg-a").unwrap();
        assert!(store.list_by_plugin("plg-a").unwrap().is_empty());
        // 其它插件与用户 server 不受影响。
        assert_eq!(store.list_by_plugin("plg-b").unwrap().len(), 1);
        assert_eq!(store.list().unwrap().len(), 2);
    }
}
