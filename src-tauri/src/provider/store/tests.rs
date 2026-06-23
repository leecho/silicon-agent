use crate::provider::adapter::{
    authorization_header_value, build_chat_completion_body, classify_status,
    emit_stream_line_delta, friendly_error_message, stream_read_timeout_ms, ToolCallStreamAcc,
};
use crate::provider::client::{ModelCallRequest, ModelEvent};
use crate::provider::message::{
    ModelAttribution, ModelMessage, ModelResponseSchema, ModelToolChoice, ToolSpecForModel,
};
use crate::provider::model::{ModelInput, ProviderInput};
use crate::storage::AppDatabase;
use std::sync::Arc;

fn temp_gateway() -> super::ProviderStore {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sw-gw-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Arc::new(AppDatabase::open(dir.join("t.sqlite3")).unwrap());
    super::ProviderStore::open(db, dir).unwrap()
}

#[test]
fn context_limit_for_prefers_configured_then_table() {
    let gw = temp_gateway();
    let p = gw
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "P".into(),
                base_url: "http://x/v1".into(),
                api_key: Some("sk-aaaa1111".into()),
                enabled: true,
            },
            "1",
        )
        .unwrap();
    // 无配置覆盖 → 内置查表：claude=200k、未知名=默认 128k。
    assert_eq!(gw.context_limit_for("claude-3-5-sonnet"), 200_000);
    assert_eq!(gw.context_limit_for("totally-unknown-xyz"), 128_000);
    // 配置覆盖 → 用覆盖值（即便表会给别的值）。
    gw.upsert_model(
        ModelInput {
            id: None,
            provider_id: p.id.clone(),
            model: "claude-custom".into(),
            display_name: None,
            enabled: true,
            context_limit: Some(64_000),
        },
        "1",
    )
    .unwrap();
    assert_eq!(gw.context_limit_for("claude-custom"), 64_000);
}

#[test]
fn friendly_error_maps_known_provider_errors() {
    // 用户给的真实样例：DeepSeek 余额不足。
    let balance = r#"{"message":"Insufficient Balance","type":"unknown_error","param":null,"code":"invalid_request_error"}"#;
    assert_eq!(
        friendly_error_message(balance, Some(402)),
        "余额不足，请充值后重试。"
    );
    // 401 鉴权。
    assert_eq!(
        friendly_error_message(r#"{"error":{"message":"Invalid API key"}}"#, Some(401)),
        "API Key 无效或未授权，请检查密钥配置。"
    );
    // 429 限流。
    assert_eq!(
        friendly_error_message("Rate limit exceeded", Some(429)),
        "请求过于频繁（限流），请稍后重试。"
    );
    // 上下文超限。
    assert_eq!(
        friendly_error_message(
            r#"{"error":{"message":"This model's maximum context length is 128000 tokens"}}"#,
            Some(400)
        ),
        "上下文超出模型上限，请压缩历史或新开会话。"
    );
}

#[test]
fn friendly_error_falls_back_to_message_or_body() {
    // 未知 code：回退提取到的 message。
    assert_eq!(
        friendly_error_message(r#"{"message":"Something odd happened"}"#, Some(400)),
        "Something odd happened"
    );
    // 非 JSON：回退原始片段。
    assert_eq!(
        friendly_error_message("plain text boom", Some(400)),
        "plain text boom"
    );
    // 空 body：按状态码兜底。
    assert_eq!(
        friendly_error_message("", Some(500)),
        "请求失败（HTTP 500）"
    );
}

#[test]
fn http_429_and_5xx_classify_transient_others_terminal() {
    use crate::provider::client::ProviderErrorClass;
    assert_eq!(classify_status(429), ProviderErrorClass::Transient);
    assert_eq!(classify_status(503), ProviderErrorClass::Transient);
    assert_eq!(classify_status(408), ProviderErrorClass::Transient);
    assert_eq!(classify_status(401), ProviderErrorClass::Terminal);
    assert_eq!(classify_status(400), ProviderErrorClass::Terminal);
}

#[test]
fn stream_read_timeout_uses_request_timeout_then_idle_default() {
    // 流式读超时即 idle 超时：优先用 request.timeout_ms，缺省 60s。
    assert_eq!(stream_read_timeout_ms(Some(30_000)), 30_000);
    assert_eq!(stream_read_timeout_ms(None), 60_000);
}

#[test]
fn stream_tool_call_chunks_emit_accumulating_arguments_for_feed() {
    // provider 流式 tool_calls：首帧带 name + 部分 args，后续帧仅 args 分片；
    // 每帧都 emit 一个带"累积 args"的 ToolCallCreated，驱动前端实时预览参数生成。
    let mut acc = ToolCallStreamAcc::default();

    let line1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"call-1","index":0,"function":{"name":"file_read","arguments":"{\"path\""}}]}}]}"#;
    let mut e1 = Vec::new();
    emit_stream_line_delta(line1, &mut acc, &mut |event| {
        e1.push(event);
        true
    })
    .expect("emit1");
    assert_eq!(e1.len(), 1);
    match &e1[0] {
        ModelEvent::ToolCallCreated {
            id,
            name,
            arguments_json,
        } => {
            assert_eq!(id, "call-1");
            assert_eq!(name, "file_read");
            assert_eq!(arguments_json, "{\"path\"");
        }
        other => panic!("expected ToolCallCreated, got {other:?}"),
    }

    // 后续仅 arguments 片段（无 name）：用 index 关联，emit 累积后的完整 args。
    let line2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"/tmp/a.md\"}"}}]}}]}"#;
    let mut e2 = Vec::new();
    emit_stream_line_delta(line2, &mut acc, &mut |event| {
        e2.push(event);
        true
    })
    .expect("emit2");
    assert_eq!(e2.len(), 1);
    match &e2[0] {
        ModelEvent::ToolCallCreated {
            id,
            name,
            arguments_json,
        } => {
            assert_eq!(id, "call-1");
            assert_eq!(name, "file_read");
            assert_eq!(arguments_json, "{\"path\":\"/tmp/a.md\"}");
        }
        other => panic!("expected ToolCallCreated, got {other:?}"),
    }
}

