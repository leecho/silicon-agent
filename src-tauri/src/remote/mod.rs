//! 远程接入：把 IM 平台接成 agent 远程前端。connector 同步、跑专用线程，
//! 引擎与工具仍全程本地执行。详见 docs/04-specs/2026-06-07-remote-im-connector-design.md。

pub mod channels;
pub mod commands;
pub mod connector;
pub mod format;
pub mod http;
pub mod router;
pub mod store;

pub use connector::{Connector, InboundMessage, OutItem, PeerRef};
pub use router::{RemoteEngine, RemotePending, RemoteRouter};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::engine::event::AgentStreamEvent;

/// 远程接入 token 密钥文件存储（{app_data_dir}/remote.secret，按 channel 作 key，0600）。
pub fn open_secret_store(
    app: &tauri::AppHandle,
) -> Result<crate::provider::FileSecretStore, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("resolve app data dir: {err}"))?;
    Ok(crate::provider::FileSecretStore::new(
        dir.join("remote.secret"),
    ))
}

/// 出站任务：把一组 OutItem 发给某 peer。
struct Outbound {
    peer: PeerRef,
    items: Vec<OutItem>,
}

/// 远程枢纽：持有 connector、出站发送线程句柄、每会话节流态。
/// on_event 由引擎 emitter 调用（引擎线程），只做查绑定 + 入队，不阻塞网络。
pub struct RemoteHub {
    store: Arc<RemoteStore>,
    /// channel → 出站发送端。
    senders: Mutex<HashMap<String, std::sync::mpsc::Sender<Outbound>>>,
    /// channel → connector（供反查 max_len、typing）。
    connectors: Mutex<HashMap<String, Arc<dyn Connector>>>,
    /// session_id → 上次进度发送毫秒（节流）。
    last_progress: Mutex<HashMap<String, u64>>,
    /// 已起轮询线程的 channel（防重复启动，如重新配对时）。
    polling: Mutex<std::collections::HashSet<String>>,
    /// channel → 接收线程 shutdown 标记。pause/disconnect 通过它请求 connector 退出。
    shutdowns: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 当前由远程入口驱动的 session。只有这些 run 的事件会回发到 IM。
    remote_runs: Mutex<std::collections::HashSet<String>>,
}

