//! 远程接入管理命令：channel 启停/配置/密钥、白名单增删查、绑定查看。
//! 一期无专用 UI，供后续设置页与调试调用。

use tauri::{Manager, State};

use crate::app_state::AppState;
use crate::remote::store::{AllowedPeer, RemoteBinding, RemoteChannelConfig};

#[tauri::command]
pub fn list_remote_channels(
    services: State<'_, AppState>,
) -> Result<Vec<RemoteChannelConfig>, String> {
    services.remote_store.list_channels()
}

/// 设 channel 启停 + 非密配置；`secret=Some` 时写入该 channel 的 token 密钥文件。
#[tauri::command]
pub fn set_remote_channel(
    services: State<'_, AppState>,
    channel: String,
    enabled: bool,
    config_json: Option<String>,
    secret: Option<String>,
) -> Result<(), String> {
    if let Some(token) = secret {
        let store = crate::remote::open_secret_store(&services.app)?;
        if token.trim().is_empty() {
            store.clear(&channel)?;
        } else {
            store.set(&channel, &token)?;
        }
    }
    if !enabled {
        services.remote_hub.stop_channel(&channel);
    }
    let now = crate::app_state::now_string();
    services
        .remote_store
        .set_channel(&channel, enabled, config_json.as_deref(), &now)
}

#[tauri::command]
pub fn pause_remote_channel(services: State<'_, AppState>, channel: String) -> Result<(), String> {
    let current = services
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|cfg| cfg.channel == channel)
        .ok_or_else(|| format!("未找到 {channel} channel 配置"))?;
    services.remote_hub.stop_channel(&channel);
    let now = crate::app_state::now_string();
    services.remote_store.set_channel_status(
        &channel,
        "paused",
        current.config_json.as_deref(),
        None,
        &now,
    )
}

#[tauri::command]
pub fn resume_remote_channel(app: tauri::AppHandle, channel: String) -> Result<(), String> {
    let services = app.state::<AppState>();
    let current = services
        .remote_store
        .list_channels()?
        .into_iter()
        .find(|cfg| cfg.channel == channel)
        .ok_or_else(|| format!("未找到 {channel} channel 配置"))?;
    let now = crate::app_state::now_string();
    services.remote_store.set_channel_status(
        &channel,
        "connecting",
        current.config_json.as_deref(),
        None,
        &now,
    )?;
    match crate::remote::start_channel_connector(&app, &services, &channel) {
        Ok(()) => services.remote_store.set_channel_status(
            &channel,
            "connected",
            current.config_json.as_deref(),
            None,
            &crate::app_state::now_string(),
        ),
        Err(err) => {
            let _ = services.remote_store.set_channel_status(
                &channel,
                "error",
                current.config_json.as_deref(),
                Some(&err),
                &crate::app_state::now_string(),
            );
            Err(err)
        }
    }
}

#[tauri::command]
pub fn disconnect_remote_channel(
    services: State<'_, AppState>,
    channel: String,
) -> Result<(), String> {
    services.remote_hub.stop_channel(&channel);
    crate::remote::open_secret_store(&services.app)?.clear(&channel)?;
    let now = crate::app_state::now_string();
    services.remote_store.set_awaiting_owner(&channel, false)?;
    services
        .remote_store
        .set_channel_status(&channel, "disconnected", None, None, &now)
}

#[tauri::command]
pub fn list_remote_allowlist(services: State<'_, AppState>) -> Result<Vec<AllowedPeer>, String> {
    services.remote_store.list_allowlist()
}

#[tauri::command]
pub fn add_remote_peer(
    services: State<'_, AppState>,
    channel: String,
    peer_id: String,
    label: Option<String>,
) -> Result<(), String> {
    let now = crate::app_state::now_string();
    services
        .remote_store
        .add_peer(&channel, &peer_id, label.as_deref(), &now)
}

#[tauri::command]
pub fn remove_remote_peer(
    services: State<'_, AppState>,
    channel: String,
    peer_id: String,
) -> Result<(), String> {
    services.remote_store.remove_peer(&channel, &peer_id)
}

#[tauri::command]
pub fn list_remote_bindings(services: State<'_, AppState>) -> Result<Vec<RemoteBinding>, String> {
    services.remote_store.list_all_bindings()
}

/// 发起微信扫码配对：返回二维码（前端展示），后台轮询并发 `remote_pairing_event`；
/// confirmed 自动存 token、enable、起 connector，无需手填 token。
#[tauri::command]
pub fn begin_remote_wechat_pairing(
    app: tauri::AppHandle,
) -> Result<crate::remote::QrCodeDto, String> {
    crate::remote::begin_wechat_pairing(&app)
}

