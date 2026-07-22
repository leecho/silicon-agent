//! 钉钉 Stream 模式 connector（第一个 WS 平台）。
//!
//! 协议（[Stream 协议](https://open-dingtalk.github.io/developerpedia/docs/learn/stream/protocol/)）：
//! 1. `POST api.dingtalk.com/v1.0/gateway/connections/open {clientId, clientSecret, subscriptions}`
//!    → `{endpoint(wss), ticket(90s 单次)}`；连 `endpoint?ticket=…`。
//! 2. WS 收 **JSON 帧** `{type, headers:{topic, messageId}, data}`：
//!    - topic=`ping`(SYSTEM)：回 ack 并回显 `opaque`（心跳）。
//!    - topic=`/v1.0/im/bot/messages/get`(CALLBACK)：data 是 bot 消息 JSON 串 → InboundMessage；回 ack。
//!    每帧必须 ACK（回显 messageId），否则服务端超时重推。
//! 3. 回复走 **OpenAPI**（避开临时 sessionWebhook）：`robot/oToMessages/batchSend`，
//!    header `x-acs-dingtalk-access-token`；access_token 自管刷新。
//!
//! 接入：企业自建应用填 AppKey/AppSecret（非扫码）。
//!
//! 注：**WS 帧/消息字段未经真实账号联调**，按文档实现 + 容错解析，联调时按真实响应校正。

use std::sync::Arc;

use serde_json::Value;
use tungstenite::Message;

use crate::remote::connector::{Connector, InboundMessage, OutItem, PeerRef};
use crate::remote::http::HttpClient;

pub const API_BASE: &str = "https://api.dingtalk.com";
const BOT_TOPIC: &str = "/v1.0/im/bot/messages/get";

pub struct DingTalk {
    pub http: Arc<dyn HttpClient>,
    pub api_base: String,
    app_key: String,
    app_secret: String,
    /// 机器人 robotCode（自建应用一般 == appKey）；回复 batchSend 用。
    robot_code: String,
    /// access_token 缓存：(token, 过期毫秒时间戳)。
    token: std::sync::Mutex<Option<(String, u64)>>,
}

/// 一帧的处理动作（纯函数产出，便于单测）。
#[derive(Debug, PartialEq, Eq)]
pub enum FrameAction {
    /// 回一条 ack 文本，无入站消息（如心跳）。
    Ack(String),
    /// 回 ack 文本 + 投递一条入站消息。
    Message(String, InboundMessage),
    /// 忽略（未知 topic）。
    Ignore,
}

impl DingTalk {
    pub fn new(
        http: Arc<dyn HttpClient>,
        api_base: String,
        app_key: String,
        app_secret: String,
        robot_code: String,
    ) -> Self {
        Self {
            http,
            api_base,
            app_key,
            app_secret,
            robot_code,
            token: std::sync::Mutex::new(None),
        }
    }

