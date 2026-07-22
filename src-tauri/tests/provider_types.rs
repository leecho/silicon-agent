use silicon_worker::provider::{ProviderCallError, ProviderErrorClass};

#[test]
fn transient_error_is_classified_for_retry() {
    let err = ProviderCallError::transient("429 rate limited");
    assert_eq!(err.class, ProviderErrorClass::Transient);
}

#[test]
fn model_event_variants_exist() {
    // 确认 ModelEvent 枚举可构造（编译期即验证 API 面）。
    use silicon_worker::provider::ModelEvent;
    let _ = ModelEvent::Delta { text: "hi".into() };
    let _ = ModelEvent::ToolCallCreated {
        id: "1".into(),
        name: "read_file".into(),
        arguments_json: "{}".into(),
    };
}
