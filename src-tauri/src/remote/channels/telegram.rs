//! Telegram Bot connector。getUpdates 长轮询纯出站（与微信同构，复用 `run` = poll_once 循环）；
//! sendMessage 回发。Bot API 契约稳定、公开，无需联调校正。
//!
//! API：`https://api.telegram.org/bot<token>/<method>`；
//! - getUpdates：body `{offset, timeout}` → `{ok, result:[{update_id, message:{chat:{id}, from:{username}, text}}]}`；
//!   offset = 上次最大 update_id + 1（推进游标，已确认的更新不再返回）。
//! - sendMessage：`{chat_id, text}`；sendChatAction：`{chat_id, action:"typing"}`。

use std::sync::Arc;

use serde_json::Value;

use crate::remote::connector::{Connector, InboundMessage, OutItem, PeerRef};
use crate::remote::http::HttpClient;

/// 默认 Bot API base。
pub const DEFAULT_BASE_URL: &str = "https://api.telegram.org";

pub struct Telegram {
    pub http: Arc<dyn HttpClient>,
    pub base_url: String,
    token: String,
    /// getUpdates 游标（下一次请求的 offset = 已见最大 update_id + 1）。内存态，重启从 0 开始
    /// （0 = 取未确认的更新；已被本 bot 处理并推进过的不再返回）。
    offset: std::sync::Mutex<i64>,
}

impl Telegram {
    pub fn new(http: Arc<dyn HttpClient>, base_url: String, token: String) -> Self {
        Self {
            http,
            base_url,
            token,
            offset: std::sync::Mutex::new(0),
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.token, method)
    }

    /// 单次长轮询：getUpdates 取一批更新 + 推进 offset 游标。`run` 循环调用它。
    pub(crate) fn poll_once(&self) -> Result<Vec<InboundMessage>, String> {
        let offset = *self.offset.lock().unwrap();
        let req = serde_json::json!({ "offset": offset, "timeout": 25 });
        let body = self
            .http
            .post_json(&self.api_url("getUpdates"), &req.to_string(), &[])?;
        let v: Value = serde_json::from_str(&body).map_err(|e| format!("parse getUpdates: {e}"))?;
        if !v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) {
            return Err(format!("getUpdates 失败：{body}"));
        }
        let mut out = Vec::new();
        let mut max_update = offset - 1;
        if let Some(arr) = v.get("result").and_then(|x| x.as_array()) {
            for u in arr {
                if let Some(id) = u.get("update_id").and_then(|x| x.as_i64()) {
                    if id > max_update {
                        max_update = id;
                    }
                }
                // 只处理含文本的 message 更新（忽略 edited_message/callback 等）。
                let Some(m) = u.get("message") else {
                    continue;
                };
                let Some(chat_id) = m.pointer("/chat/id").and_then(|x| x.as_i64()) else {
                    continue;
                };
                let Some(text) = m.get("text").and_then(|x| x.as_str()) else {
                    continue;
                };
                out.push(InboundMessage {
                    channel: "telegram".into(),
                    account: None,
                    peer_id: chat_id.to_string(),
                    peer_name: m
                        .pointer("/from/username")
                        .and_then(|x| x.as_str())
                        .map(String::from),
                    text: text.into(),
                    kind: "text".into(),
                    context_token: None,
                    received_at: crate::engine::now_string(),
                });
            }
        }
        // 推进游标：下次从最大 update_id + 1 开始，确认已处理的更新。
        *self.offset.lock().unwrap() = max_update + 1;
        Ok(out)
    }
}

impl Connector for Telegram {
    fn channel(&self) -> &str {
        "telegram"
    }

    fn max_len(&self) -> usize {
        4096 // Telegram 单条文本上限
    }

