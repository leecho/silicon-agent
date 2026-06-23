//! 远程接入持久化：白名单、peer↔session 绑定 + 暂停态、各平台启停配置。
//! 遵循既有 SessionStore 模式：open() 内 ensure_schema()，with_connection 短事务。

use std::sync::Arc;

use crate::storage::AppDatabase;

pub struct RemoteStore {
    db: Arc<AppDatabase>,
}

/// 白名单条目。仅其中的 (channel, peer_id) 可驱动 agent。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPeer {
    pub channel: String,
    pub peer_id: String,
    pub label: Option<String>,
    pub created_at: String,
}

/// (channel, peer) ↔ 当前 session 的绑定 + 暂停态。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBinding {
    pub channel: String,
    pub peer_id: String,
    pub account: Option<String>,
    pub account_name: Option<String>,
    pub session_id: String,
    pub context_token: Option<String>,
    pub pending_kind: Option<String>,
    pub pending_payload: Option<String>,
    pub updated_at: String,
}

/// 平台启停 + 非密配置。token 类密钥另存 secret 文件，不进 config_json。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteChannelConfig {
    pub channel: String,
    pub enabled: bool,
    pub status: String,
    pub config_json: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

fn clean_account_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.chars().take(64).collect())
}

fn generated_account_name(channel: &str, peer_id: &str) -> String {
    let label = match channel {
        "wechat" => "微信",
        "telegram" => "Telegram",
        "dingtalk" => "钉钉",
        "feishu" => "飞书",
        other => other,
    };
    format!("{label} {}", shorten_peer_id(peer_id))
}

