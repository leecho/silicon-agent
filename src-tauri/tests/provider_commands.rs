//! Provider 命令逻辑测试（不触网）。
//!
//! Tauri 多模型命令（`list_providers`/`upsert_provider`/`set_default_model` 等）是
//! ProviderStore 的薄包装；真实命令需要 Tauri runtime 才能构造 `State<AppState>`，
//! 故此处直接对命令所委托的 ProviderStore 行为做往返断言（未配置→空/解析失败、
//! 保存后→去敏 view）。`test_provider`/`fetch_provider_models` 的真实网络路径不在单测覆盖，
//! 由设置页手动验证。

use std::sync::Arc;

use silicon_agent::provider::{ModelInput, ProviderInput, ProviderStore};
use silicon_agent::storage::AppDatabase;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "silicon-agent-{tag}_{}_{}_{nanos}",
        std::process::id(),
        seq,
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open_store(dir: &std::path::Path) -> ProviderStore {
    let db = Arc::new(AppDatabase::open(dir.join("app.db")).expect("open db"));
    ProviderStore::open(db, dir.to_path_buf()).expect("open gateway")
}

#[test]
fn list_providers_empty_and_resolution_fails_when_unconfigured() {
    let dir = temp_dir("commands-unconfigured");
    let gateway = open_store(&dir);

    // 未配置时（命令委托 list_providers）应为空。
    assert!(
        gateway.list_providers().expect("list providers").is_empty(),
        "未配置时 list_providers 应为空"
    );
    // 无可用模型时解析（命令调用路径）应报错。
    assert!(
        gateway.resolve_selection(None).is_err(),
        "未配置时 resolve_selection 应失败"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn upsert_then_list_returns_redacted_view() {
    let dir = temp_dir("commands-roundtrip");
    let gateway = open_store(&dir);

    // upsert_provider 委托写库（命令侧传 now_string）。
    let p = gateway
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-plain-secret-1234".into()),
                enabled: true,
            },
            "1717545600",
        )
        .expect("upsert provider");
    let m = gateway
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "deepseek-chat".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1717545600",
        )
        .expect("upsert model");
    gateway
        .set_default_model(&m.id, "1717545600")
        .expect("set default");

    // list_providers 返回去敏 view。
    let view = gateway
        .list_providers()
        .expect("list providers")
        .into_iter()
        .find(|p| p.id == p.id.clone())
        .expect("provider present after save");

    assert_eq!(view.name, "DeepSeek");
    assert_eq!(view.base_url, "https://api.deepseek.com/v1");
    assert!(view.has_secret, "保存了 api_key 应标记 has_secret");

    // 解析默认应得到 deepseek-chat。
    let resolved = gateway.resolve_selection(None).expect("resolve default");
    assert_eq!(resolved.model, "deepseek-chat");
    assert_eq!(resolved.provider_id, p.id);

    // 去敏：序列化后的 view 不得回显明文 api_key。
    let serialized = serde_json::to_string(&view).expect("serialize view");
    assert!(
        !serialized.contains("sk-plain-secret-1234"),
        "view 不得回显明文 api_key: {serialized}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
