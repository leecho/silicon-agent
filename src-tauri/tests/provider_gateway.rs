use std::sync::Arc;

use silicon_worker::provider::{ModelInput, ProviderGateway, ProviderInput, ProviderStore};
use silicon_worker::storage::AppDatabase;

/// 每次测试用独立临时目录，避免数据库/secret 文件互相污染。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "silicon-worker-{tag}_{}_{}_{nanos}",
        std::process::id(),
        seq,
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open_store(dir: &std::path::Path) -> Arc<ProviderStore> {
    let db = Arc::new(AppDatabase::open(dir.join("app.db")).expect("open db"));
    Arc::new(ProviderStore::open(db, dir.to_path_buf()).expect("open store"))
}

#[test]
fn upsert_provider_round_trips_through_list() {
    let dir = temp_dir("gateway-roundtrip");
    let store = open_store(&dir);

    let view = store
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-plain-secret-1234".into()),
                enabled: true,
                protocol: "openai".into(),
            },
            "2026-06-05T00:00:00Z",
        )
        .expect("upsert provider");

    // 写入返回的 view 与读回的 view 一致。
    let reloaded = store
        .list_providers()
        .expect("list providers")
        .into_iter()
        .find(|p| p.id == view.id)
        .expect("provider present after save");

    assert_eq!(reloaded.name, "DeepSeek");
    assert_eq!(reloaded.base_url, "https://api.deepseek.com/v1");
    assert!(reloaded.has_secret, "保存了 api_key 应标记 has_secret");
    assert_eq!(view.has_secret, reloaded.has_secret);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn provider_view_never_exposes_plaintext_api_key() {
    let dir = temp_dir("gateway-redact");
    let store = open_store(&dir);

    let view = store
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-plain-secret-1234".into()),
                enabled: true,
                protocol: "openai".into(),
            },
            "2026-06-05T00:00:00Z",
        )
        .expect("upsert provider");

    // 去敏：序列化后的 view 不得包含明文密钥；仅暴露掩码 hint。
    let serialized = serde_json::to_string(&view).expect("serialize view");
    assert!(
        !serialized.contains("sk-plain-secret-1234"),
        "view 不得回显明文 api_key: {serialized}"
    );
    assert_eq!(view.secret_hint.as_deref(), Some("****1234"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn fallback_model_returns_configured_backup() {
    use silicon_worker::provider::ModelClient;
    let dir = temp_dir("gateway-fallback");
    let store = open_store(&dir);
    let gateway = ProviderGateway::new(store.clone());

    assert!(gateway.fallback_model().is_none(), "未配置时无备用模型");

    let p = store
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-plain-secret-1234".into()),
                enabled: true,
                protocol: "openai".into(),
            },
            "2026-06-05T00:00:00Z",
        )
        .expect("upsert provider");
    let fallback = store
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "deepseek-reasoner".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
                supports_vision: None,
            },
            "2026-06-05T00:00:00Z",
        )
        .expect("upsert model");
    store
        .set_fallback_model(Some(&fallback.id))
        .expect("set fallback");

    let selection = gateway.fallback_model().expect("fallback selection");
    assert_eq!(selection.model, "deepseek-reasoner");
    assert_eq!(selection.provider_id, p.id);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn active_model_provider_reflects_default_model() {
    use silicon_worker::provider::client::ModelClient;
    let dir = temp_dir("gateway-active-model");
    let store = open_store(&dir);
    let gateway = ProviderGateway::new(store.clone());
    let p = store
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-plain-secret-1234".into()),
                enabled: true,
                protocol: "openai".into(),
            },
            "100",
        )
        .expect("upsert provider");
    let m = store
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "deepseek-chat".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
                supports_vision: None,
            },
            "100",
        )
        .expect("upsert model");
    store.set_default_model(&m.id, "100").expect("set default");

    let pair = gateway.active_model_provider().expect("active pair");
    assert_eq!(pair, ("DeepSeek".to_string(), "deepseek-chat".to_string()));
    std::fs::remove_dir_all(&dir).ok();
}