fn shorten_peer_id(peer_id: &str) -> String {
    let chars: Vec<char> = peer_id.chars().collect();
    if chars.len() <= 8 {
        return peer_id.to_string();
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

impl RemoteStore {
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "
                    create table if not exists remote_allowlist (
                        channel text not null,
                        peer_id text not null,
                        label text,
                        created_at text not null,
                        primary key (channel, peer_id)
                    );
                    create table if not exists remote_bindings (
                        channel text not null,
                        peer_id text not null,
                        account text,
                        account_name text,
                        session_id text not null,
                        context_token text,
                        pending_kind text,
                        pending_payload text,
                        updated_at text not null,
                        primary key (channel, peer_id)
                    );
                    create table if not exists remote_channels (
                        channel text not null primary key,
                        enabled integer not null default 0,
                        status text not null default 'disconnected',
                        config_json text,
                        last_error text,
                        updated_at text not null,
                        awaiting_owner integer not null default 0
                    );
                    ",
                )?;
                // 既有库幂等补列（旧版本无 awaiting_owner）。重复添加报错忽略。
                let _ = c.execute(
                    "alter table remote_channels add column awaiting_owner integer not null default 0",
                    [],
                );
                let _ = c.execute(
                    "alter table remote_channels add column status text not null default 'disconnected'",
                    [],
                );
                let _ = c.execute("alter table remote_channels add column last_error text", []);
                let _ = c.execute("alter table remote_bindings add column account_name text", []);
                c.execute(
                    "update remote_channels set status = 'connected' \
                     where enabled != 0 and status = 'disconnected'",
                    [],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn is_allowed(&self, channel: &str, peer_id: &str) -> Result<bool, String> {
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from remote_allowlist where channel = ?1 and peer_id = ?2",
                    rusqlite::params![channel, peer_id],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())
    }

    pub fn add_peer(
        &self,
        channel: &str,
        peer_id: &str,
        label: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into remote_allowlist (channel, peer_id, label, created_at) \
                     values (?1, ?2, ?3, ?4) \
                     on conflict(channel, peer_id) do update set label = excluded.label",
                    rusqlite::params![channel, peer_id, label, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn remove_peer(&self, channel: &str, peer_id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "delete from remote_allowlist where channel = ?1 and peer_id = ?2",
                    rusqlite::params![channel, peer_id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn list_allowlist(&self) -> Result<Vec<AllowedPeer>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select channel, peer_id, label, created_at from remote_allowlist \
                     order by created_at asc",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(AllowedPeer {
                        channel: r.get(0)?,
                        peer_id: r.get(1)?,
                        label: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn get_binding(
        &self,
        channel: &str,
        peer_id: &str,
    ) -> Result<Option<RemoteBinding>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select channel, peer_id, account, account_name, session_id, context_token, \
                     pending_kind, pending_payload, updated_at \
                     from remote_bindings where channel = ?1 and peer_id = ?2",
                )?;
                let mut rows = stmt.query(rusqlite::params![channel, peer_id])?;
                if let Some(r) = rows.next()? {
                    Ok(Some(RemoteBinding {
                        channel: r.get(0)?,
                        peer_id: r.get(1)?,
                        account: r.get(2)?,
                        account_name: r.get(3)?,
                        session_id: r.get(4)?,
                        context_token: r.get(5)?,
                        pending_kind: r.get(6)?,
                        pending_payload: r.get(7)?,
                        updated_at: r.get(8)?,
                    }))
                } else {
                    Ok(None)
                }
            })
            .map_err(|e| e.to_string())
    }

    /// upsert 绑定的 session/account/account_name/context_token；保留既有 pending_* 不动。
    pub fn set_binding(
        &self,
        channel: &str,
        peer_id: &str,
        account: Option<&str>,
        account_name: Option<&str>,
        session_id: &str,
        context_token: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        let resolved_account_name = self.resolve_account_name(channel, peer_id, account_name)?;
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into remote_bindings \
                     (channel, peer_id, account, account_name, session_id, context_token, pending_kind, pending_payload, updated_at) \
                     values (?1, ?2, ?3, ?4, ?5, ?6, null, null, ?7) \
                     on conflict(channel, peer_id) do update set \
                     account = excluded.account, \
                     account_name = excluded.account_name, \
                     session_id = excluded.session_id, \
                     context_token = excluded.context_token, \
                     updated_at = excluded.updated_at",
                    rusqlite::params![
                        channel,
                        peer_id,
                        account,
                        resolved_account_name.as_deref(),
                        session_id,
                        context_token,
                        now
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    fn resolve_account_name(
        &self,
        channel: &str,
        peer_id: &str,
        incoming: Option<&str>,
    ) -> Result<Option<String>, String> {
        if let Some(name) = clean_account_name(incoming) {
            return Ok(Some(name));
        }
        self.db
            .with_connection(|c| {
                let existing: Option<String> = c
                    .query_row(
                        "select account_name from remote_bindings where channel = ?1 and peer_id = ?2",
                        rusqlite::params![channel, peer_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(None);
                if let Some(name) = clean_account_name(existing.as_deref()) {
                    return Ok(Some(name));
                }
                let label: Option<String> = c
                    .query_row(
                        "select label from remote_allowlist where channel = ?1 and peer_id = ?2",
                        rusqlite::params![channel, peer_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(None);
                Ok(clean_account_name(label.as_deref())
                    .or_else(|| Some(generated_account_name(channel, peer_id))))
            })
            .map_err(|e| e.to_string())
    }

    /// 写/清暂停态（None 表示清除）。
    pub fn set_pending(
        &self,
        channel: &str,
        peer_id: &str,
        kind: Option<&str>,
        payload: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update remote_bindings set pending_kind = ?3, pending_payload = ?4, updated_at = ?5 \
                     where channel = ?1 and peer_id = ?2",
                    rusqlite::params![channel, peer_id, kind, payload, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 反查某 session 的所有远程绑定（一期通常 0 或 1 条）。
    pub fn list_bindings_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<RemoteBinding>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select channel, peer_id, account, account_name, session_id, context_token, \
                     pending_kind, pending_payload, updated_at \
                     from remote_bindings where session_id = ?1",
                )?;
                let rows = stmt.query_map([session_id], |r| {
                    Ok(RemoteBinding {
                        channel: r.get(0)?,
                        peer_id: r.get(1)?,
                        account: r.get(2)?,
                        account_name: r.get(3)?,
                        session_id: r.get(4)?,
                        context_token: r.get(5)?,
                        pending_kind: r.get(6)?,
                        pending_payload: r.get(7)?,
                        updated_at: r.get(8)?,
                    })
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 列出所有绑定（供本地 UI 查看远程当前会话映射）。
    pub fn list_all_bindings(&self) -> Result<Vec<RemoteBinding>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select channel, peer_id, account, account_name, session_id, context_token, \
                     pending_kind, pending_payload, updated_at from remote_bindings \
                     order by updated_at desc",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(RemoteBinding {
                        channel: r.get(0)?,
                        peer_id: r.get(1)?,
                        account: r.get(2)?,
                        account_name: r.get(3)?,
                        session_id: r.get(4)?,
                        context_token: r.get(5)?,
                        pending_kind: r.get(6)?,
                        pending_payload: r.get(7)?,
                        updated_at: r.get(8)?,
                    })
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn set_channel(
        &self,
        channel: &str,
        enabled: bool,
        config_json: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        let status = if enabled { "connected" } else { "disconnected" };
        self.set_channel_status(channel, status, config_json, None, now)
    }

    pub fn set_channel_status(
        &self,
        channel: &str,
        status: &str,
        config_json: Option<&str>,
        last_error: Option<&str>,
        now: &str,
    ) -> Result<(), String> {
        let enabled = matches!(status, "connecting" | "connected" | "error");
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into remote_channels (channel, enabled, status, config_json, last_error, updated_at) \
                     values (?1, ?2, ?3, ?4, ?5, ?6) \
                     on conflict(channel) do update set \
                     enabled = excluded.enabled, \
                     status = excluded.status, \
                     config_json = excluded.config_json, \
                     last_error = excluded.last_error, \
                     updated_at = excluded.updated_at",
                    rusqlite::params![channel, enabled as i64, status, config_json, last_error, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 置「待认领 owner」标记（配对成功后置 true：下一个入站 peer 自动认领为 owner）。
    pub fn set_awaiting_owner(&self, channel: &str, awaiting: bool) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update remote_channels set awaiting_owner = ?2 where channel = ?1",
                    rusqlite::params![channel, awaiting as i64],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 原子地读取并清除「待认领 owner」标记：为 true 时返回 true 并清零，否则返回 false。
    /// 用于配对后让第一个入站 peer 自动入白名单（owner 即扫码人）。
    pub fn take_awaiting_owner(&self, channel: &str) -> Result<bool, String> {
        self.db
            .with_connection(|c| {
                let cur: i64 = c
                    .query_row(
                        "select awaiting_owner from remote_channels where channel = ?1",
                        rusqlite::params![channel],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if cur != 0 {
                    c.execute(
                        "update remote_channels set awaiting_owner = 0 where channel = ?1",
                        rusqlite::params![channel],
                    )?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            })
            .map_err(|e| e.to_string())
    }

    pub fn list_channels(&self) -> Result<Vec<RemoteChannelConfig>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select channel, enabled, status, config_json, last_error, updated_at from remote_channels \
                     order by channel asc",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok(RemoteChannelConfig {
                        channel: r.get(0)?,
                        enabled: r.get::<_, i64>(1)? != 0,
                        status: r.get(2)?,
                        config_json: r.get(3)?,
                        last_error: r.get(4)?,
                        updated_at: r.get(5)?,
                    })
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppDatabase;
    use std::sync::Arc;

    /// 临时文件库（与既有 store 测试一致，本仓库测试不使用 :memory:）。
    pub(super) fn store() -> RemoteStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-remote-test-{nanos}.sqlite3"));
        let db = Arc::new(AppDatabase::open(path).unwrap());
        RemoteStore::open(db).unwrap()
    }

    #[test]
    fn allowlist_add_list_remove() {
        let s = store();
        assert!(!s.is_allowed("wechat", "peerA").unwrap());
        s.add_peer("wechat", "peerA", Some("我自己"), "2026-06-07T00:00:00Z")
            .unwrap();
        assert!(s.is_allowed("wechat", "peerA").unwrap());
        let list = s.list_allowlist().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].peer_id, "peerA");
        assert_eq!(list[0].label.as_deref(), Some("我自己"));
        s.remove_peer("wechat", "peerA").unwrap();
        assert!(!s.is_allowed("wechat", "peerA").unwrap());
    }

    #[test]
    fn binding_resolve_and_pending() {
        let s = store();
        assert_eq!(s.get_binding("wechat", "peerA").unwrap(), None);
        s.set_binding(
            "wechat",
            "peerA",
            Some("acct"),
            None,
            "sess1",
            Some("ctx1"),
            "t0",
        )
        .unwrap();
        let b = s.get_binding("wechat", "peerA").unwrap().unwrap();
        assert_eq!(b.session_id, "sess1");
        assert_eq!(b.context_token.as_deref(), Some("ctx1"));
        assert_eq!(b.pending_kind, None);
        // 切换当前 session
        s.set_binding(
            "wechat",
            "peerA",
            Some("acct"),
            None,
            "sess2",
            Some("ctx1"),
            "t1",
        )
        .unwrap();
        assert_eq!(
            s.get_binding("wechat", "peerA")
                .unwrap()
                .unwrap()
                .session_id,
            "sess2"
        );
        // 写/清暂停态
        s.set_pending(
            "wechat",
            "peerA",
            Some("permission"),
            Some("{\"toolCallId\":\"tc1\"}"),
            "t2",
        )
        .unwrap();
        let b = s.get_binding("wechat", "peerA").unwrap().unwrap();
        assert_eq!(b.pending_kind.as_deref(), Some("permission"));
        assert_eq!(
            b.pending_payload.as_deref(),
            Some("{\"toolCallId\":\"tc1\"}")
        );
        s.set_pending("wechat", "peerA", None, None, "t3").unwrap();
        assert_eq!(
            s.get_binding("wechat", "peerA")
                .unwrap()
                .unwrap()
                .pending_kind,
            None
        );
    }

    #[test]
    fn binding_account_name_roundtrip() {
        let s = store();
        s.set_binding("telegram", "42", None, Some("alice"), "sess1", None, "t0")
            .unwrap();

        let b = s.get_binding("telegram", "42").unwrap().unwrap();
        assert_eq!(b.account_name.as_deref(), Some("alice"));

        s.set_binding(
            "telegram",
            "42",
            None,
            Some("alice-new"),
            "sess1",
            None,
            "t1",
        )
        .unwrap();

        let b = s.get_binding("telegram", "42").unwrap().unwrap();
        assert_eq!(b.account_name.as_deref(), Some("alice-new"));
    }

    #[test]
    fn channel_config_upsert_and_enabled_list() {
        let s = store();
        assert!(s.list_channels().unwrap().is_empty());
        s.set_channel("wechat", true, Some("{\"appId\":\"x\"}"), "t0")
            .unwrap();
        let chans = s.list_channels().unwrap();
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].channel, "wechat");
        assert!(chans[0].enabled);
        assert_eq!(chans[0].config_json.as_deref(), Some("{\"appId\":\"x\"}"));
        s.set_channel("wechat", false, Some("{\"appId\":\"x\"}"), "t1")
            .unwrap();
        assert!(!s.list_channels().unwrap()[0].enabled);
    }

    #[test]
    fn channel_status_and_error_roundtrip() {
        let s = store();
        s.set_channel_status(
            "telegram",
            "connected",
            Some("{\"baseUrl\":\"x\"}"),
            None,
            "t0",
        )
        .unwrap();
        let chans = s.list_channels().unwrap();
        assert_eq!(chans.len(), 1);
        assert_eq!(chans[0].channel, "telegram");
        assert!(chans[0].enabled);
        assert_eq!(chans[0].status, "connected");
        assert_eq!(chans[0].last_error, None);

        s.set_channel_status(
            "telegram",
            "error",
            Some("{\"baseUrl\":\"x\"}"),
            Some("poll failed"),
            "t1",
        )
        .unwrap();
        let channel = s.list_channels().unwrap().remove(0);
        assert!(channel.enabled);
        assert_eq!(channel.status, "error");
        assert_eq!(channel.last_error.as_deref(), Some("poll failed"));

        s.set_channel_status(
            "telegram",
            "paused",
            Some("{\"baseUrl\":\"x\"}"),
            None,
            "t2",
        )
        .unwrap();
        let channel = s.list_channels().unwrap().remove(0);
        assert!(!channel.enabled);
        assert_eq!(channel.status, "paused");
        assert_eq!(channel.config_json.as_deref(), Some("{\"baseUrl\":\"x\"}"));
        assert_eq!(channel.last_error, None);
    }

    #[test]
    fn awaiting_owner_take_is_one_shot() {
        let s = store();
        s.set_channel("wechat", true, None, "t0").unwrap();
        assert!(!s.take_awaiting_owner("wechat").unwrap()); // 默认 false
        s.set_awaiting_owner("wechat", true).unwrap();
        assert!(s.take_awaiting_owner("wechat").unwrap()); // 首次 true
        assert!(!s.take_awaiting_owner("wechat").unwrap()); // 已清零
    }
}
