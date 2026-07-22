use std::sync::Mutex;

use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ModelSelection, ProviderCallError,
};
use silicon_worker::provider::fallback::call_with_fallback;

struct FlakyClient {
    fallback: Option<ModelSelection>,
    calls: Mutex<usize>,
}

impl ModelClient for FlakyClient {
    fn fallback_model(&self) -> Option<ModelSelection> {
        self.fallback.clone()
    }

    fn complete_model(&self, _r: ModelCallRequest) -> Result<ModelCallResult, ProviderCallError> {
        unimplemented!()
    }

    fn stream_model(&self, _r: ModelCallRequest) -> Result<ModelCallResult, ProviderCallError> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            Err(ProviderCallError::transient("primary down"))
        } else {
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "ok".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }

    fn stream_model_with_events(
        &self,
        r: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        _on: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        self.stream_model(r)
    }
}

fn req() -> ModelCallRequest {
    ModelCallRequest::default()
}

#[test]
fn primary_failure_falls_back_once() {
    let client = FlakyClient {
        fallback: Some(ModelSelection {
            provider_id: "prov-1".into(),
            model: "backup-model".into(),
        }),
        calls: Mutex::new(0),
    };
    let result = call_with_fallback(&client, req()).expect("fallback succeeds");
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
    assert_eq!(*client.calls.lock().unwrap(), 2, "应恰好调用两次（主+备）");
}

#[test]
fn no_fallback_propagates_error() {
    let client = FlakyClient {
        fallback: None,
        calls: Mutex::new(0),
    };
    assert!(call_with_fallback(&client, req()).is_err());
    assert_eq!(*client.calls.lock().unwrap(), 1, "无备用时只调一次");
}