#[test]
fn authorization_header_uses_plaintext_secret_for_provider_call() {
    let header = authorization_header_value("  sk-plain-secret-1234\n");

    assert_eq!(header, "Bearer sk-plain-secret-1234");
    assert!(!header.contains("****"));
}

fn structured_request(schema: ModelResponseSchema) -> ModelCallRequest {
    ModelCallRequest {
        messages: vec![
            ModelMessage::system("Return JSON."),
            ModelMessage::user("hi"),
        ],
        tools: Vec::new(),
        tool_choice: ModelToolChoice::None,
        response_schema: schema,
        attribution: ModelAttribution {
            session_id: "session-1".into(),
            ..Default::default()
        },
        max_output_tokens: Some(1200),
        timeout_ms: None,
        stream: true,
        model_selection: None,
    }
}

#[test]
fn structured_schema_requests_enable_provider_json_mode() {
    let body = build_chat_completion_body(
        "deepseek-v4-flash",
        &structured_request(ModelResponseSchema::AgentFinalOutput),
        true,
    );

    assert_eq!(
        body.pointer("/response_format/type")
            .and_then(|value| value.as_str()),
        Some("json_object")
    );
}

#[test]
fn freeform_assistant_requests_omit_response_format() {
    let body = build_chat_completion_body(
        "deepseek-v4-flash",
        &structured_request(ModelResponseSchema::FreeformAssistant),
        true,
    );

    assert!(body.get("response_format").is_none());
}

