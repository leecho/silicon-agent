//! 入站路由：白名单闸门 → 命令 → 暂停态回复 → 普通消息驱动引擎。
//! 经 RemoteEngine trait 依赖引擎，便于单测。

use std::sync::Arc;

use crate::remote::connector::{Connector, InboundMessage, OutItem, PeerRef};
use crate::remote::format;
use crate::remote::store::RemoteStore;

/// 远程暂停态投影（Hub 据此渲染并写 binding 暂停态；router 据 binding 暂停态把回复路由回引擎）。
pub enum RemotePending {
    Permission {
        tool_call_id: String,
        tool_name: String,
        input: String,
    },
    Ask {
        tool_call_id: String,
        questions: Vec<crate::session::AskQuestion>,
    },
    Plan {
        tool_call_id: String,
        plan_text: String,
    },
}

/// 引擎驱动边界。实测包 AppState（见 AppStateEngine），单测用 fake。
/// 各方法与 AppState::spawn_* 一一对应，driver 内部走与本地命令完全相同的执行路径。
pub trait RemoteEngine: Send + Sync {
    fn drive_message(&self, session_id: &str, text: &str) -> Result<(), String>;
    fn drive_permission(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), String>;
    fn drive_ask(
        &self,
        session_id: &str,
        tool_call_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), String>;
    fn drive_plan(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
        comment: Option<String>,
    ) -> Result<(), String>;
    /// 新建一个会话，返回其 id。
    fn new_session(&self) -> Result<String, String>;
}

pub struct RemoteRouter {
    store: Arc<RemoteStore>,
    engine: Arc<dyn RemoteEngine>,
}

impl RemoteRouter {
    pub fn new(store: Arc<RemoteStore>, engine: Arc<dyn RemoteEngine>) -> Self {
        Self { store, engine }
    }

    fn peer_ref(&self, m: &InboundMessage) -> PeerRef {
        PeerRef {
            channel: m.channel.clone(),
            account: m.account.clone(),
            peer_id: m.peer_id.clone(),
            context_token: m.context_token.clone(),
        }
    }

    fn reply(&self, conn: &dyn Connector, peer: &PeerRef, text: &str) {
        let _ = conn.send(peer, &[OutItem::Text(text.to_string())]);
    }

    /// 解析 (channel, peer) 当前 session：无绑定则建会话并绑定；有则刷新 context_token。
    fn current_session(&self, m: &InboundMessage, now: &str) -> Result<String, String> {
        if let Some(b) = self.store.get_binding(&m.channel, &m.peer_id)? {
            self.store.set_binding(
                &m.channel,
                &m.peer_id,
                m.account.as_deref(),
                m.peer_name.as_deref(),
                &b.session_id,
                m.context_token.as_deref(),
                now,
            )?;
            Ok(b.session_id)
        } else {
            let sid = self.engine.new_session()?;
            self.store.set_binding(
                &m.channel,
                &m.peer_id,
                m.account.as_deref(),
                m.peer_name.as_deref(),
                &sid,
                m.context_token.as_deref(),
                now,
            )?;
            Ok(sid)
        }
    }

    pub fn handle(&self, m: &InboundMessage, conn: &dyn Connector) -> Result<(), String> {
        let peer = self.peer_ref(m);
        let now = m.received_at.clone();

        // 1. 白名单闸门——远程触发本地工具执行的唯一入口。
        if !self.store.is_allowed(&m.channel, &m.peer_id)? {
            // 配对后「待认领 owner」：第一个入站 peer 自动认领为 owner 入白名单（扫码人即 owner）。
            if self.store.take_awaiting_owner(&m.channel)? {
                self.store
                    .add_peer(&m.channel, &m.peer_id, Some("owner(扫码绑定)"), &now)?;
                eprintln!("[remote] owner 自动入白名单：{}/{}", m.channel, m.peer_id);
            } else {
                eprintln!("[remote] 拒绝非白名单 peer：{}/{}", m.channel, m.peer_id);
                return Ok(());
            }
        }

        let text = m.text.trim();

        if self
            .store
            .list_channels()?
            .into_iter()
            .any(|cfg| cfg.channel == m.channel && cfg.status == "paused")
        {
            self.reply(
                conn,
                &peer,
                "远程接入已暂停，请在桌面端恢复连接后再发送消息。",
            );
            return Ok(());
        }

        // 2. 命令优先（/ 前缀），便于卡住时用 /new 重置暂停态。
        if let Some(rest) = text.strip_prefix('/') {
            return self.handle_command(m, &peer, conn, rest, &now);
        }

        // 3. 暂停态：有 pending_kind 则把本条当编号回复解析。
        if let Some(b) = self.store.get_binding(&m.channel, &m.peer_id)? {
            if let Some(kind) = b.pending_kind.as_deref() {
                return self.handle_pending_reply(m, &peer, conn, &b.session_id, kind, text, &now);
            }
        }

        // 4. 普通消息 → 驱动引擎（完全等同本地）。
        let sid = self.current_session(m, &now)?;
        self.engine.drive_message(&sid, text)?;
        Ok(())
    }