    fn run(&self, sink: &dyn Fn(InboundMessage), shutdown: &std::sync::atomic::AtomicBool) {
        use std::sync::atomic::Ordering;
        while !shutdown.load(Ordering::Relaxed) {
            match self.poll_once() {
                Ok(msgs) => {
                    let empty = msgs.is_empty();
                    for m in msgs {
                        sink(m);
                    }
                    if empty {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
                Err(e) => {
                    eprintln!("[remote] telegram poll 失败，退避重试：{e}");
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            }
        }
    }

    fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String> {
        let url = self.api_url("sendMessage");
        for item in items {
            let OutItem::Text(text) = item;
            // chat_id 优先按整数发（数字 id）；非数字（如 @username）则按字符串。
            let chat_id = peer
                .peer_id
                .parse::<i64>()
                .map(|n| serde_json::json!(n))
                .unwrap_or_else(|_| serde_json::json!(peer.peer_id));
            let req = serde_json::json!({ "chat_id": chat_id, "text": text });
            let resp = self.http.post_json(&url, &req.to_string(), &[])?;
            if let Ok(v) = serde_json::from_str::<Value>(&resp) {
                if !v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) {
                    return Err(format!("sendMessage 失败：{resp}"));
                }
            }
        }
        Ok(())
    }

    fn send_typing(&self, peer: &PeerRef) -> Result<(), String> {
        let chat_id = peer
            .peer_id
            .parse::<i64>()
            .map(|n| serde_json::json!(n))
            .unwrap_or_else(|_| serde_json::json!(peer.peer_id));
        let req = serde_json::json!({ "chat_id": chat_id, "action": "typing" });
        self.http
            .post_json(&self.api_url("sendChatAction"), &req.to_string(), &[])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::http::{HttpClient, MockHttp};
    use std::sync::Arc;

    fn conn(http: Arc<dyn HttpClient>) -> Telegram {
        Telegram::new(http, DEFAULT_BASE_URL.into(), "TOK".into())
    }

    #[test]
    fn poll_parses_message_and_advances_offset() {
        let body = r#"{"ok":true,"result":[
            {"update_id":100,"message":{"chat":{"id":42},"from":{"username":"alice"},"text":"你好"}}
        ]}"#;
        let http = Arc::new(MockHttp::new(vec![("getUpdates".into(), body.into())]));
        let c = conn(http);
        let msgs = c.poll_once().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].peer_id, "42");
        assert_eq!(msgs[0].text, "你好");
        assert_eq!(msgs[0].peer_name.as_deref(), Some("alice"));
        assert_eq!(msgs[0].channel, "telegram");
        // 游标推进到 update_id + 1。
        assert_eq!(*c.offset.lock().unwrap(), 101);
    }

    #[test]
    fn poll_skips_non_text_updates() {
        let body = r#"{"ok":true,"result":[
            {"update_id":7,"edited_message":{"chat":{"id":1},"text":"x"}},
            {"update_id":8,"message":{"chat":{"id":1}}}
        ]}"#;
        let http = Arc::new(MockHttp::new(vec![("getUpdates".into(), body.into())]));
        let c = conn(http);
        assert!(c.poll_once().unwrap().is_empty());
        // 即便无可用消息，游标也推进，避免重复拉取。
        assert_eq!(*c.offset.lock().unwrap(), 9);
    }

    #[test]
    fn send_posts_chat_id_and_text() {
        let http = Arc::new(MockHttp::new(vec![(
            "sendMessage".into(),
            "{\"ok\":true}".into(),
        )]));
        let c = conn(http);
        let peer = PeerRef {
            channel: "telegram".into(),
            account: None,
            peer_id: "42".into(),
            context_token: None,
        };
        c.send(&peer, &[OutItem::Text("回复".into())]).unwrap();
    }

    #[test]
    fn send_errors_when_not_ok() {
        let http = Arc::new(MockHttp::new(vec![(
            "sendMessage".into(),
            "{\"ok\":false,\"description\":\"bad\"}".into(),
        )]));
        let c = conn(http);
        let peer = PeerRef {
            channel: "telegram".into(),
            account: None,
            peer_id: "42".into(),
            context_token: None,
        };
        assert!(c.send(&peer, &[OutItem::Text("x".into())]).is_err());
    }
}