impl RemoteHub {
    pub fn new(store: Arc<RemoteStore>) -> Self {
        Self {
            store,
            senders: Mutex::new(HashMap::new()),
            connectors: Mutex::new(HashMap::new()),
            last_progress: Mutex::new(HashMap::new()),
            polling: Mutex::new(std::collections::HashSet::new()),
            shutdowns: Mutex::new(HashMap::new()),
            remote_runs: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// 注册一个 connector：存句柄并起专用出站发送线程（串行消费该 channel 的 mpsc）。
    pub fn register(&self, connector: Arc<dyn Connector>) {
        let channel = connector.channel().to_string();
        let (tx, rx) = std::sync::mpsc::channel::<Outbound>();
        let conn = connector.clone();
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                if let Err(e) = conn.send(&job.peer, &job.items) {
                    eprintln!("[remote] 发送失败 channel={}：{e}", conn.channel());
                }
            }
        });
        self.senders.lock().unwrap().insert(channel.clone(), tx);
        self.connectors.lock().unwrap().insert(channel, connector);
    }

    pub fn connector(&self, channel: &str) -> Option<Arc<dyn Connector>> {
        self.connectors.lock().unwrap().get(channel).cloned()
    }

    /// 停止某个 channel 的运行时句柄。配置和密钥由 command/store 负责处理。
    pub fn stop_channel(&self, channel: &str) -> bool {
        let mut stopped = false;
        if let Some(shutdown) = self.shutdowns.lock().unwrap().remove(channel) {
            shutdown.store(true, Ordering::Relaxed);
            stopped = true;
        }
        if self.polling.lock().unwrap().remove(channel) {
            stopped = true;
        }
        if self.senders.lock().unwrap().remove(channel).is_some() {
            stopped = true;
        }
        if self.connectors.lock().unwrap().remove(channel).is_some() {
            stopped = true;
        }
        stopped
    }

    #[cfg(test)]
    fn mark_polling_for_test(&self, channel: &str) -> bool {
        self.polling.lock().unwrap().insert(channel.to_string())
    }

    fn enqueue(&self, channel: &str, peer: PeerRef, items: Vec<OutItem>) {
        if let Some(tx) = self.senders.lock().unwrap().get(channel) {
            let _ = tx.send(Outbound { peer, items });
        }
    }

    pub fn begin_remote_run(&self, session_id: &str) {
        self.remote_runs
            .lock()
            .unwrap()
            .insert(session_id.to_string());
    }

    pub fn end_remote_run(&self, session_id: &str) {
        self.remote_runs.lock().unwrap().remove(session_id);
    }

    fn is_remote_run(&self, session_id: &str) -> bool {
        self.remote_runs.lock().unwrap().contains(session_id)
    }

    /// 引擎事件分发入口。无远程绑定的会话直接返回（本地会话零开销）。
    /// 一期：message_completed 发完整回复（分段）；tool_call 节流进度；message_failed 发错误。
    pub fn on_event(&self, event: AgentStreamEvent) {
        if !self.is_remote_run(&event.session_id) {
            return;
        }
        let Some((channel, peer)) = self.peer_for_session(&event.session_id) else {
            return;
        };
        let conn = match self.connector(&channel) {
            Some(c) => c,
            None => return,
        };
        match event.kind.as_str() {
            "message_completed" => {
                if let Some(text) = event.text.as_deref() {
                    if !text.trim().is_empty() {
                        let chunks = format::segment_text(text, conn.max_len());
                        self.enqueue(
                            &channel,
                            peer,
                            chunks.into_iter().map(OutItem::Text).collect(),
                        );
                    }
                }
            }
            "tool_call" => {
                let now = now_ms();
                let mut lp = self.last_progress.lock().unwrap();
                let last = lp.get(&event.session_id).copied();
                if format::should_emit_progress(last, now, 3000) {
                    lp.insert(event.session_id.clone(), now);
                    drop(lp);
                    // 标签来自 Tool::label()（经事件带出）；缺失时回退工具名，再回退「处理中」。
                    let label = event
                        .tool_label
                        .clone()
                        .or_else(|| event.tool_name.clone())
                        .unwrap_or_else(|| "处理中".to_string());
                    self.enqueue(&channel, peer, vec![OutItem::Text(format!("🔧 {label}…"))]);
                }
            }
            "message_failed" => {
                let err = event.text.as_deref().unwrap_or("执行失败");
                self.enqueue(&channel, peer, vec![OutItem::Text(format!("⚠️ {err}"))]);
            }
            _ => {}
        }
    }

    /// 引擎暂停后调用：渲染编号提示、写 binding 暂停态、推 IM。
    pub fn notify_pending(&self, session_id: &str, pending: &RemotePending, now: &str) {
        let Some((channel, peer)) = self.peer_for_session(session_id) else {
            return;
        };
        let (kind, payload, text) = match pending {
            RemotePending::Permission {
                tool_call_id,
                tool_name,
                input,
            } => {
                let p = crate::session::PendingPermission {
                    session_id: session_id.to_string(),
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    input: input.clone(),
                };
                (
                    "permission",
                    serde_json::json!({ "toolCallId": tool_call_id }).to_string(),
                    format::render_permission(&p),
                )
            }
            RemotePending::Plan {
                tool_call_id,
                plan_text,
            } => (
                "plan",
                serde_json::json!({ "toolCallId": tool_call_id }).to_string(),
                format::render_plan(plan_text),
            ),
            RemotePending::Ask {
                tool_call_id,
                questions,
            } => {
                // 一期取首题。
                let q = &questions[0];
                (
                    "ask",
                    serde_json::json!({
                        "toolCallId": tool_call_id,
                        "options": q.options.len(),
                        "multi": q.multi_select,
                        "labels": q.options,
                    })
                    .to_string(),
                    format::render_ask_question(q, 0, questions.len()),
                )
            }
        };
        let _ = self
            .store
            .set_pending(&channel, &peer.peer_id, Some(kind), Some(&payload), now);
        self.enqueue(&channel, peer, vec![OutItem::Text(text)]);
    }

    /// 起一个 connector 的入站接收线程：注册（含出站发送线程）→ 跑 `connector.run(sink)`。
    /// 接收策略（拉/推、退避、重连）封在 connector 内部；Hub 只提供 sink = `router.handle`。
    /// 同一 channel 已起则跳过并返回 false（幂等）；首次启动返回 true。
    pub fn start_polling(
        self: &Arc<Self>,
        connector: Arc<dyn Connector>,
        router: Arc<RemoteRouter>,
    ) -> bool {
        let channel = connector.channel().to_string();
        if !self.polling.lock().unwrap().insert(channel) {
            return false;
        }
        self.register(connector.clone());
        let shutdown = Arc::new(AtomicBool::new(false));
        self.shutdowns
            .lock()
            .unwrap()
            .insert(connector.channel().to_string(), shutdown.clone());
        let conn_for_sink = connector.clone();
        std::thread::spawn(move || {
            let sink = |msg: crate::remote::connector::InboundMessage| {
                if let Err(e) = router.handle(&msg, conn_for_sink.as_ref()) {
                    eprintln!("[remote] 处理入站失败：{e}");
                }
            };
            connector.run(&sink, shutdown.as_ref());
        });
        true
    }

    /// 反查 session 的远程 peer。一期线性扫描绑定表（量小）。
    fn peer_for_session(&self, session_id: &str) -> Option<(String, PeerRef)> {
        let bindings = self.store.list_bindings_for_session(session_id).ok()?;
        let b = bindings.into_iter().next()?;
        Some((
            b.channel.clone(),
            PeerRef {
                channel: b.channel,
                account: b.account,
                peer_id: b.peer_id,
                context_token: b.context_token,
            },
        ))
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::connector::fake::FakeConnector;
    use crate::storage::AppDatabase;

    fn temp_store() -> Arc<RemoteStore> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-remote-hub-test-{nanos}.sqlite3"));
        let db = Arc::new(AppDatabase::open(path).unwrap());
        Arc::new(RemoteStore::open(db).unwrap())
    }

    #[test]
    fn stop_channel_removes_runtime_handles_and_allows_restart() {
        let hub = Arc::new(RemoteHub::new(temp_store()));
        let connector = Arc::new(FakeConnector::new("telegram"));
        hub.register(connector.clone());
        hub.mark_polling_for_test("telegram");

        assert!(hub.connector("telegram").is_some());
        assert!(hub.stop_channel("telegram"));
        assert!(hub.connector("telegram").is_none());
        assert!(!hub.stop_channel("telegram"));
        assert!(hub.mark_polling_for_test("telegram"));
    }

    #[test]
    fn local_run_for_bound_session_does_not_send_remote_reply() {
        let store = temp_store();
        store
            .set_binding(
                "telegram",
                "42",
                None,
                Some("Alice"),
                "sess-local",
                None,
                "t0",
            )
            .unwrap();
        let hub = Arc::new(RemoteHub::new(store));
        let connector = Arc::new(FakeConnector::new("telegram"));
        hub.register(connector.clone());

        hub.on_event(completed_event("sess-local", "本地回复"));
        wait_for_sent_count(&connector, 0);

        assert!(connector.sent_texts().is_empty());
    }

    #[test]
    fn remote_run_for_bound_session_sends_remote_reply() {
        let store = temp_store();
        store
            .set_binding(
                "telegram",
                "42",
                None,
                Some("Alice"),
                "sess-remote",
                None,
                "t0",
            )
            .unwrap();
        let hub = Arc::new(RemoteHub::new(store));
        let connector = Arc::new(FakeConnector::new("telegram"));
        hub.register(connector.clone());
        hub.begin_remote_run("sess-remote");

        hub.on_event(completed_event("sess-remote", "远程回复"));
        wait_for_sent_count(&connector, 1);

        assert_eq!(connector.sent_texts(), vec!["远程回复"]);
    }

    fn completed_event(session_id: &str, text: &str) -> AgentStreamEvent {
        AgentStreamEvent {
            kind: "message_completed".into(),
            session_id: session_id.into(),
            message_id: "msg1".into(),
            sequence: 1,
            text: Some(text.into()),
            status: None,
            tool_name: None,
            tool_label: None,
            tool_call_id: None,
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: "t0".into(),
        }
    }

    fn wait_for_sent_count(connector: &FakeConnector, expected: usize) {
        for _ in 0..20 {
            if connector.sent.lock().unwrap().len() == expected {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// RemoteEngine 实测实现：持 AppHandle，按需取托管的 AppState 走与本地命令完全相同的
/// spawn_* 执行路径（复用 run_registry 锁 + engine.resume + 暂停点推送）。
pub struct AppStateEngine {
    pub app: tauri::AppHandle,
}

impl RemoteEngine for AppStateEngine {
    fn drive_message(&self, session_id: &str, text: &str) -> Result<(), String> {
        use tauri::Manager;
        // 远程驱动不做乐观 UI 对账，丢弃入队/起跑布尔返回。
        self.app
            .state::<crate::app_state::AppState>()
            .coordinator
            .spawn_user_message_with_origin(session_id, text, crate::app_state::RunOrigin::Remote)
            .map(|_| ())
    }
    fn drive_permission(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), String> {
        use tauri::Manager;
        self.app
            .state::<crate::app_state::AppState>()
            .coordinator
            .spawn_permission_decision_with_origin(
                session_id,
                tool_call_id,
                approved,
                crate::app_state::RunOrigin::Remote,
            )
    }
    fn drive_ask(
        &self,
        session_id: &str,
        tool_call_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), String> {
        use tauri::Manager;
        self.app
            .state::<crate::app_state::AppState>()
            .coordinator
            .spawn_ask_response_with_origin(
                session_id,
                tool_call_id,
                answers,
                crate::app_state::RunOrigin::Remote,
            )
    }
    fn drive_plan(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
        comment: Option<String>,
    ) -> Result<(), String> {
        use tauri::Manager;
        self.app
            .state::<crate::app_state::AppState>()
            .coordinator
            .spawn_plan_decision_with_origin(
                session_id,
                tool_call_id,
                approved,
                comment,
                crate::app_state::RunOrigin::Remote,
            )
    }
    fn new_session(&self) -> Result<String, String> {
        use tauri::Manager;
        self.app
            .state::<crate::app_state::AppState>()
            .facade
            .create_remote_session()
    }
}

/// 按 enabled 启动各 channel 的 connector 线程。在 Tauri setup（app.manage 之后）调用。
/// 支持微信 / Telegram；缺配置/密钥的 channel 跳过并记录，不阻断启动。
pub fn start_enabled_channels(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::app_state::AppState>();
    let channels = state.remote_store.list_channels().unwrap_or_default();
    for cfg in channels.iter().filter(|c| c.enabled) {
        let r = match cfg.channel.as_str() {
            "wechat" => start_wechat_connector(app, &state),
            "telegram" => start_telegram_connector(app, &state),
            "dingtalk" => start_dingtalk_connector(app, &state),
            "feishu" => start_feishu_connector(app, &state),
            other => {
                eprintln!("[remote] 暂不支持的 channel：{other}");
                continue;
            }
        };
        if let Err(e) = r {
            eprintln!("[remote] {} connector 未启动：{e}", cfg.channel);
        }
    }
}

/// 构建好 connector 后统一接线：建 router（白名单 + AppStateEngine 驱动）+ start_polling（幂等）。
fn spawn_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
    connector: Arc<dyn Connector>,
) {
    let channel = connector.channel().to_string();
    let router = Arc::new(RemoteRouter::new(
        state.remote_store.clone(),
        Arc::new(AppStateEngine { app: app.clone() }),
    ));
    if state.remote_hub.start_polling(connector, router) {
        eprintln!("[remote] {channel} connector 已启动");
    }
}

pub(crate) fn start_channel_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
    channel: &str,
) -> Result<(), String> {
    match channel {
        "wechat" => start_wechat_connector(app, state),
        "telegram" => start_telegram_connector(app, state),
        "dingtalk" => start_dingtalk_connector(app, state),
        "feishu" => start_feishu_connector(app, state),
        other => Err(format!("暂不支持的 channel：{other}")),
    }
}

/// 构建并启动微信 connector（幂等：Hub 内已起则跳过）。配对成功与启动共用。
fn start_wechat_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
) -> Result<(), String> {
    let cfg = state
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|c| c.channel == "wechat")
        .ok_or_else(|| "未找到 wechat channel 配置".to_string())?;
    let connector = build_wechat_connector(state, &cfg)?;
    spawn_connector(app, state, connector);
    Ok(())
}