    fn handle_command(
        &self,
        m: &InboundMessage,
        peer: &PeerRef,
        conn: &dyn Connector,
        rest: &str,
        now: &str,
    ) -> Result<(), String> {
        let mut parts = rest.split_whitespace();
        match parts.next() {
            Some("new") => {
                let sid = self.engine.new_session()?;
                self.store.set_binding(
                    &m.channel,
                    &m.peer_id,
                    m.account.as_deref(),
                    m.peer_name.as_deref(),
                    &sid,
                    m.context_token.as_deref(),
                    now,
                )?;
                self.reply(conn, peer, "已新建会话，可以开始了。");
            }
            Some("help") | None => {
                self.reply(
                    conn,
                    peer,
                    "命令：/new 新建会话，/help 帮助。直接发消息即派任务。",
                );
            }
            _ => {
                self.reply(conn, peer, "未知命令，发 /help 看可用命令。");
            }
        }
        Ok(())
    }

    fn handle_pending_reply(
        &self,
        m: &InboundMessage,
        peer: &PeerRef,
        conn: &dyn Connector,
        session_id: &str,
        kind: &str,
        text: &str,
        now: &str,
    ) -> Result<(), String> {
        let payload = self
            .store
            .get_binding(&m.channel, &m.peer_id)?
            .and_then(|b| b.pending_payload)
            .unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
        let tool_call_id = v
            .get("toolCallId")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        match kind {
            "permission" => {
                let Some(approved) = format::parse_yes_no(text) else {
                    self.reply(conn, peer, "请回复 1 批准 / 2 拒绝。");
                    return Ok(());
                };
                self.store
                    .set_pending(&m.channel, &m.peer_id, None, None, now)?;
                self.engine
                    .drive_permission(session_id, &tool_call_id, approved)?;
            }
            "plan" => {
                let Some(approved) = format::parse_yes_no(text) else {
                    self.reply(conn, peer, "请回复 1 批准执行 / 2 拒绝。");
                    return Ok(());
                };
                self.store
                    .set_pending(&m.channel, &m.peer_id, None, None, now)?;
                self.engine
                    .drive_plan(session_id, &tool_call_id, approved, None)?;
            }
            "ask" => {
                let count = v.get("options").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                let multi = v.get("multi").and_then(|x| x.as_bool()).unwrap_or(false);
                let labels: Vec<String> = v
                    .get("labels")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| s.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let Some(idxs) = format::parse_choice(text, count, multi) else {
                    self.reply(conn, peer, "请回复选项编号（多选用逗号分隔）。");
                    return Ok(());
                };
                let answers: Vec<Vec<String>> = vec![idxs
                    .iter()
                    .filter_map(|&i| labels.get(i).cloned())
                    .collect()];
                self.store
                    .set_pending(&m.channel, &m.peer_id, None, None, now)?;
                self.engine.drive_ask(session_id, &tool_call_id, answers)?;
            }
            _ => {
                self.store
                    .set_pending(&m.channel, &m.peer_id, None, None, now)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::connector::fake::FakeConnector;
    use crate::remote::connector::InboundMessage;
    use crate::remote::store::RemoteStore;
    use crate::storage::AppDatabase;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeEngine {
        driven: Mutex<Vec<(String, String)>>, // (session_id, text)
        permission: Mutex<Vec<(String, String, bool)>>, // (session, tool_call_id, approved)
        ask: Mutex<Vec<(String, String, Vec<Vec<String>>)>>, // (session, tool_call_id, answers)
        plan: Mutex<Vec<(String, String, bool)>>, // (session, tool_call_id, approved)
    }
    impl RemoteEngine for FakeEngine {
        fn drive_message(&self, session_id: &str, text: &str) -> Result<(), String> {
            self.driven
                .lock()
                .unwrap()
                .push((session_id.into(), text.into()));
            Ok(())
        }
        fn drive_permission(&self, s: &str, tc: &str, a: bool) -> Result<(), String> {
            self.permission
                .lock()
                .unwrap()
                .push((s.into(), tc.into(), a));
            Ok(())
        }
        fn drive_ask(&self, s: &str, tc: &str, ans: Vec<Vec<String>>) -> Result<(), String> {
            self.ask.lock().unwrap().push((s.into(), tc.into(), ans));
            Ok(())
        }
        fn drive_plan(
            &self,
            s: &str,
            tc: &str,
            approved: bool,
            _comment: Option<String>,
        ) -> Result<(), String> {
            self.plan
                .lock()
                .unwrap()
                .push((s.into(), tc.into(), approved));
            Ok(())
        }
        fn new_session(&self) -> Result<String, String> {
            Ok("new-sess".into())
        }
    }

    fn temp_store() -> Arc<RemoteStore> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-router-test-{nanos}.sqlite3"));
        let db = Arc::new(AppDatabase::open(path).unwrap());
        Arc::new(RemoteStore::open(db).unwrap())
    }

    fn msg(peer: &str, text: &str) -> InboundMessage {
        InboundMessage {
            channel: "wechat".into(),
            account: None,
            peer_id: peer.into(),
            peer_name: None,
            text: text.into(),
            kind: "text".into(),
            context_token: Some("ctx".into()),
            received_at: "t0".into(),
        }
    }

    fn named_msg(peer: &str, peer_name: &str, text: &str) -> InboundMessage {
        InboundMessage {
            peer_name: Some(peer_name.into()),
            ..msg(peer, text)
        }
    }

    fn fixture() -> (
        RemoteRouter,
        Arc<RemoteStore>,
        Arc<FakeConnector>,
        Arc<FakeEngine>,
    ) {
        let store = temp_store();
        let conn = Arc::new(FakeConnector::new("wechat"));
        let engine = Arc::new(FakeEngine::default());
        let router = RemoteRouter::new(store.clone(), engine.clone());
        (router, store, conn, engine)
    }

    #[test]
    fn rejects_non_allowlisted_peer() {
        let (router, _store, conn, engine) = fixture();
        router
            .handle(&msg("stranger", "hi"), conn.as_ref())
            .unwrap();
        assert!(engine.driven.lock().unwrap().is_empty());
    }

    #[test]
    fn awaiting_owner_first_peer_auto_claimed_then_drives() {
        let (router, store, conn, engine) = fixture();
        // 配对后置标记；peer 不在白名单，但应被自动认领并驱动。
        store.set_channel("wechat", true, None, "t0").unwrap();
        store.set_awaiting_owner("wechat", true).unwrap();
        router
            .handle(&msg("owner1", "你好"), conn.as_ref())
            .unwrap();
        assert!(store.is_allowed("wechat", "owner1").unwrap()); // 已入白名单
        assert_eq!(engine.driven.lock().unwrap().len(), 1); // 已驱动
                                                            // 标记一次性：第二个陌生 peer 仍被拒。
        router
            .handle(&msg("stranger", "hi"), conn.as_ref())
            .unwrap();
        assert!(!store.is_allowed("wechat", "stranger").unwrap());
        assert_eq!(engine.driven.lock().unwrap().len(), 1);
    }

    #[test]
    fn allowlisted_plain_message_drives_engine() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        router
            .handle(&msg("me", "帮我整理代码"), conn.as_ref())
            .unwrap();
        let driven = engine.driven.lock().unwrap();
        assert_eq!(driven.len(), 1);
        assert_eq!(driven[0].1, "帮我整理代码");
    }

    #[test]
    fn inbound_peer_name_is_saved_as_binding_account_name() {
        let (router, store, conn, _engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();

        router
            .handle(&named_msg("me", "Alice", "帮我整理代码"), conn.as_ref())
            .unwrap();

        let binding = store.get_binding("wechat", "me").unwrap().unwrap();
        assert_eq!(binding.account_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn paused_channel_replies_without_driving_engine() {
        let (router, store, conn, engine) = fixture();
        store
            .set_channel_status("wechat", "paused", Some("{\"baseUrl\":\"x\"}"), None, "t0")
            .unwrap();
        store.add_peer("wechat", "me", None, "t0").unwrap();

        router
            .handle(&msg("me", "帮我整理代码"), conn.as_ref())
            .unwrap();

        assert!(engine.driven.lock().unwrap().is_empty());
        assert_eq!(
            conn.sent_texts(),
            vec!["远程接入已暂停，请在桌面端恢复连接后再发送消息。"]
        );
    }

    #[test]
    fn slash_new_creates_and_binds_session() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        router.handle(&msg("me", "/new"), conn.as_ref()).unwrap();
        assert_eq!(
            store
                .get_binding("wechat", "me")
                .unwrap()
                .unwrap()
                .session_id,
            "new-sess"
        );
        assert!(engine.driven.lock().unwrap().is_empty());
        assert!(!conn.sent_texts().is_empty());
    }

    #[test]
    fn pending_permission_numbered_reply_drives_decision() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        store
            .set_binding("wechat", "me", None, None, "sess1", Some("ctx"), "t0")
            .unwrap();
        store
            .set_pending(
                "wechat",
                "me",
                Some("permission"),
                Some("{\"toolCallId\":\"tc1\"}"),
                "t1",
            )
            .unwrap();
        // 合法编号回复 → 驱动决定 + 清暂停态
        router.handle(&msg("me", "1"), conn.as_ref()).unwrap();
        let p = engine.permission.lock().unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0], ("sess1".into(), "tc1".into(), true));
        assert_eq!(
            store
                .get_binding("wechat", "me")
                .unwrap()
                .unwrap()
                .pending_kind,
            None
        );
    }