/// 连接 Telegram：保存 BotFather token（入 secret store）+ enable channel + 置 awaiting_owner
/// （首个发消息者认作 owner）+ 立即启动 connector。token 非法时 connector 首轮 poll 会失败并记录。
#[tauri::command]
pub fn connect_remote_telegram(app: tauri::AppHandle, token: String) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("请填写 Telegram bot token".into());
    }
    let services = app.state::<AppState>();
    let now = crate::app_state::now_string();
    crate::remote::open_secret_store(&services.app)?.set("telegram", token.trim())?;
    let cfg = serde_json::json!({
        "baseUrl": crate::remote::channels::telegram::DEFAULT_BASE_URL
    })
    .to_string();
    services
        .remote_store
        .set_channel_status("telegram", "connecting", Some(&cfg), None, &now)?;
    services.remote_store.set_awaiting_owner("telegram", true)?;
    match crate::remote::start_telegram_connector(&app, &services) {
        Ok(()) => services.remote_store.set_channel_status(
            "telegram",
            "connected",
            Some(&cfg),
            None,
            &crate::app_state::now_string(),
        ),
        Err(err) => {
            let _ = services.remote_store.set_channel_status(
                "telegram",
                "error",
                Some(&cfg),
                Some(&err),
                &crate::app_state::now_string(),
            );
            Err(err)
        }
    }
}

/// 连接钉钉：保存 AppSecret（secret store）+ 写 AppKey/robotCode 到 config + enable +
/// 置 awaiting_owner + 立即启动 Stream connector。
#[tauri::command]
pub fn connect_remote_dingtalk(
    app: tauri::AppHandle,
    app_key: String,
    app_secret: String,
) -> Result<(), String> {
    if app_key.trim().is_empty() || app_secret.trim().is_empty() {
        return Err("请填写钉钉 AppKey 和 AppSecret".into());
    }
    let services = app.state::<AppState>();
    let now = crate::app_state::now_string();
    crate::remote::open_secret_store(&services.app)?.set("dingtalk", app_secret.trim())?;
    let cfg = serde_json::json!({
        "appKey": app_key.trim(),
        "robotCode": app_key.trim(),
        "apiBase": crate::remote::channels::dingtalk::API_BASE,
    })
    .to_string();
    services
        .remote_store
        .set_channel_status("dingtalk", "connecting", Some(&cfg), None, &now)?;
    services.remote_store.set_awaiting_owner("dingtalk", true)?;
    match crate::remote::start_dingtalk_connector(&app, &services) {
        Ok(()) => services.remote_store.set_channel_status(
            "dingtalk",
            "connected",
            Some(&cfg),
            None,
            &crate::app_state::now_string(),
        ),
        Err(err) => {
            let _ = services.remote_store.set_channel_status(
                "dingtalk",
                "error",
                Some(&cfg),
                Some(&err),
                &crate::app_state::now_string(),
            );
            Err(err)
        }
    }
}

/// 连接飞书：保存 AppSecret（secret store）+ 写 AppID 到 config + enable + 置 awaiting_owner
/// + 立即启动长连接 connector。需在飞书开放平台开启长连接订阅 im.message.receive_v1。
#[tauri::command]
pub fn connect_remote_feishu(
    app: tauri::AppHandle,
    app_id: String,
    app_secret: String,
) -> Result<(), String> {
    if app_id.trim().is_empty() || app_secret.trim().is_empty() {
        return Err("请填写飞书 AppID 和 AppSecret".into());
    }
    let services = app.state::<AppState>();
    let now = crate::app_state::now_string();
    crate::remote::open_secret_store(&services.app)?.set("feishu", app_secret.trim())?;
    let cfg = serde_json::json!({
        "appId": app_id.trim(),
        "domain": crate::remote::channels::feishu::DOMAIN,
    })
    .to_string();
    services
        .remote_store
        .set_channel_status("feishu", "connecting", Some(&cfg), None, &now)?;
    services.remote_store.set_awaiting_owner("feishu", true)?;
    match crate::remote::start_feishu_connector(&app, &services) {
        Ok(()) => services.remote_store.set_channel_status(
            "feishu",
            "connected",
            Some(&cfg),
            None,
            &crate::app_state::now_string(),
        ),
        Err(err) => {
            let _ = services.remote_store.set_channel_status(
                "feishu",
                "error",
                Some(&cfg),
                Some(&err),
                &crate::app_state::now_string(),
            );
            Err(err)
        }
    }
}
