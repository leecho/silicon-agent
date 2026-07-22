//! 平台无关的连接器边界。RemoteHub/Router 只依赖本 trait 与归一化类型，
//! 平台细节封死在 channels/**。trait 同步（HttpClient 同步门面），跑专用线程。

/// 入站消息（已归一化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    pub channel: String,
    pub account: Option<String>,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub text: String,
    pub kind: String, // 一期固定 "text"
    pub context_token: Option<String>,
    pub received_at: String,
}

/// 出站内容项。一期仅文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutItem {
    Text(String),
}

/// 发送目标。微信发送必须带 context_token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRef {
    pub channel: String,
    pub account: Option<String>,
    pub peer_id: String,
    pub context_token: Option<String>,
}

/// 同步连接器。**自管接收循环**：`run` 跑在专用线程里，按平台方式（微信 HTTP 长轮询、
/// 钉钉/飞书 WS 收帧）持续接收，把归一化 `InboundMessage` 推进 `sink`，直到 `shutdown` 置位；
/// 内部自行退避/重连。`send` 由出站发送线程串行调用。
///
/// 这样把「拉 vs 推」的差异封死在 connector 内部，Hub 只负责 spawn `run` + 出站 mpsc，
/// 三平台对上层同构。
pub trait Connector: Send + Sync {
    fn channel(&self) -> &str;
    fn max_len(&self) -> usize;
    /// 自管接收循环：持续接收并对每条消息调用 `sink`，直到 `shutdown` 为 true。
    fn run(&self, sink: &dyn Fn(InboundMessage), shutdown: &std::sync::atomic::AtomicBool);
    fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String>;
    fn send_typing(&self, peer: &PeerRef) -> Result<(), String>;
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    /// 测试用 connector：记录发出的内容，poll 返回空批次。
    pub struct FakeConnector {
        pub channel: String,
        pub sent: Mutex<Vec<(PeerRef, Vec<OutItem>)>>,
        pub typing: Mutex<Vec<PeerRef>>,
    }

    impl FakeConnector {
        pub fn new(channel: &str) -> Self {
            Self {
                channel: channel.to_string(),
                sent: Mutex::new(Vec::new()),
                typing: Mutex::new(Vec::new()),
            }
        }
        pub fn sent_texts(&self) -> Vec<String> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .flat_map(|(_, items)| {
                    items.iter().map(|i| match i {
                        OutItem::Text(t) => t.clone(),
                    })
                })
                .collect()
        }
    }

    impl Connector for FakeConnector {
        fn channel(&self) -> &str {
            &self.channel
        }
        fn max_len(&self) -> usize {
            2000
        }
        fn run(&self, _sink: &dyn Fn(InboundMessage), _shutdown: &std::sync::atomic::AtomicBool) {
            // 测试不经 run 接收：router 测试直接调 router.handle。
        }
        fn send(&self, peer: &PeerRef, items: &[OutItem]) -> Result<(), String> {
            self.sent
                .lock()
                .unwrap()
                .push((peer.clone(), items.to_vec()));
            Ok(())
        }
        fn send_typing(&self, peer: &PeerRef) -> Result<(), String> {
            self.typing.lock().unwrap().push(peer.clone());
            Ok(())
        }
    }
}