#[test]
fn stream_requests_enable_include_usage() {
    let body = build_chat_completion_body(
        "deepseek-v4-flash",
        &structured_request(ModelResponseSchema::FreeformAssistant),
        true,
    );
    assert_eq!(
        body.pointer("/stream_options/include_usage")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn non_stream_requests_omit_stream_options() {
    let body = build_chat_completion_body(
        "deepseek-v4-flash",
        &structured_request(ModelResponseSchema::FreeformAssistant),
        false,
    );
    assert!(body.get("stream_options").is_none());
}

#[test]
fn function_tools_are_always_sent_regardless_of_model() {
    // MVP 统一：function tools 一律按支持处理，不再为 search 模型抑制。
    let body = build_chat_completion_body(
        "gpt-4o-search-preview",
        &ModelCallRequest {
            messages: vec![
                ModelMessage::system("Return JSON."),
                ModelMessage::user("写一份报告"),
            ],
            tools: vec![ToolSpecForModel::json_schema(
                "local_file_write_artifact",
                "Write a local artifact",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "fileName": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["fileName", "content"],
                    "additionalProperties": false
                }),
                "file_write",
                "medium",
            )],
            tool_choice: ModelToolChoice::Auto,
            response_schema: ModelResponseSchema::AgentFinalOutput,
            attribution: ModelAttribution {
                session_id: "session-1".into(),
                ..Default::default()
            },
            max_output_tokens: Some(1200),
            timeout_ms: None,
            stream: true,
            model_selection: None,
        },
        true,
    );

    assert!(body.get("web_search_options").is_none());
    assert_eq!(
        body.pointer("/tools/0/function/name")
            .and_then(|value| value.as_str()),
        Some("local_file_write_artifact")
    );
    assert_eq!(
        body.get("tool_choice").and_then(|value| value.as_str()),
        Some("auto")
    );
}

#[test]
fn provider_model_crud_and_default_resolution() {
    let gw = temp_gateway();
    // 建厂商 + 写 key。
    let p = gw
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "DeepSeek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: Some("sk-test1234".into()),
                enabled: true,
            },
            "1",
        )
        .unwrap();
    assert!(p.has_secret);
    assert_eq!(p.secret_hint.as_deref(), Some("****1234"));

    // 建两个模型，设第二个为默认。
    let m1 = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "deepseek-chat".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    let m2 = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "deepseek-reasoner".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    gw.set_default_model(&m2.id, "1").unwrap();

    // 解析：None → 默认模型 m2。
    let def = gw.resolve_selection(None).unwrap();
    assert_eq!(def.model, "deepseek-reasoner");
    assert_eq!(def.provider_id, p.id);
    // 解析：指定 m1。
    let r1 = gw.resolve_selection(Some(&m1.id)).unwrap();
    assert_eq!(r1.model, "deepseek-chat");

    // set_default_model 唯一性：默认只剩 m2。
    let defaults: Vec<_> = gw
        .list_models(&p.id)
        .unwrap()
        .into_iter()
        .filter(|m| m.is_default)
        .map(|m| m.id)
        .collect();
    assert_eq!(defaults, vec![m2.id.clone()]);

    // 停用默认所属厂商 → 解析回退失败（无其它可用）。
    gw.set_provider_enabled(&p.id, false, "1").unwrap();
    assert!(gw.resolve_selection(None).is_err());
}

#[test]
fn deleting_model_clears_default_reference() {
    let gw = temp_gateway();
    let p = gw
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "P".into(),
                base_url: "http://x/v1".into(),
                api_key: Some("sk-zzzz9999".into()),
                enabled: true,
            },
            "1",
        )
        .unwrap();
    let m = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "m".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    gw.set_default_model(&m.id, "1").unwrap();
    gw.delete_model(&m.id).unwrap();
    assert!(gw.list_models(&p.id).unwrap().is_empty());
    assert!(gw.resolve_selection(None).is_err());
}

#[test]
fn delete_provider_cascades_models_and_clears_refs() {
    let gw = temp_gateway();
    let p = gw
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "P".into(),
                base_url: "http://x/v1".into(),
                api_key: Some("sk-aaaa1111".into()),
                enabled: true,
            },
            "1",
        )
        .unwrap();
    let m1 = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "m1".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    let m2 = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "m2".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    gw.set_default_model(&m1.id, "1").unwrap();
    gw.set_fallback_model(Some(&m1.id)).unwrap();
    let _ = m2;

    gw.delete_provider(&p.id).unwrap();

    assert!(gw.list_models(&p.id).unwrap().is_empty());
    assert_eq!(gw.get_fallback_model_id().unwrap(), None);
    assert!(gw.list_providers().unwrap().iter().all(|pv| pv.id != p.id));
}

