// 用伪造的 OpenAI 流式 SSE 行驱动归一化，验证 delta 累积 + tool_call 识别 + finish。不触网。
use silicon_worker::provider::call::normalize_chat_completion_response;
use silicon_worker::provider::call::normalize_chat_completion_stream_lines;
use silicon_worker::provider::ModelEvent;

#[test]
fn stream_lines_normalize_to_assistant_text() {
    let lines = vec![
        r#"data: {"choices":[{"delta":{"content":"你"}}]}"#.to_string(),
        r#"data: {"choices":[{"delta":{"content":"好"}}]}"#.to_string(),
        r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.to_string(),
        "data: [DONE]".to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("normalize");
    let has_completed = result.events.iter().any(|e| match e {
        ModelEvent::AssistantMessageCompleted { content } => content.contains("你好"),
        _ => false,
    });
    assert!(
        has_completed,
        "expected AssistantMessageCompleted with '你好', got: {:?}",
        result.events
    );
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
}

#[test]
fn stream_lines_normalize_tool_call() {
    let lines = vec![
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"read_file","arguments":"{\"path\""}}]}}]}"#.to_string(),
        r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"foo.txt\"}"}}]}}]}"#.to_string(),
        r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#.to_string(),
        "data: [DONE]".to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("normalize tool call");
    let tool_call = result
        .events
        .iter()
        .find(|e| matches!(e, ModelEvent::ToolCallCreated { .. }));
    assert!(
        tool_call.is_some(),
        "expected ToolCallCreated event, got: {:?}",
        result.events
    );
    if let Some(ModelEvent::ToolCallCreated {
        id,
        name,
        arguments_json,
    }) = tool_call
    {
        assert_eq!(id, "call_abc");
        assert_eq!(name, "read_file");
        assert!(
            arguments_json.contains("foo.txt"),
            "arguments: {arguments_json}"
        );
    }
    assert_eq!(result.finish_reason.as_deref(), Some("tool_calls"));
}

#[test]
fn stream_lines_malformed_chunk_skipped() {
    let lines = vec![
        "data: {bad json".to_string(),
        ": keep-alive".to_string(),
        r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#.to_string(),
        "data: [DONE]".to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("tolerant");
    let has_completed = result.events.iter().any(|e| match e {
        ModelEvent::AssistantMessageCompleted { content } => content == "hi",
        _ => false,
    });
    assert!(
        has_completed,
        "expected AssistantMessageCompleted with 'hi', got: {:?}",
        result.events
    );
}

#[test]
fn stream_lines_in_stream_error_surfaces_as_event() {
    let lines =
        vec![r#"data: {"error":{"message":"overloaded","type":"server_error"}}"#.to_string()];
    let result = normalize_chat_completion_stream_lines(lines).expect("ok");
    let has_error = result.events.iter().any(|e| match e {
        ModelEvent::Error { message } => message.contains("overloaded"),
        _ => false,
    });
    assert!(
        has_error,
        "expected Error event with 'overloaded', got: {:?}",
        result.events
    );
}

#[test]
fn stream_lines_done_terminates_early() {
    let lines = vec![
        r#"data: {"choices":[{"delta":{"content":"first"}}]}"#.to_string(),
        "data: [DONE]".to_string(),
        r#"data: {"choices":[{"delta":{"content":"second"}}]}"#.to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("done terminates");
    // Only "first" should appear in AssistantMessageCompleted; "second" is after [DONE]
    let completed_content = result.events.iter().find_map(|e| match e {
        ModelEvent::AssistantMessageCompleted { content } => Some(content.as_str()),
        _ => None,
    });
    assert_eq!(completed_content, Some("first"), "got: {:?}", result.events);
}

#[test]
fn non_stream_usage_parses_openai_cached_tokens() {
    let value = serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": "hi" }, "finish_reason": "stop" }],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 50,
            "prompt_tokens_details": { "cached_tokens": 800 }
        }
    });
    let result = normalize_chat_completion_response(value).expect("normalize");
    let usage = result.usage.expect("usage");
    assert_eq!(usage.input_tokens, Some(1000));
    assert_eq!(usage.output_tokens, Some(50));
    assert_eq!(usage.cache_read_tokens, Some(800));
    assert_eq!(usage.cache_create_tokens, None);
}

#[test]
fn non_stream_usage_parses_anthropic_compat_cache_fields() {
    let value = serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": "hi" }, "finish_reason": "stop" }],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 50,
            "cache_read_input_tokens": 700,
            "cache_creation_input_tokens": 120
        }
    });
    let result = normalize_chat_completion_response(value).expect("normalize");
    let usage = result.usage.expect("usage");
    assert_eq!(usage.cache_read_tokens, Some(700));
    assert_eq!(usage.cache_create_tokens, Some(120));
}

#[test]
fn stream_captures_trailing_usage_chunk_with_cache() {
    let lines = vec![
        r#"data: {"choices":[{"delta":{"content":"你好"}}]}"#.to_string(),
        r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.to_string(),
        r#"data: {"choices":[],"usage":{"prompt_tokens":1200,"completion_tokens":80,"prompt_tokens_details":{"cached_tokens":900}}}"#.to_string(),
        "data: [DONE]".to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("normalize");
    let usage = result.usage.expect("usage present");
    assert_eq!(usage.input_tokens, Some(1200));
    assert_eq!(usage.output_tokens, Some(80));
    assert_eq!(usage.cache_read_tokens, Some(900));
}

#[test]
fn stream_without_usage_chunk_keeps_usage_none() {
    let lines = vec![
        r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#.to_string(),
        "data: [DONE]".to_string(),
    ];
    let result = normalize_chat_completion_stream_lines(lines).expect("normalize");
    assert!(result.usage.is_none());
}
