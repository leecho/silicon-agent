//! 传输抽象：一条 JSON-RPC 出、对应响应回。
//! 请求/响应配对由各实现负责（stdio 按 id 匹配流内消息；http 一问一答）；client 层另有 id 一致性兜底校验。

use std::time::Duration;

pub trait McpTransport: Send {
    /// 发送一条消息并等待其响应。通知（无 id）只发不等，返回 Ok(None)。
    /// 一期不支持 server 主动发起的请求（sampling 等），收到即忽略。
    fn request(
        &mut self,
        msg: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Option<serde_json::Value>, String>;
}

/// 测试用内存传输：按脚本回放响应。`sent` 记录全部出站消息供断言。
#[cfg(test)]
pub struct MockTransport {
    pub sent: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    /// 每次带 id 的请求弹出一条作为响应（按队列顺序）。
    pub replies: std::collections::VecDeque<serde_json::Value>,
    /// true：自动对齐响应 id（存量行为）；false：原样回放（用于测试 id 不匹配）。
    align_ids: bool,
}

#[cfg(test)]
impl MockTransport {
    pub fn new(replies: Vec<serde_json::Value>) -> Self {
        Self {
            sent: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            replies: replies.into(),
            align_ids: true,
        }
    }

    /// 原样回放模式：响应 id 不做任何改写，用于测试 id 不匹配场景。
    pub fn new_raw(replies: Vec<serde_json::Value>) -> Self {
        Self {
            sent: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            replies: replies.into(),
            align_ids: false,
        }
    }
}

#[cfg(test)]
impl McpTransport for MockTransport {
    fn request(
        &mut self,
        msg: &serde_json::Value,
        _timeout: Duration,
    ) -> Result<Option<serde_json::Value>, String> {
        let has_id = msg.get("id").is_some();
        self.sent.lock().unwrap().push(msg.clone());
        if !has_id {
            return Ok(None);
        }
        let mut reply = self
            .replies
            .pop_front()
            .ok_or_else(|| "mock: 无预置响应".to_string())?;
        // 自动对齐 id，脚本里不必手写。
        if self.align_ids {
            if let Some(id) = msg.get("id") {
                reply["id"] = id.clone();
            }
        }
        Ok(Some(reply))
    }
}
