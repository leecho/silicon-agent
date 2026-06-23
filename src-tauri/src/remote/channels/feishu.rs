//! 飞书 / Lark 长连接 connector（最重的平台：protobuf 帧 over WebSocket）。
//!
//! 协议（从官方 SDK larksuite/oapi-sdk-python 源码钉死）：
//! 1. `POST {domain}/callback/ws/endpoint {AppID, AppSecret}`（header locale:zh）
//!    → `{code:0, data:{URL}}`，URL 是 wss（query 带 device_id/service_id），直接连。
//! 2. WS 收 **protobuf Frame**（见 `Frame`）：`method`=FrameType（0=CONTROL/1=DATA），
//!    header `type`=event/card/ping/pong。
//!    - DATA + type=event：`payload` 是 JSON 事件（im.message.receive_v1）→ 入站；
//!      **回 ACK**：把同一 Frame 的 payload 换成 `{"code":200}` 再发回（headers 不变）。
//!    - 客户端周期发 ping 控制帧保活。
//! 3. 回复走 OpenAPI `im/v1/messages`（receive_id_type=open_id）+ tenant_access_token（刷新）。
//!
//! 接入：企业自建应用填 AppID/AppSecret（非扫码）；需开启长连接订阅 im.message.receive_v1。
//!
//! 注：**WS 帧/事件字段未经真实应用联调**，按 SDK 源码实现，联调时校正。proto2 required 字段在
//! prost 下零值会省略编码（ping 帧），若服务端严格校验可能影响保活；联调验证。

use std::net::TcpStream;
use std::sync::Arc;

use prost::Message as ProstMessage;
use serde_json::Value;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message as WsMessage;

use crate::remote::connector::{Connector, InboundMessage, OutItem, PeerRef};
use crate::remote::http::HttpClient;

pub const DOMAIN: &str = "https://open.feishu.cn";

const FRAME_CONTROL: i32 = 0;
const FRAME_DATA: i32 = 1;

/// frontier pbbp2 Frame（字段号来自官方 SDK pbbp2.proto）。
#[derive(Clone, PartialEq, ProstMessage)]
struct Frame {
    #[prost(uint64, tag = "1")]
    seq_id: u64,
    #[prost(uint64, tag = "2")]
    log_id: u64,
    #[prost(int32, tag = "3")]
    service: i32,
    #[prost(int32, tag = "4")]
    method: i32,
    #[prost(message, repeated, tag = "5")]
    headers: Vec<Header>,
    #[prost(string, optional, tag = "6")]
    payload_encoding: Option<String>,
    #[prost(string, optional, tag = "7")]
    payload_type: Option<String>,
    #[prost(bytes = "vec", optional, tag = "8")]
    payload: Option<Vec<u8>>,
    #[prost(string, optional, tag = "9")]
    log_id_new: Option<String>,
}

#[derive(Clone, PartialEq, ProstMessage)]
struct Header {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(string, tag = "2")]
    value: String,
}

impl Frame {
    fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.as_str())
    }
}

pub struct Feishu {
    pub http: Arc<dyn HttpClient>,
    pub domain: String,
    app_id: String,
    app_secret: String,
    /// tenant_access_token 缓存：(token, 过期毫秒)。
    token: std::sync::Mutex<Option<(String, u64)>>,
}

impl Feishu {
    pub fn new(
        http: Arc<dyn HttpClient>,
        domain: String,
        app_id: String,
        app_secret: String,
    ) -> Self {
        Self {
            http,
            domain,
            app_id,
            app_secret,
            token: std::sync::Mutex::new(None),
        }
    }

    /// 取长连接 wss 地址：POST /callback/ws/endpoint。
    fn gen_endpoint(&self) -> Result<String, String> {
        let url = format!("{}/callback/ws/endpoint", self.domain);
        let body = serde_json::json!({ "AppID": self.app_id, "AppSecret": self.app_secret });
        let resp = self
            .http
            .post_json(&url, &body.to_string(), &[("locale", "zh")])?;
        let v: Value =
            serde_json::from_str(&resp).map_err(|e| format!("parse ws/endpoint: {e}"))?;
        let code = v.get("code").and_then(|x| x.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(format!("ws/endpoint 失败：{resp}"));
        }
        v.pointer("/data/URL")
            .and_then(|x| x.as_str())
            .map(String::from)
            .ok_or_else(|| format!("ws/endpoint 无 URL：{resp}"))
    }

