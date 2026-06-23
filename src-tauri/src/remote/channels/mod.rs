//! 各 IM 平台的 Connector 实现。平台细节封死在本子模块内。

pub mod dingtalk;
pub mod feishu;
pub mod telegram;
pub mod wechat_clawbot;

pub use dingtalk::DingTalk;
pub use feishu::Feishu;
pub use telegram::Telegram;
pub use wechat_clawbot::WechatClawbot;
