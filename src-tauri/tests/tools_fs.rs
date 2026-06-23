use std::path::PathBuf;
use std::sync::Arc;

use silicon_agent::tools::{
    fs_tools::{ReadFile, WriteFile},
    registry::{cap_result, ToolRegistry},
    sandbox::resolve_in_workspace,
};

fn make_workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "silicon_agent_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp workspace");
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn test_write_then_read_file() {
    let ws = make_workspace();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ReadFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(WriteFile {
        workspace: ws.clone(),
    }));

    // write a file
    let write_args = serde_json::json!({
        "path": "hello.txt",
        "content": "hello world\nline 2"
    });
    let write_result = registry.execute("write_file", &write_args).unwrap();
    assert!(
        write_result.contains("已写入"),
        "unexpected write result: {write_result}"
    );

    // read it back
    let read_args = serde_json::json!({ "path": "hello.txt" });
    let read_result = registry.execute("read_file", &read_args).unwrap();
    assert!(
        read_result.contains("hello world"),
        "read result should contain written content, got: {read_result}"
    );
    assert!(
        read_result.contains("line 2"),
        "read result should contain second line, got: {read_result}"
    );
    // Should have line-range prefix
    assert!(
        read_result.contains("[0-"),
        "read result should contain line range prefix, got: {read_result}"
    );

    cleanup(&ws);
}

#[test]
fn test_write_file_creates_parent_dirs() {
    let ws = make_workspace();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(ReadFile {
        workspace: ws.clone(),
    }));

    let write_args = serde_json::json!({
        "path": "subdir/nested/file.txt",
        "content": "nested content"
    });
    let result = registry.execute("write_file", &write_args).unwrap();
    assert!(result.contains("已写入"), "got: {result}");

    let read_args = serde_json::json!({ "path": "subdir/nested/file.txt" });
    let read_result = registry.execute("read_file", &read_args).unwrap();
    assert!(read_result.contains("nested content"), "got: {read_result}");

    cleanup(&ws);
}

#[test]
fn test_sandbox_escape_returns_err() {
    let ws = make_workspace();

    let result = resolve_in_workspace(&ws, "../escape");
    assert!(
        result.is_err(),
        "expected Err for path escape, got Ok({:?})",
        result
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("越出工作区"),
        "error message should mention escape, got: {err_msg}"
    );

    cleanup(&ws);
}

#[test]
fn test_sandbox_absolute_escape_returns_err() {
    let ws = make_workspace();

    let result = resolve_in_workspace(&ws, "/etc/passwd");
    assert!(
        result.is_err(),
        "expected Err for absolute path outside workspace"
    );

    cleanup(&ws);
}

#[test]
fn test_sandbox_valid_path_ok() {
    let ws = make_workspace();

    let result = resolve_in_workspace(&ws, "some/file.txt");
    assert!(
        result.is_ok(),
        "expected Ok for valid path, got {:?}",
        result
    );
    let resolved = result.unwrap();
    assert!(resolved.starts_with(&ws));

    cleanup(&ws);
}

#[test]
fn test_cap_result_no_truncation_when_under_limit() {
    let text = "hello world";
    let result = cap_result(text, 8000);
    assert_eq!(result, text);
}

#[test]
fn test_cap_result_truncates_large_text() {
    // Build a string > 8000 chars
    let text: String = "x".repeat(9000);
    let result = cap_result(&text, 8000);
    assert!(
        result.contains("已截断"),
        "expected truncation marker, got length {}",
        result.len()
    );
    // The result should be significantly shorter than original
    assert!(
        result.chars().count() < 9000,
        "truncated result should be shorter"
    );
    // Should preserve head and tail
    assert!(
        result.starts_with('x'),
        "should start with original content"
    );
    assert!(result.ends_with('x'), "should end with tail content");
}

#[test]
fn test_read_file_with_offset_limit() {
    let ws = make_workspace();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WriteFile {
        workspace: ws.clone(),
    }));
    registry.register(Arc::new(ReadFile {
        workspace: ws.clone(),
    }));

    // Write 5 lines
    let content = "line0\nline1\nline2\nline3\nline4";
    let write_args = serde_json::json!({ "path": "paged.txt", "content": content });
    registry.execute("write_file", &write_args).unwrap();

    // Read with offset=1, limit=2 → should get line1, line2
    let read_args = serde_json::json!({ "path": "paged.txt", "offset": 1, "limit": 2 });
    let result = registry.execute("read_file", &read_args).unwrap();
    assert!(result.contains("line1"), "expected line1, got: {result}");
    assert!(result.contains("line2"), "expected line2, got: {result}");
    assert!(
        !result.contains("line0"),
        "should not contain line0, got: {result}"
    );
    assert!(
        !result.contains("line3"),
        "should not contain line3, got: {result}"
    );

    cleanup(&ws);
}

#[test]
fn test_registry_unknown_tool_returns_err() {
    let registry = ToolRegistry::new();
    let result = registry.execute("no_such_tool", &serde_json::json!({}));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("未知工具"));
}