    /// 取（缓存 + 到期刷新）tenant_access_token。
    fn tenant_token(&self) -> Result<String, String> {
        {
            let g = self.token.lock().unwrap();
            if let Some((t, exp)) = g.as_ref() {
                if now_ms() + 60_000 < *exp {
                    return Ok(t.clone());
                }
            }
        }
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.domain
        );
        let body = serde_json::json!({ "app_id": self.app_id, "app_secret": self.app_secret });
        let resp = self.http.post_json(&url, &body.to_string(), &[])?;
        let v: Value = serde_json::from_str(&resp).map_err(|e| format!("parse token: {e}"))?;
        let token = v
            .get("tenant_access_token")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("tenant_access_token 响应异常：{resp}"))?
            .to_string();
        let expire = v.get("expire").and_then(|x| x.as_u64()).unwrap_or(7200);
        *self.token.lock().unwrap() = Some((token.clone(), now_ms() + expire * 1000));
        Ok(token)
    }

    fn connect_and_listen(
        &self,
        sink: &dyn Fn(InboundMessage),
        shutdown: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let conn_url = self.gen_endpoint()?;
        let service_id = parse_service_id(&conn_url);
        let (mut socket, _) =
            tungstenite::connect(&conn_url).map_err(|e| format!("ws connect: {e}"))?;
        set_read_timeout(&mut socket, Some(std::time::Duration::from_secs(30)));
        eprintln!("[remote] 飞书 WS 已连接 service_id={service_id}");
        let mut last_ping = now_ms();
        while !shutdown.load(Ordering::Relaxed) {
            match socket.read() {
                Ok(WsMessage::Binary(bytes)) => {
                    if let Ok(frame) = Frame::decode(bytes.as_ref()) {
                        if frame.method == FRAME_DATA {
                            // 任何数据帧都要 ACK（否则服务端重推）。
                            if let Some(inbound) = data_frame_to_inbound(&frame) {
                                sink(inbound);
                            }
                            let ack = build_ack(&frame);
                            let _ = socket.send(WsMessage::Binary(ack.into()));
                        }
                        // 控制帧（ping/pong）：忽略。
                    }
                }
                Ok(WsMessage::Ping(p)) => {
                    let _ = socket.send(WsMessage::Pong(p));
                }
                Ok(WsMessage::Close(_)) => return Ok(()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    // 读超时：到点发应用层 ping 保活。
                    if now_ms().saturating_sub(last_ping) >= 110_000 {
                        let _ = socket.send(WsMessage::Binary(ping_frame(service_id).into()));
                        last_ping = now_ms();
                    }
                }
                Err(e) => return Err(format!("ws read: {e}")),
            }
        }
        let _ = socket.close(None);
        Ok(())
    }
}

impl Connector for Feishu {
    fn channel(&self) -> &str {
        "feishu"
    }

    fn max_len(&self) -> usize {
        4000
    }