/// 构建并启动 Telegram connector（幂等）。token（BotFather）入 secret store。
fn start_telegram_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
) -> Result<(), String> {
    let cfg = state
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|c| c.channel == "telegram");
    let base_url = cfg
        .and_then(|c| c.config_json)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("baseUrl").and_then(|x| x.as_str()).map(String::from))
        .unwrap_or_else(|| crate::remote::channels::telegram::DEFAULT_BASE_URL.to_string());
    let token = crate::remote::open_secret_store(&state.app)?
        .read("telegram")
        .unwrap_or_default();
    if token.trim().is_empty() {
        return Err("缺少 telegram bot token（请在远程接入里填写 BotFather 的 token）".into());
    }
    let connector = Arc::new(crate::remote::channels::Telegram::new(
        Arc::new(crate::remote::http::UreqHttp::new(45_000)),
        base_url,
        token,
    ));
    spawn_connector(app, state, connector);
    Ok(())
}

/// 构建并启动钉钉 Stream connector（幂等）。AppKey/robotCode 在 config_json；AppSecret 入 secret store。
pub(crate) fn start_dingtalk_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
) -> Result<(), String> {
    use crate::remote::channels::dingtalk;
    let cfg = state
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|c| c.channel == "dingtalk")
        .ok_or_else(|| "未找到 dingtalk channel 配置".to_string())?;
    let v: serde_json::Value = cfg
        .config_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let app_key = v
        .get("appKey")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let robot_code = v
        .get("robotCode")
        .and_then(|x| x.as_str())
        .map(String::from)
        .unwrap_or_else(|| app_key.clone());
    let api_base = v
        .get("apiBase")
        .and_then(|x| x.as_str())
        .unwrap_or(dingtalk::API_BASE)
        .to_string();
    let app_secret = crate::remote::open_secret_store(&state.app)?
        .read("dingtalk")
        .unwrap_or_default();
    if app_key.is_empty() || app_secret.trim().is_empty() {
        return Err("缺少钉钉 AppKey/AppSecret（请在远程接入里填写）".into());
    }
    let connector = Arc::new(dingtalk::DingTalk::new(
        Arc::new(crate::remote::http::UreqHttp::new(20_000)),
        api_base,
        app_key,
        app_secret,
        robot_code,
    ));
    spawn_connector(app, state, connector);
    Ok(())
}