    /// 取 WS 接入点：POST gateway/connections/open → (endpoint, ticket)。
    fn connections_open(&self) -> Result<(String, String), String> {
        let url = format!("{}/v1.0/gateway/connections/open", self.api_base);
        let body = serde_json::json!({
            "clientId": self.app_key,
            "clientSecret": self.app_secret,
            "ua": "silicon-worker-sdk-rust/0.1",
            "subscriptions": [
                { "type": "CALLBACK", "topic": BOT_TOPIC }
            ]
        });
        let resp = self.http.post_json(&url, &body.to_string(), &[])?;
        let v: Value =
            serde_json::from_str(&resp).map_err(|e| format!("parse connections/open: {e}"))?;
        let endpoint = v
            .get("endpoint")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("connections/open 无 endpoint：{resp}"))?;
        let ticket = v
            .get("ticket")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("connections/open 无 ticket：{resp}"))?;
        Ok((endpoint.to_string(), ticket.to_string()))
    }

    /// 取（带缓存、到期刷新）access_token。
    fn access_token(&self) -> Result<String, String> {
        {
            let guard = self.token.lock().unwrap();
            if let Some((t, exp)) = guard.as_ref() {
                if now_ms() + 60_000 < *exp {
                    return Ok(t.clone());
                }
            }
        }
        let url = format!("{}/v1.0/oauth2/accessToken", self.api_base);
        let body = serde_json::json!({ "appKey": self.app_key, "appSecret": self.app_secret });
        let resp = self.http.post_json(&url, &body.to_string(), &[])?;
        let v: Value =
            serde_json::from_str(&resp).map_err(|e| format!("parse accessToken: {e}"))?;
        let token = v
            .get("accessToken")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("accessToken 响应异常：{resp}"))?
            .to_string();
        let expire_in = v.get("expireIn").and_then(|x| x.as_u64()).unwrap_or(7200);
        *self.token.lock().unwrap() = Some((token.clone(), now_ms() + expire_in * 1000));
        Ok(token)
    }

    /// 单次建连 + 收帧循环（连接断开/出错即返回，由 run 重连）。
    fn connect_and_listen(
        &self,
        sink: &dyn Fn(InboundMessage),
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let (endpoint, ticket) = self.connections_open()?;
        let url = format!("{endpoint}?ticket={ticket}");
        let (mut socket, _) = tungstenite::connect(&url).map_err(|e| format!("ws connect: {e}"))?;
        eprintln!("[remote] 钉钉 WS 已连接");
        while !shutdown.load(Ordering::Relaxed) {
            let msg = socket.read().map_err(|e| format!("ws read: {e}"))?;
            match msg {
                Message::Text(txt) => match handle_frame(&txt) {
                    FrameAction::Ack(ack) => {
                        let _ = socket.send(Message::Text(ack.into()));
                    }
                    FrameAction::Message(ack, inbound) => {
                        sink(inbound);
                        let _ = socket.send(Message::Text(ack.into()));
                    }
                    FrameAction::Ignore => {}
                },
                Message::Ping(p) => {
                    let _ = socket.send(Message::Pong(p));
                }
                Message::Close(_) => return Ok(()),
                _ => {}
            }
        }
        let _ = socket.close(None);
        Ok(())
    }
}

impl Connector for DingTalk {
    fn channel(&self) -> &str {
        "dingtalk"
    }

    fn max_len(&self) -> usize {
        2000
    }