    fn run(&self, sink: &dyn Fn(InboundMessage), shutdown: &std::sync::atomic::AtomicBool) {
        use std::sync::atomic::Ordering;
        while !shutdown.load(Ordering::Relaxed) {
            if let Err(e) = self.connect_and_listen(sink, shutdown) {
                eprintln!("[remote] 飞书连接断开，3s 后重连：{e}");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }

    fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String> {
        let token = self.tenant_token()?;
        let url = format!(
            "{}/open-apis/im/v1/messages?receive_id_type=open_id",
            self.domain
        );
        let auth = format!("Bearer {token}");
        let headers = [("Authorization", auth.as_str())];
        for item in items {
            let OutItem::Text(text) = item;
            let content = lark_md_card_content(text);
            let req = serde_json::json!({
                "receive_id": peer.peer_id,
                "msg_type": "interactive",
                "content": content,
            });
            let resp = self.http.post_json(&url, &req.to_string(), &headers)?;
            if let Ok(v) = serde_json::from_str::<Value>(&resp) {
                if v.get("code").and_then(|x| x.as_i64()).unwrap_or(0) != 0 {
                    return Err(format!("飞书 send 失败：{resp}"));
                }
            }
        }
        Ok(())
    }

    fn send_typing(&self, _peer: &PeerRef) -> Result<(), String> {
        Ok(())
    }
}

fn lark_md_card_content(text: &str) -> String {
    serde_json::json!({
        "config": {
            "wide_screen_mode": true
        },
        "elements": [
            {
                "tag": "div",
                "text": {
                    "tag": "lark_md",
                    "content": text
                }
            }
        ]
    })
    .to_string()
}

/// DATA 帧 → 归一化入站（仅 im.message.receive_v1 文本；合包 sum>1 暂跳过）。
fn data_frame_to_inbound(frame: &Frame) -> Option<InboundMessage> {
    if frame.header("type") != Some("event") {
        return None;
    }
    let sum: i32 = frame
        .header("sum")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    if sum > 1 {
        eprintln!("[remote] 飞书合包消息(sum>1)暂不支持，跳过");
        return None;
    }
    let payload = frame.payload.as_ref()?;
    event_to_inbound(payload)
}

/// 飞书事件 JSON（payload）→ 入站消息。仅处理 im.message.receive_v1 的文本。
fn event_to_inbound(payload: &[u8]) -> Option<InboundMessage> {
    let v: Value = serde_json::from_slice(payload).ok()?;
    let event_type = v.pointer("/header/event_type").and_then(|x| x.as_str())?;
    if event_type != "im.message.receive_v1" {
        return None;
    }
    let msg = v.pointer("/event/message")?;
    if msg.get("message_type").and_then(|x| x.as_str()) != Some("text") {
        return None;
    }
    let content = msg.get("content").and_then(|x| x.as_str())?;
    let text = serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|c| c.get("text").and_then(|x| x.as_str()).map(String::from))?;
    if text.trim().is_empty() {
        return None;
    }
    let open_id = v
        .pointer("/event/sender/sender_id/open_id")
        .and_then(|x| x.as_str())?;
    Some(InboundMessage {
        channel: "feishu".into(),
        account: msg
            .get("chat_id")
            .and_then(|x| x.as_str())
            .map(String::from),
        peer_id: open_id.into(),
        peer_name: None,
        text: text.trim().into(),
        kind: "text".into(),
        context_token: None,
        received_at: crate::engine::now_string(),
    })
}

/// 构造 ACK：复用收到的 Frame，payload 换成 `{"code":200}`。
fn build_ack(frame: &Frame) -> Vec<u8> {
    let mut ack = frame.clone();
    ack.payload = Some(b"{\"code\":200}".to_vec());
    ack.encode_to_vec()
}

/// 构造 ping 控制帧（header type=ping，method=CONTROL）。
fn ping_frame(service_id: i32) -> Vec<u8> {
    let frame = Frame {
        seq_id: 0,
        log_id: 0,
        service: service_id,
        method: FRAME_CONTROL,
        headers: vec![Header {
            key: "type".into(),
            value: "ping".into(),
        }],
        payload_encoding: None,
        payload_type: None,
        payload: None,
        log_id_new: None,
    };
    frame.encode_to_vec()
}

/// 从 wss URL 的 query 解析 service_id（失败回 1）。
fn parse_service_id(url: &str) -> i32 {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("service_id="))
        .and_then(|v| v.split('&').next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn set_read_timeout(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    dur: Option<std::time::Duration>,
) {
    match socket.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(dur);
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.get_ref().set_read_timeout(dur);
        }
        _ => {}
    }
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
    use crate::remote::connector::{OutItem, PeerRef};
    use crate::remote::http::HttpClient;
    use std::sync::Mutex;

    struct CaptureHttp {
        bodies: Mutex<Vec<String>>,
    }

    impl CaptureHttp {
        fn new() -> Self {
            Self {
                bodies: Mutex::new(Vec::new()),
            }
        }

        fn bodies(&self) -> Vec<String> {
            self.bodies.lock().unwrap().clone()
        }
    }