#[test]
fn set_default_model_unknown_id_errors_and_preserves_default() {
    let gw = temp_gateway();
    let p = gw
        .upsert_provider(
            ProviderInput {
                id: None,
                name: "P".into(),
                base_url: "http://x/v1".into(),
                api_key: Some("sk-aaaa1111".into()),
                enabled: true,
            },
            "1",
        )
        .unwrap();
    let m = gw
        .upsert_model(
            ModelInput {
                id: None,
                provider_id: p.id.clone(),
                model: "m".into(),
                display_name: None,
                enabled: true,
                context_limit: None,
            },
            "1",
        )
        .unwrap();
    gw.set_default_model(&m.id, "1").unwrap();

    assert!(gw.set_default_model("mdl_does_not_exist", "1").is_err());

    // 原默认仍然保留。
    let defaults: Vec<_> = gw
        .list_models(&p.id)
        .unwrap()
        .into_iter()
        .filter(|m| m.is_default)
        .map(|m| m.id)
        .collect();
    assert_eq!(defaults, vec![m.id.clone()]);
}

#[test]
fn migrate_legacy_imports_row_and_drops_table() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sw-gw-mig-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("t.sqlite3");
    let db = Arc::new(AppDatabase::open(&db_path).unwrap());
    // 手工建旧表并写一行（含 NOT NULL 列：id/provider/base_url/model/updated_at）。
    db.with_connection(|c| {
            c.execute_batch(
                "create table provider_config (
                    id text primary key,
                    provider text not null,
                    base_url text not null,
                    model text not null,
                    fallback_model text,
                    has_secret integer not null default 0,
                    secret_hint text,
                    last_check_status text,
                    last_check_detail text,
                    last_check_at text,
                    updated_at text not null
                );
                insert into provider_config (id, provider, base_url, model, fallback_model, updated_at)
                values ('default', 'DeepSeek', 'https://api.deepseek.com/v1', 'deepseek-chat', 'deepseek-reasoner', '1');",
            )?;
            Ok(())
        })
        .unwrap();
    // 写旧单密钥明文文件。
    std::fs::write(dir.join("provider.secret"), "sk-legacy-9999\n").unwrap();

    let gw = super::ProviderStore::open(db.clone(), &dir).unwrap();

    // 一个厂商，含密钥。
    let providers = gw.list_providers().unwrap();
    assert_eq!(providers.len(), 1);
    let p = &providers[0];
    assert_eq!(p.name, "DeepSeek");
    assert!(p.has_secret);
    // 默认模型匹配旧 model。
    let def = gw.resolve_selection(None).unwrap();
    assert_eq!(def.model, "deepseek-chat");
    // fallback 已设置且指向 reasoner 模型。
    let fb_id = gw.get_fallback_model_id().unwrap().expect("fallback set");
    let fb = gw.resolve_selection(Some(&fb_id)).unwrap();
    assert_eq!(fb.model, "deepseek-reasoner");
    // 旧表已删除。
    let legacy_count: i64 = db
        .with_connection(|c| {
            Ok(c.query_row(
                "select count(*) from sqlite_master where name='provider_config'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(legacy_count, 0);
}

#[test]
fn migrate_legacy_no_table_is_noop() {
    // 无旧表时 open() 成功且无副作用。temp_gateway 已 open，再次 open 仍可。
    let gw = temp_gateway();
    assert!(gw.list_providers().unwrap().is_empty());
}

#[test]
fn fallback_id_migrates_from_legacy_app_settings_on_open() {
    // 模拟旧库：fallback_model_id 存在共享表 app_settings。open() 应把它搬到 provider_settings，
    // 既有用户的 fallback 设置不丢失（幂等）。
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sw-gw-fbmig-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Arc::new(AppDatabase::open(dir.join("t.sqlite3")).unwrap());
    db.with_connection(|c| {
            c.execute_batch(
                "create table if not exists app_settings (key text primary key, value text not null);
                 insert into app_settings (key, value) values ('fallback_model_id', 'mdl_legacy_fb');",
            )?;
            Ok(())
        })
        .unwrap();

    let gw = super::ProviderStore::open(db.clone(), &dir).unwrap();
    assert_eq!(
        gw.get_fallback_model_id().unwrap(),
        Some("mdl_legacy_fb".to_string()),
        "旧 app_settings 里的 fallback 应迁入 provider_settings"
    );

    // 幂等：再次 open 不报错、值不变。
    let gw2 = super::ProviderStore::open(db, &dir).unwrap();
    assert_eq!(
        gw2.get_fallback_model_id().unwrap(),
        Some("mdl_legacy_fb".to_string())
    );
}