    fn run(&self, sink: &dyn Fn(InboundMessage), shutdown: &std::sync::atomic::AtomicBool) {
        use std::sync::atomic::Ordering;
        while !shutdown.load(Ordering::Relaxed) {
            if let Err(e) = self.connect_and_listen(sink, shutdown) {
                eprintln!("[remote] 钉钉连接断开，3s 后重连：{e}");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }

    fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String> {
        let token = self.access_token()?;
        let url = format!("{}/v1.0/robot/oToMessages/batchSend", self.api_base);
        let headers = [("x-acs-dingtalk-access-token", token.as_str())];
        for item in items {
            let OutItem::Text(text) = item;
            let msg_param = serde_json::json!({ "content": text }).to_string();
            let req = serde_json::json!({
                "robotCode": self.robot_code,
                "userIds": [ peer.peer_id ],
                "msgKey": "sampleText",
                "msgParam": msg_param,
            });
            let resp = self.http.post_json(&url, &req.to_string(), &headers)?;
            // batchSend 成功返回 processQueryKey；含 code/错误则报错。
            if let Ok(v) = serde_json::from_str::<Value>(&resp) {
                if v.get("code").is_some() && v.get("processQueryKey").is_none() {
                    return Err(format!("钉钉 batchSend 失败：{resp}"));
                }
            }
        }
        Ok(())
    }

    fn send_typing(&self, _peer: &PeerRef) -> Result<(), String> {
        // 钉钉无 typing 指示；空实现。
        Ok(())
    }
}

/// 处理一帧 JSON 文本，产出 ack + 可选入站消息（纯函数，便于单测）。
pub fn handle_frame(txt: &str) -> FrameAction {
    let Ok(v) = serde_json::from_str::<Value>(txt) else {
        return FrameAction::Ignore;
    };
    let topic = v
        .pointer("/headers/topic")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let message_id = v
        .pointer("/headers/messageId")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let data = v.get("data").and_then(|x| x.as_str()).unwrap_or("");

    if topic == "ping" {
        // 心跳：回显 opaque。
        let opaque = serde_json::from_str::<Value>(data)
            .ok()
            .and_then(|d| d.get("opaque").and_then(|x| x.as_str()).map(String::from))
            .unwrap_or_default();
        return FrameAction::Ack(ack_json(
            &message_id,
            &serde_json::json!({ "opaque": opaque }).to_string(),
        ));
    }
    if topic == BOT_TOPIC {
        let ack = ack_json(&message_id, "{\"status\":\"SUCCESS\"}");
        if let Some(inbound) = bot_message_to_inbound(data) {
            return FrameAction::Message(ack, inbound);
        }
        return FrameAction::Ack(ack);
    }
    // 其它 topic：回个 ack 让服务端别重推。
    FrameAction::Ack(ack_json(&message_id, "{}"))
}

/// 钉钉机器人文本消息（data JSON 串）→ 归一化入站。无文本/无发送者则 None。
fn bot_message_to_inbound(data: &str) -> Option<InboundMessage> {
    let v: Value = serde_json::from_str(data).ok()?;
    let peer_id = v.get("senderStaffId").and_then(|x| x.as_str())?;
    if peer_id.is_empty() {
        return None;
    }
    let text = v.pointer("/text/content").and_then(|x| x.as_str())?;
    if text.trim().is_empty() {
        return None;
    }
    Some(InboundMessage {
        channel: "dingtalk".into(),
        account: v
            .get("conversationId")
            .and_then(|x| x.as_str())
            .map(String::from),
        peer_id: peer_id.into(),
        peer_name: v
            .get("senderNick")
            .and_then(|x| x.as_str())
            .map(String::from),
        text: text.trim().into(),
        kind: "text".into(),
        context_token: None,
        received_at: crate::engine::now_string(),
    })
}

/// 构造 Stream ack 响应帧（回显 messageId）。
fn ack_json(message_id: &str, data: &str) -> String {
    serde_json::json!({
        "code": 200,
        "headers": { "messageId": message_id, "contentType": "application/json" },
        "message": "OK",
        "data": data,
    })
    .to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_frame_acks_with_opaque() {
        let frame = r#"{"type":"SYSTEM","headers":{"topic":"ping","messageId":"m1"},"data":"{\"opaque\":\"abc\"}"}"#;
        match handle_frame(frame) {
            FrameAction::Ack(ack) => {
                assert!(ack.contains("\"messageId\":\"m1\""));
                assert!(ack.contains("abc"));
            }
            other => panic!("expected Ack, got {other:?}"),
        }
    }

    #[test]
    fn bot_text_frame_yields_inbound_and_ack() {
        let inner = r#"{"senderStaffId":"u1","conversationId":"c1","senderNick":"小明","msgtype":"text","text":{"content":"你好"}}"#;
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "headers": { "topic": BOT_TOPIC, "messageId": "m2" },
            "data": inner,
        })
        .to_string();
        match handle_frame(&frame) {
            FrameAction::Message(ack, m) => {
                assert!(ack.contains("SUCCESS"));
                assert_eq!(m.peer_id, "u1");
                assert_eq!(m.text, "你好");
                assert_eq!(m.account.as_deref(), Some("c1"));
                assert_eq!(m.peer_name.as_deref(), Some("小明"));
                assert_eq!(m.channel, "dingtalk");
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn bot_non_text_frame_acks_without_inbound() {
        let inner = r#"{"senderStaffId":"u1","msgtype":"image"}"#;
        let frame = serde_json::json!({
            "headers": { "topic": BOT_TOPIC, "messageId": "m3" },
            "data": inner,
        })
        .to_string();
        assert!(matches!(handle_frame(&frame), FrameAction::Ack(_)));
    }

    #[test]
    fn send_builds_batchsend_and_succeeds() {
        use crate::remote::http::MockHttp;
        let http = Arc::new(MockHttp::new(vec![
            (
                "oauth2/accessToken".into(),
                "{\"accessToken\":\"AT\",\"expireIn\":7200}".into(),
            ),
            (
                "oToMessages/batchSend".into(),
                "{\"processQueryKey\":\"k\"}".into(),
            ),
        ]));
        let c = DingTalk::new(http, API_BASE.into(), "ak".into(), "as".into(), "rc".into());
        let peer = PeerRef {
            channel: "dingtalk".into(),
            account: Some("c1".into()),
            peer_id: "u1".into(),
            context_token: None,
        };
        c.send(&peer, &[OutItem::Text("回复".into())]).unwrap();
    }
}
