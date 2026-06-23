//! 微信 ClawBot / iLink connector + 扫码配对（login）。
//!
//! 协议（base https://ilinkai.weixin.qq.com）：
//! - login：GET ilink/bot/get_bot_qrcode?bot_type=3 取码 → GET ilink/bot/get_qrcode_status?qrcode=…
//!   轮询 wait→scaned→confirmed，confirmed 返回 bot_token / ilink_bot_id / baseurl。
//! - 业务：POST ilink/bot/getupdates（长轮询）/ ilink/bot/sendmessage / sendtyping，
//!   header 带 AuthorizationType: ilink_bot_token + Authorization: Bearer <token> + X-WECHAT-UIN，
//!   body 带 base_info.channel_version。context_token 每条回复必带。
//!
//! 注：exact JSON 字段/状态串以真实服务为准（参考 github.com/photon-hq/wechat-ilink-client、
//! nightsailer/wechat-clawbot）。本实现解析做容错（按已知键/子串匹配），便于联调校正。

use std::sync::Arc;

use serde_json::Value;

use crate::remote::connector::{Connector, InboundMessage, OutItem, PeerRef};
use crate::remote::http::HttpClient;

/// 默认 iLink base url。
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// 取码结果：qrcode 标识（轮询用）+ **二维码内容**（需前端编码成二维码图片让用户扫）。
/// 注：iLink 返回的是要被微信扫码的深链（liteapp.weixin.qq.com/...），不是图片 URL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCode {
    pub qrcode: String,
    pub qr_content: String,
}

/// 扫码配对状态机。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairStatus {
    Wait,
    Scanned,
    Expired,
    Confirmed {
        bot_token: String,
        ilink_bot_id: Option<String>,
        base_url: Option<String>,
    },
}

fn str_at<'a>(v: &'a Value, key: &str, ptr: &str) -> Option<&'a str> {
    v.get(key)
        .and_then(|x| x.as_str())
        .or_else(|| v.pointer(ptr).and_then(|x| x.as_str()))
}

