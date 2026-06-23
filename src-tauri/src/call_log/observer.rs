use std::sync::Arc;

use crate::app_settings::AppSettingsStore;
use crate::call_log::{CallLogRecord, CallLogStore};
use crate::provider::client::{ModelEvent, ProviderErrorClass};
use crate::provider::gateway::{ModelCallObservation, ModelCallObserver};
use crate::session::new_id;

/// 把 gateway 观察数据落进 model_call_log。开关读 app_settings，写入 best-effort。
pub struct CallLogObserver {
    store: Arc<CallLogStore>,
    settings: Arc<AppSettingsStore>,
}

impl CallLogObserver {
    pub fn new(store: Arc<CallLogStore>, settings: Arc<AppSettingsStore>) -> Self {
        Self { store, settings }
    }
}

impl ModelCallObserver for CallLogObserver {
    fn enabled(&self) -> bool {
        self.settings.get_model_call_log_enabled().unwrap_or(false)
    }

    fn on_call(&self, obs: ModelCallObservation) {
        let now = crate::app_state::now_string();
        let a = &obs.attribution;
        let session_id = if a.session_id.is_empty() {
            None
        } else {
            Some(a.session_id.clone())
        };

        let mut rec = CallLogRecord {
            created_at: now,
            session_id,
            message_id: a.message_id.clone(),
            parent_session_id: a.parent_session_id.clone(),
            parent_tool_call_id: a.parent_tool_call_id.clone(),
            expert_name: a.expert_name.clone(),
            usage_type: a.usage_type.clone().unwrap_or_else(|| "other".to_string()),
            provider: obs.provider,
            model: obs.model,
            request_json: obs.request_json,
            response_text: None,
            response_tool_calls_json: None,
            reasoning_text: None,
            finish_reason: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_create_tokens: 0,
            latency_ms: obs.latency_ms,
            status: "ok".to_string(),
            error_message: None,
            error_class: None,
            http_status: None,
        };

        match &obs.outcome {
            Ok(result) => {
                // 最终文本 / tool_calls / reasoning 从 events 提取。
                let mut text = String::new();
                let mut reasoning = String::new();
                let mut tool_calls = Vec::new();
                for ev in &result.events {
                    match ev {
                        ModelEvent::AssistantMessageCompleted { content } => text = content.clone(),
                        ModelEvent::ThinkingDelta { text: t } => reasoning.push_str(t),
                        ModelEvent::ToolCallCreated {
                            id,
                            name,
                            arguments_json,
                        } => {
                            tool_calls.push(serde_json::json!({
                                "id": id, "name": name, "argumentsJson": arguments_json
                            }));
                        }
                        _ => {}
                    }
                }
                rec.response_text = if text.is_empty() { None } else { Some(text) };
                rec.reasoning_text = if reasoning.is_empty() {
                    None
                } else {
                    Some(reasoning)
                };
                rec.response_tool_calls_json = if tool_calls.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Array(tool_calls).to_string())
                };
                rec.finish_reason = result.finish_reason.clone();
                if let Some(u) = &result.usage {
                    rec.input_tokens = u.input_tokens.unwrap_or(0);
                    rec.output_tokens = u.output_tokens.unwrap_or(0);
                    rec.cache_read_tokens = u.cache_read_tokens.unwrap_or(0);
                    rec.cache_create_tokens = u.cache_create_tokens.unwrap_or(0);
                }
            }
            Err(e) => {
                rec.status = "error".to_string();
                rec.error_message = Some(e.message.clone());
                rec.error_class = Some(match e.class {
                    ProviderErrorClass::Transient => "transient".to_string(),
                    ProviderErrorClass::Terminal => "terminal".to_string(),
                });
                rec.http_status = e.http_status;
            }
        }

        let _ = self.store.record(&new_id("calllog"), &rec); // best-effort
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_log::CallLogFilter;
    use crate::provider::client::{ModelCallResult, ModelUsage, ProviderCallError};
    use crate::provider::message::ModelAttribution;
    use crate::storage::AppDatabase;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn setup() -> (Arc<CallLogStore>, Arc<AppSettingsStore>) {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("sw-calllog-obs-{nanos}-{seq}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("c.sqlite3")).unwrap());
        let store = Arc::new(CallLogStore::open(db.clone()).unwrap());
        let settings = Arc::new(AppSettingsStore::open(db).unwrap());
        (store, settings)
    }

    fn obs(outcome: Result<ModelCallResult, ProviderCallError>) -> ModelCallObservation {
        ModelCallObservation {
            provider: "openai".into(),
            model: "gpt-x".into(),
            attribution: ModelAttribution {
                usage_type: Some("main_agent".into()),
                ..Default::default()
            },
            request_json: "{\"messages\":[]}".into(),
            outcome,
            latency_ms: 42,
        }
    }

    #[test]
    fn enabled_reflects_setting() {
        let (store, settings) = setup();
        let o = CallLogObserver::new(store, settings.clone());
        assert!(!o.enabled());
        settings.set_model_call_log_enabled(true).unwrap();
        assert!(o.enabled());
    }

    #[test]
    fn records_ok_with_usage() {
        let (store, settings) = setup();
        let o = CallLogObserver::new(store.clone(), settings);
        let result = ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "hi".into(),
            }],
            usage: Some(ModelUsage {
                input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_tokens: Some(2),
                cache_create_tokens: Some(0),
            }),
            finish_reason: Some("stop".into()),
        };
        o.on_call(obs(Ok(result)));
        let rows = store.list(&CallLogFilter::default()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].usage_type, "main_agent");
        assert_eq!(rows[0].output_tokens, 5);
        assert_eq!(rows[0].status, "ok");
    }

    #[test]
    fn records_error_outcome() {
        let (store, settings) = setup();
        let o = CallLogObserver::new(store.clone(), settings);
        o.on_call(obs(Err(ProviderCallError::transient("boom").with_status(503))));
        let rows = store.list(&CallLogFilter::default()).unwrap();
        let d = store.get(&rows[0].id).unwrap().unwrap();
        assert_eq!(d.status, "error");
        assert_eq!(d.error_class.as_deref(), Some("transient"));
        assert_eq!(d.http_status, Some(503));
    }

    #[test]
    fn empty_usage_type_falls_back_to_other() {
        let (store, settings) = setup();
        let o = CallLogObserver::new(store.clone(), settings);
        let mut ob = obs(Ok(ModelCallResult {
            events: vec![],
            usage: None,
            finish_reason: None,
        }));
        ob.attribution.usage_type = None;
        o.on_call(ob);
        assert_eq!(
            store.list(&CallLogFilter::default()).unwrap()[0].usage_type,
            "other"
        );
    }
}