/// 构建并启动飞书长连接 connector（幂等）。AppID 在 config_json；AppSecret 入 secret store。
pub(crate) fn start_feishu_connector(
    app: &tauri::AppHandle,
    state: &crate::app_state::AppState,
) -> Result<(), String> {
    use crate::remote::channels::feishu;
    let cfg = state
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|c| c.channel == "feishu");
    let v: serde_json::Value = cfg
        .and_then(|c| c.config_json)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let app_id = v
        .get("appId")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let domain = v
        .get("domain")
        .and_then(|x| x.as_str())
        .unwrap_or(feishu::DOMAIN)
        .to_string();
    let app_secret = crate::remote::open_secret_store(&state.app)?
        .read("feishu")
        .unwrap_or_default();
    if app_id.is_empty() || app_secret.trim().is_empty() {
        return Err("缺少飞书 AppID/AppSecret（请在远程接入里填写）".into());
    }
    let connector = Arc::new(feishu::Feishu::new(
        Arc::new(crate::remote::http::UreqHttp::new(20_000)),
        domain,
        app_id,
        app_secret,
    ));
    spawn_connector(app, state, connector);
    Ok(())
}

/// 序列化给前端的二维码（配对命令返回）。`qr_content` 是待编码深链，前端生成二维码图片让用户扫。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeDto {
    pub qrcode: String,
    pub qr_content: String,
}