/// 从一条 msg 的 `item_list` 取首个非空文本（`text_item.text`）。无文本返回 None。
fn extract_text(msg: &Value) -> Option<String> {
    let items = msg.get("item_list")?.as_array()?;
    for it in items {
        if let Some(t) = it.pointer("/text_item/text").and_then(|x| x.as_str()) {
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// 取二维码：GET ilink/bot/get_bot_qrcode?bot_type=3。
/// 真实响应：`{"qrcode":"<id>","qrcode_img_content":"https://liteapp.weixin.qq.com/q/...","ret":0}`。
/// `qrcode` 是轮询标识；`qrcode_img_content` 是要被微信扫码的深链内容（前端编码成二维码图片）。
pub fn request_qrcode(http: &dyn HttpClient, base_url: &str) -> Result<QrCode, String> {
    let url = format!("{base_url}/ilink/bot/get_bot_qrcode?bot_type=3");
    let body = http.get_json(&url, &[])?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("parse get_bot_qrcode: {e}"))?;
    let ret = v.get("ret").and_then(|x| x.as_i64()).unwrap_or(0);
    if ret != 0 {
        return Err(format!("get_bot_qrcode 失败 ret={ret}"));
    }
    let qrcode = str_at(&v, "qrcode", "/data/qrcode")
        .ok_or_else(|| "get_bot_qrcode：缺少 qrcode 字段".to_string())?
        .to_string();
    let qr_content = str_at(&v, "qrcode_img_content", "/data/qrcode_img_content")
        .ok_or_else(|| "get_bot_qrcode：缺少 qrcode_img_content 字段".to_string())?
        .to_string();
    Ok(QrCode { qrcode, qr_content })
}

/// 轮询配对状态：GET ilink/bot/get_qrcode_status?qrcode=…。
pub fn poll_qrcode_status(
    http: &dyn HttpClient,
    base_url: &str,
    qrcode: &str,
) -> Result<PairStatus, String> {
    let url = format!("{base_url}/ilink/bot/get_qrcode_status?qrcode={qrcode}");
    let body = http.get_json(&url, &[])?;
    let v: Value =
        serde_json::from_str(&body).map_err(|e| format!("parse get_qrcode_status: {e}"))?;
    let status = str_at(&v, "status", "/data/status")
        .unwrap_or("")
        .to_lowercase();
    if status.contains("confirm") {
        let bot_token = str_at(&v, "bot_token", "/data/bot_token")
            .ok_or_else(|| "confirmed：缺少 bot_token".to_string())?
            .to_string();
        Ok(PairStatus::Confirmed {
            bot_token,
            ilink_bot_id: str_at(&v, "ilink_bot_id", "/data/ilink_bot_id").map(String::from),
            base_url: str_at(&v, "baseurl", "/data/baseurl").map(String::from),
        })
    } else if status.contains("scan") {
        Ok(PairStatus::Scanned)
    } else if status.contains("expire") {
        Ok(PairStatus::Expired)
    } else {
        Ok(PairStatus::Wait)
    }
}

pub struct WechatClawbot {
    pub http: Arc<dyn HttpClient>,
    pub base_url: String,
    pub account: String,
    /// "Bearer <bot_token>"，业务请求 Authorization 头。
    bearer: String,
    /// X-WECHAT-UIN：base64(随机 uint32 的十进制串)，每连接器实例固定。
    uin: String,
    /// getupdates 长轮询游标（`get_updates_buf`）。每次 poll 用上一次响应的游标推进，
    /// 否则收不到新消息。内存态，重启从空开始（服务端按当前位置续发，不会重放旧消息）。
    cursor: std::sync::Mutex<String>,
}

impl WechatClawbot {
    /// 用配对得到的 bot_token 构造连接器。
    pub fn with_token(
        http: Arc<dyn HttpClient>,
        base_url: String,
        bot_token: String,
        account: String,
    ) -> Self {
        Self {
            http,
            base_url,
            account,
            bearer: format!("Bearer {bot_token}"),
            uin: gen_uin(),
            cursor: std::sync::Mutex::new(String::new()),
        }
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        vec![
            ("AuthorizationType", "ilink_bot_token"),
            ("Authorization", self.bearer.as_str()),
            ("X-WECHAT-UIN", self.uin.as_str()),
        ]
    }

    /// 单次长轮询：getupdates 取一批消息 + 推进游标。`run` 循环调用它。
    ///
    /// 真实契约（联调验证）：响应 `{ "msgs":[…], "get_updates_buf":"…" }`；每条 msg 含
    /// from_user_id / context_token / item_list[].text_item.text；必须把上次的 get_updates_buf
    /// 回传以推进游标，否则收不到新消息。
    pub(crate) fn poll_once(&self) -> Result<Vec<InboundMessage>, String> {
        let url = format!("{}/ilink/bot/getupdates", self.base_url);
        let buf = self.cursor.lock().unwrap().clone();
        let req = if buf.is_empty() {
            serde_json::json!({ "base_info": { "channel_version": "2.0.0" } })
        } else {
            serde_json::json!({ "base_info": { "channel_version": "2.0.0" }, "get_updates_buf": buf })
        };
        let body = self
            .http
            .post_json(&url, &req.to_string(), &self.headers())?;
        let v: Value = serde_json::from_str(&body).map_err(|e| format!("parse getupdates: {e}"))?;
        // 推进游标。
        if let Some(nb) = v.get("get_updates_buf").and_then(|x| x.as_str()) {
            *self.cursor.lock().unwrap() = nb.to_string();
        }
        let mut out = Vec::new();
        if let Some(msgs) = v.get("msgs").and_then(|x| x.as_array()) {
            for m in msgs {
                let peer_id = m.get("from_user_id").and_then(|x| x.as_str()).unwrap_or("");
                if peer_id.is_empty() {
                    continue;
                }
                // 一期只处理文本：取 item_list 里首个 text_item.text；无文本则跳过（图/语音等）。
                let Some(text) = extract_text(m) else {
                    continue;
                };
                out.push(InboundMessage {
                    channel: "wechat".into(),
                    account: Some(self.account.clone()),
                    peer_id: peer_id.into(),
                    peer_name: None,
                    text,
                    kind: "text".into(),
                    context_token: m
                        .get("context_token")
                        .and_then(|x| x.as_str())
                        .map(String::from),
                    received_at: crate::engine::now_string(),
                });
            }
        }
        Ok(out)
    }
}

impl Connector for WechatClawbot {
    fn channel(&self) -> &str {
        "wechat"
    }

    fn max_len(&self) -> usize {
        2000
    }

    fn run(&self, sink: &dyn Fn(InboundMessage), shutdown: &std::sync::atomic::AtomicBool) {
        use std::sync::atomic::Ordering;
        // 微信接收循环：长轮询 poll_once → 逐条推 sink，直到 shutdown。失败退避重试。
        while !shutdown.load(Ordering::Relaxed) {
            match self.poll_once() {
                Ok(msgs) => {
                    let empty = msgs.is_empty();
                    for m in msgs {
                        sink(m);
                    }
                    // getupdates 是 ~30s 长轮询；空批次小睡避免快速空转。
                    if empty {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
                Err(e) => {
                    eprintln!("[remote] 微信 poll 失败，退避重试：{e}");
                    std::thread::sleep(std::time::Duration::from_secs(3));
                }
            }
        }
    }

    fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String> {
        // 真实契约（联调验证）：所有字段在 `msg` 包裹内——to_user_id / client_id(每条唯一) /
        // message_type=2(BOT) / message_state=2(FINISH) / context_token / item_list；ret==0 成功。
        // 顶层缺 msg 包裹或缺 client_id/message_type/message_state 会返回 ret=-2。
        let url = format!("{}/ilink/bot/sendmessage", self.base_url);
        for item in items {
            let OutItem::Text(text) = item;
            let client_id = format!("bot-{}", now_nanos());
            let req = serde_json::json!({
                "base_info": { "channel_version": "2.0.0" },
                "msg": {
                    "from_user_id": "",
                    "to_user_id": peer.peer_id,
                    "client_id": client_id,
                    "message_type": 2,
                    "message_state": 2,
                    "context_token": peer.context_token,
                    "item_list": [ { "type": 1, "text_item": { "text": text } } ],
                }
            });
            let resp = self
                .http
                .post_json(&url, &req.to_string(), &self.headers())?;
            if let Ok(v) = serde_json::from_str::<Value>(&resp) {
                let ret = v.get("ret").and_then(|x| x.as_i64()).unwrap_or(0);
                if ret != 0 {
                    return Err(format!("sendmessage 失败 ret={ret}"));
                }
            }
        }
        Ok(())
    }

    fn send_typing(&self, peer: &PeerRef) -> Result<(), String> {
        let url = format!("{}/ilink/bot/sendtyping", self.base_url);
        let req = serde_json::json!({
            "base_info": { "channel_version": "2.0.0" },
            "to_user_id": peer.peer_id,
            "context_token": peer.context_token,
        });
        self.http
            .post_json(&url, &req.to_string(), &self.headers())?;
        Ok(())
    }
}

/// 当前时间纳秒（用于 sendmessage 的每条唯一 client_id）。
fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// X-WECHAT-UIN = base64(随机 uint32 的十进制字符串)。随机性由时间纳秒派生（无 rand 依赖）。
fn gen_uin() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    b64(nanos.to_string().as_bytes())
}

/// 极小标准 base64 编码（仅供 UIN 短串用，避免引入 base64 依赖）。
fn b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(T[(b0 >> 2) as usize] as char);
        out.push(T[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::connector::{Connector, OutItem, PeerRef};
    use crate::remote::http::{HttpClient, MockHttp};
    use std::sync::Arc;

    fn conn(http: Arc<dyn HttpClient>) -> WechatClawbot {
        WechatClawbot::with_token(
            http,
            DEFAULT_BASE_URL.into(),
            "tok123".into(),
            "acct".into(),
        )
    }

    #[test]
    fn b64_known_vectors() {
        assert_eq!(b64(b"M"), "TQ==");
        assert_eq!(b64(b"Ma"), "TWE=");
        assert_eq!(b64(b"Man"), "TWFu");
    }

    #[test]
    fn request_qrcode_parses_real_response_shape() {
        // 真实 iLink 响应：qrcode（轮询 id）+ qrcode_img_content（待编码深链）+ ret。
        let body = r#"{"qrcode":"QR1","qrcode_img_content":"https://liteapp.weixin.qq.com/q/7GiQu1?qrcode=QR1&bot_type=3","ret":0}"#;
        let http = Arc::new(MockHttp::new(vec![("get_bot_qrcode".into(), body.into())]));
        let q = request_qrcode(http.as_ref(), DEFAULT_BASE_URL).unwrap();
        assert_eq!(q.qrcode, "QR1");
        assert_eq!(
            q.qr_content,
            "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode=QR1&bot_type=3"
        );
    }

    #[test]
    fn request_qrcode_errors_on_nonzero_ret() {
        let body = r#"{"ret":1,"msg":"bad"}"#;
        let http = Arc::new(MockHttp::new(vec![("get_bot_qrcode".into(), body.into())]));
        assert!(request_qrcode(http.as_ref(), DEFAULT_BASE_URL).is_err());
    }

    #[test]
    fn poll_status_maps_states() {
        let confirmed = r#"{"status":"confirmed","bot_token":"BT","ilink_bot_id":"id1"}"#;
        let http = Arc::new(MockHttp::new(vec![(
            "get_qrcode_status".into(),
            confirmed.into(),
        )]));
        match poll_qrcode_status(http.as_ref(), DEFAULT_BASE_URL, "QR1").unwrap() {
            PairStatus::Confirmed {
                bot_token,
                ilink_bot_id,
                ..
            } => {
                assert_eq!(bot_token, "BT");
                assert_eq!(ilink_bot_id.as_deref(), Some("id1"));
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
        let waiting = Arc::new(MockHttp::new(vec![(
            "get_qrcode_status".into(),
            r#"{"status":"wait"}"#.into(),
        )]));
        assert_eq!(
            poll_qrcode_status(waiting.as_ref(), DEFAULT_BASE_URL, "QR1").unwrap(),
            PairStatus::Wait
        );
        let scaned = Arc::new(MockHttp::new(vec![(
            "get_qrcode_status".into(),
            r#"{"status":"scaned"}"#.into(),
        )]));
        assert_eq!(
            poll_qrcode_status(scaned.as_ref(), DEFAULT_BASE_URL, "QR1").unwrap(),
            PairStatus::Scanned
        );
    }

    #[test]
    fn poll_parses_real_msgs_shape() {
        // 真实契约：msgs[].from_user_id / context_token / item_list[].text_item.text + get_updates_buf。
        let body = r#"{
            "msgs":[{
                "from_user_id":"u1@im.wechat",
                "context_token":"c1",
                "message_type":1,
                "item_list":[{"type":1,"text_item":{"text":"你好"}}]
            }],
            "get_updates_buf":"BUF1"
        }"#;
        let http = Arc::new(MockHttp::new(vec![("getupdates".into(), body.into())]));
        let c = conn(http);
        let msgs = c.poll_once().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].peer_id, "u1@im.wechat");
        assert_eq!(msgs[0].text, "你好");
        assert_eq!(msgs[0].context_token.as_deref(), Some("c1"));
        assert_eq!(msgs[0].channel, "wechat");
        // 游标已推进。
        assert_eq!(*c.cursor.lock().unwrap(), "BUF1");
    }

    #[test]
    fn poll_skips_non_text_msgs() {
        // 无 item_list / 非文本 → 跳过，不产生入站。
        let body =
            r#"{"msgs":[{"from_user_id":"u1","item_list":[{"type":2}]}],"get_updates_buf":"B"}"#;
        let http = Arc::new(MockHttp::new(vec![("getupdates".into(), body.into())]));
        let c = conn(http);
        assert!(c.poll_once().unwrap().is_empty());
    }

    #[test]
    fn send_builds_request_and_succeeds() {
        let http = Arc::new(MockHttp::new(vec![(
            "sendmessage".into(),
            "{\"ok\":true}".into(),
        )]));
        let c = conn(http);
        let peer = PeerRef {
            channel: "wechat".into(),
            account: Some("acct".into()),
            peer_id: "u1".into(),
            context_token: Some("c1".into()),
        };
        c.send(&peer, &[OutItem::Text("回复内容".into())]).unwrap();
    }
}