    #[test]
    fn pending_permission_invalid_reply_reprompts_keeps_pending() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        store
            .set_binding("wechat", "me", None, None, "sess1", Some("ctx"), "t0")
            .unwrap();
        store
            .set_pending(
                "wechat",
                "me",
                Some("permission"),
                Some("{\"toolCallId\":\"tc1\"}"),
                "t1",
            )
            .unwrap();
        router.handle(&msg("me", "批准吧"), conn.as_ref()).unwrap();
        assert!(engine.permission.lock().unwrap().is_empty()); // 未驱动
        assert!(!conn.sent_texts().is_empty()); // 重发提示
        assert_eq!(
            store
                .get_binding("wechat", "me")
                .unwrap()
                .unwrap()
                .pending_kind
                .as_deref(),
            Some("permission") // 暂停态保留
        );
    }

    #[test]
    fn pending_ask_multi_reply_maps_labels() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        store
            .set_binding("wechat", "me", None, None, "sess1", Some("ctx"), "t0")
            .unwrap();
        // 3 选项多选；payload 带 toolCallId/options/multi/labels（与 Hub::notify_pending 写入一致）。
        store
            .set_pending(
                "wechat",
                "me",
                Some("ask"),
                Some(
                    "{\"toolCallId\":\"ask1\",\"options\":3,\"multi\":true,\
                     \"labels\":[\"A\",\"B\",\"C\"]}",
                ),
                "t1",
            )
            .unwrap();
        router.handle(&msg("me", "1,3"), conn.as_ref()).unwrap();
        let a = engine.ask.lock().unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].0, "sess1");
        assert_eq!(a[0].1, "ask1");
        assert_eq!(a[0].2, vec![vec!["A".to_string(), "C".to_string()]]);
        assert_eq!(
            store
                .get_binding("wechat", "me")
                .unwrap()
                .unwrap()
                .pending_kind,
            None
        );
    }

    #[test]
    fn pending_plan_reply_drives_plan() {
        let (router, store, conn, engine) = fixture();
        store.add_peer("wechat", "me", None, "t0").unwrap();
        store
            .set_binding("wechat", "me", None, None, "sess1", Some("ctx"), "t0")
            .unwrap();
        store
            .set_pending(
                "wechat",
                "me",
                Some("plan"),
                Some("{\"toolCallId\":\"plan1\"}"),
                "t1",
            )
            .unwrap();
        router.handle(&msg("me", "2"), conn.as_ref()).unwrap();
        let p = engine.plan.lock().unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0], ("sess1".into(), "plan1".into(), false));
    }
}