    impl HttpClient for CaptureHttp {
        fn post_json(
            &self,
            url: &str,
            body: &str,
            _headers: &[(&str, &str)],
        ) -> Result<String, String> {
            self.bodies.lock().unwrap().push(body.to_string());
            if url.contains("/tenant_access_token/internal") {
                Ok(r#"{"tenant_access_token":"tenant-token","expire":7200}"#.into())
            } else {
                Ok(r#"{"code":0}"#.into())
            }
        }

        fn get_json(&self, _url: &str, _headers: &[(&str, &str)]) -> Result<String, String> {
            Err("unexpected get".into())
        }
    }

    fn data_event_frame(event_json: &str) -> Frame {
        Frame {
            seq_id: 7,
            log_id: 9,
            service: 100,
            method: FRAME_DATA,
            headers: vec![
                Header {
                    key: "type".into(),
                    value: "event".into(),
                },
                Header {
                    key: "message_id".into(),
                    value: "m1".into(),
                },
                Header {
                    key: "sum".into(),
                    value: "1".into(),
                },
                Header {
                    key: "seq".into(),
                    value: "0".into(),
                },
            ],
            payload_encoding: None,
            payload_type: None,
            payload: Some(event_json.as_bytes().to_vec()),
            log_id_new: None,
        }
    }

    #[test]
    fn frame_roundtrip_and_ack() {
        let f = data_event_frame("{}");
        let bytes = f.encode_to_vec();
        let decoded = Frame::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.method, FRAME_DATA);
        assert_eq!(decoded.header("type"), Some("event"));
        assert_eq!(decoded.header("message_id"), Some("m1"));
        // ack 复用 headers，payload 换成 code:200。
        let ack = Frame::decode(build_ack(&decoded).as_slice()).unwrap();
        assert_eq!(ack.header("message_id"), Some("m1"));
        assert_eq!(ack.payload.as_deref(), Some(&b"{\"code\":200}"[..]));
    }

    #[test]
    fn event_to_inbound_extracts_text() {
        let event = r#"{
            "schema":"2.0",
            "header":{"event_type":"im.message.receive_v1"},
            "event":{
                "sender":{"sender_id":{"open_id":"ou_abc"}},
                "message":{"chat_id":"oc_1","message_type":"text","content":"{\"text\":\"你好\"}"}
            }
        }"#;
        let m = event_to_inbound(event.as_bytes()).unwrap();
        assert_eq!(m.peer_id, "ou_abc");
        assert_eq!(m.text, "你好");
        assert_eq!(m.account.as_deref(), Some("oc_1"));
        assert_eq!(m.channel, "feishu");
    }

    #[test]
    fn event_to_inbound_skips_non_text_and_other_events() {
        let img = r#"{"header":{"event_type":"im.message.receive_v1"},"event":{"sender":{"sender_id":{"open_id":"o"}},"message":{"message_type":"image","content":"{}"}}}"#;
        assert!(event_to_inbound(img.as_bytes()).is_none());
        let other = r#"{"header":{"event_type":"im.chat.updated_v1"},"event":{}}"#;
        assert!(event_to_inbound(other.as_bytes()).is_none());
    }

    #[test]
    fn ping_frame_is_control_ping() {
        let f = Frame::decode(ping_frame(42).as_slice()).unwrap();
        assert_eq!(f.method, FRAME_CONTROL);
        assert_eq!(f.service, 42);
        assert_eq!(f.header("type"), Some("ping"));
    }

    #[test]
    fn data_frame_acks_only_text_event() {
        let f = data_event_frame(
            r#"{"header":{"event_type":"im.message.receive_v1"},"event":{"sender":{"sender_id":{"open_id":"o"}},"message":{"chat_id":"c","message_type":"text","content":"{\"text\":\"hi\"}"}}}"#,
        );
        assert!(data_frame_to_inbound(&f).is_some());
    }

    #[test]
    fn parse_service_id_from_url() {
        assert_eq!(
            parse_service_id("wss://h/ws?device_id=d&service_id=88&x=1"),
            88
        );
        assert_eq!(parse_service_id("wss://h/ws?device_id=d"), 1);
    }

    #[test]
    fn send_uses_interactive_lark_md_for_markdown_rendering() {
        let http = Arc::new(CaptureHttp::new());
        let connector = Feishu::new(
            http.clone(),
            DOMAIN.into(),
            "app_id".into(),
            "app_secret".into(),
        );
        let peer = PeerRef {
            channel: "feishu".into(),
            account: None,
            peer_id: "ou_abc".into(),
            context_token: None,
        };

        connector
            .send(&peer, &[OutItem::Text("**加粗**\n- 列表".into())])
            .unwrap();

        let bodies = http.bodies();
        let message_body: Value = serde_json::from_str(&bodies[1]).unwrap();
        assert_eq!(message_body["receive_id"], "ou_abc");
        assert_eq!(message_body["msg_type"], "interactive");
        let content: Value =
            serde_json::from_str(message_body["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["elements"][0]["tag"], "div");
        assert_eq!(content["elements"][0]["text"]["tag"], "lark_md");
        assert_eq!(
            content["elements"][0]["text"]["content"],
            "**加粗**\n- 列表"
        );
    }
}