/// 发起微信扫码配对：取二维码（同步返回给命令），并后台轮询状态、发 `remote_pairing_event`。
/// confirmed 时：存 bot_token、enable channel、置 awaiting_owner、启动 connector。
pub fn begin_wechat_pairing(app: &tauri::AppHandle) -> Result<QrCodeDto, String> {
    use crate::remote::channels::wechat_clawbot::{self, PairStatus};
    use tauri::{Emitter, Manager};

    let state = app.state::<crate::app_state::AppState>();
    // base url：沿用既有 channel 配置，否则默认。
    let base_url = state
        .remote_store
        .list_channels()
        .ok()
        .and_then(|cs| cs.into_iter().find(|c| c.channel == "wechat"))
        .and_then(|c| c.config_json)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("baseUrl").and_then(|x| x.as_str()).map(String::from))
        .unwrap_or_else(|| wechat_clawbot::DEFAULT_BASE_URL.to_string());

    let http = std::sync::Arc::new(crate::remote::http::UreqHttp::new(15_000));
    let qr = wechat_clawbot::request_qrcode(http.as_ref(), &base_url)?;
    let dto = QrCodeDto {
        qrcode: qr.qrcode.clone(),
        qr_content: qr.qr_content.clone(),
    };
    let _ = app.emit(
        "remote_pairing_event",
        serde_json::json!({ "channel": "wechat", "phase": "qr", "qrcode": qr.qrcode, "qrContent": qr.qr_content }),
    );

    // 后台轮询线程。
    let app2 = app.clone();
    let qrcode = qr.qrcode.clone();
    std::thread::spawn(move || {
        use tauri::Manager;
        let emit = |phase: &str, msg: Option<&str>| {
            let _ = app2.emit(
                "remote_pairing_event",
                serde_json::json!({ "channel": "wechat", "phase": phase, "message": msg }),
            );
        };
        // 最长 ~2 分钟（60 次 × 2s）。
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            match wechat_clawbot::poll_qrcode_status(http.as_ref(), &base_url, &qrcode) {
                Ok(PairStatus::Wait) => continue,
                Ok(PairStatus::Scanned) => emit("scanned", None),
                Ok(PairStatus::Expired) => {
                    emit("expired", None);
                    return;
                }
                Ok(PairStatus::Confirmed {
                    bot_token,
                    base_url: confirmed_base,
                    ..
                }) => {
                    let state = app2.state::<crate::app_state::AppState>();
                    let eff_base = confirmed_base.unwrap_or_else(|| base_url.clone());
                    // 1) 存 token
                    if let Ok(store) = crate::remote::open_secret_store(&state.app) {
                        let _ = store.set("wechat", &bot_token);
                    }
                    // 2) enable channel + 写 base url
                    let now = crate::app_state::now_string();
                    let cfg = serde_json::json!({ "baseUrl": eff_base }).to_string();
                    let _ = state.remote_store.set_channel_status(
                        "wechat",
                        "connecting",
                        Some(&cfg),
                        None,
                        &now,
                    );
                    // 3) 待认领 owner：下一个入站 peer 即扫码人
                    let _ = state.remote_store.set_awaiting_owner("wechat", true);
                    // 4) 启动 connector
                    if let Err(e) = start_wechat_connector(&app2, &state) {
                        let _ = state.remote_store.set_channel_status(
                            "wechat",
                            "error",
                            Some(&cfg),
                            Some(&e),
                            &crate::app_state::now_string(),
                        );
                        emit("error", Some(&e));
                        return;
                    }
                    let _ = state.remote_store.set_channel_status(
                        "wechat",
                        "connected",
                        Some(&cfg),
                        None,
                        &crate::app_state::now_string(),
                    );
                    emit("confirmed", None);
                    return;
                }
                Err(e) => {
                    eprintln!("[remote] 配对轮询错误：{e}");
                    // 瞬时错误继续重试。
                }
            }
        }
        emit("expired", None);
    });

    Ok(dto)
}

/// 从 channel 配置 + 密钥构造微信 ClawBot connector。
fn build_wechat_connector(
    state: &crate::app_state::AppState,
    cfg: &RemoteChannelConfig,
) -> Result<Arc<dyn Connector>, String> {
    let v: serde_json::Value = cfg
        .config_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let get = |k: &str, default: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or(default)
            .to_string()
    };
    let token = crate::remote::open_secret_store(&state.app)?
        .read("wechat")
        .unwrap_or_default();
    if token.trim().is_empty() {
        return Err("尚未配对微信（请在远程接入里扫码绑定）".into());
    }
    Ok(Arc::new(
        crate::remote::channels::WechatClawbot::with_token(
            // getupdates 是 ~30s 长轮询，读超时留足余量（45s），避免边界处超时。
            Arc::new(crate::remote::http::UreqHttp::new(45_000)),
            get(
                "baseUrl",
                crate::remote::channels::wechat_clawbot::DEFAULT_BASE_URL,
            ),
            token,
            get("account", ""),
        ),
    ))
}
pub use store::{AllowedPeer, RemoteBinding, RemoteChannelConfig, RemoteStore};
