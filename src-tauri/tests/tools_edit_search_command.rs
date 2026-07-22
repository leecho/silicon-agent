use std::path::PathBuf;
use std::sync::Arc;

use silicon_worker::tools::{
    command_tool::CommandExecute,
    fs_search::{Glob, Grep},
    fs_tools::{EditFile, WriteFile},
    registry::ToolRegistry,
};

fn make_workspace() -> PathBuf {
    // 进程内原子计数器保证唯一：同二进制内多 #[test] 并行、可能同纳秒，仅 pid+nanos 会撞名
    // 共享目录、互相 cleanup 删文件（test_glob_finds_files 间歇失败的根因）。
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "silicon_worker_t2_test_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("failed to create temp workspace");
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

fn registry(ws: &PathBuf) -> ToolRegistry {
    let mut r = ToolRegistry::new();
    r.register(Arc::new(WriteFile {
        workspace: ws.clone(),
    }));
    r.register(Arc::new(EditFile {
        workspace: ws.clone(),
    }));
    r.register(Arc::new(Glob {
        workspace: ws.clone(),
    }));
    r.register(Arc::new(Grep {
        workspace: ws.clone(),
    }));
    r.register(Arc::new(CommandExecute {
        workspace: ws.clone(),
    }));
    r
}

#[test]
fn test_edit_file_replaces_text() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "a.txt", "content": "hello world\nfoo bar"}),
    )
    .unwrap();

    let result = reg
        .execute(
            "edit_file",
            &serde_json::json!({"path": "a.txt", "old_text": "world", "new_text": "rust"}),
        )
        .unwrap();
    assert!(result.contains("已编辑"), "got: {result}");
    assert!(result.contains("1 处"), "got: {result}");

    let read_back = std::fs::read_to_string(ws.join("a.txt")).unwrap();
    assert_eq!(read_back, "hello rust\nfoo bar");

    cleanup(&ws);
}

#[test]
fn test_edit_file_non_unique_errors() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "dup.txt", "content": "x\nx\nx"}),
    )
    .unwrap();

    let result = reg.execute(
        "edit_file",
        &serde_json::json!({"path": "dup.txt", "old_text": "x", "new_text": "y"}),
    );
    assert!(
        result.is_err(),
        "expected err for non-unique, got: {result:?}"
    );
    assert!(result.unwrap_err().contains("不唯一"));

    cleanup(&ws);
}

#[test]
fn test_edit_file_replace_all() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "dup.txt", "content": "x\nx\nx"}),
    )
    .unwrap();

    let result = reg
        .execute(
            "edit_file",
            &serde_json::json!({"path": "dup.txt", "old_text": "x", "new_text": "y", "replace_all": true}),
        )
        .unwrap();
    assert!(result.contains("3 处"), "got: {result}");
    let read_back = std::fs::read_to_string(ws.join("dup.txt")).unwrap();
    assert_eq!(read_back, "y\ny\ny");

    cleanup(&ws);
}

#[test]
fn test_glob_finds_files() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}),
    )
    .unwrap();
    reg.execute(
        "write_file",
        &serde_json::json!({"path": "src/lib.rs", "content": "// lib"}),
    )
    .unwrap();
    reg.execute(
        "write_file",
        &serde_json::json!({"path": "notes.txt", "content": "note"}),
    )
    .unwrap();

    let result = reg
        .execute("glob", &serde_json::json!({"pattern": "*.rs"}))
        .unwrap();
    assert!(result.contains("main.rs"), "got: {result}");
    assert!(result.contains("lib.rs"), "got: {result}");
    assert!(!result.contains("notes.txt"), "got: {result}");

    cleanup(&ws);
}

#[test]
fn test_glob_ignores_target_dir() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "keep.rs", "content": "a"}),
    )
    .unwrap();
    reg.execute(
        "write_file",
        &serde_json::json!({"path": "target/skip.rs", "content": "b"}),
    )
    .unwrap();

    let result = reg
        .execute("glob", &serde_json::json!({"pattern": "*.rs"}))
        .unwrap();
    assert!(result.contains("keep.rs"), "got: {result}");
    assert!(
        !result.contains("skip.rs"),
        "should ignore target/, got: {result}"
    );

    cleanup(&ws);
}

#[test]
fn test_grep_finds_matching_lines() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "code.rs", "content": "let foo = 1;\nlet bar = 2;\nfoo();"}),
    )
    .unwrap();

    let result = reg
        .execute("grep", &serde_json::json!({"pattern": "foo"}))
        .unwrap();
    assert!(result.contains("code.rs:1:"), "got: {result}");
    assert!(result.contains("code.rs:3:"), "got: {result}");
    assert!(
        !result.contains(":2:"),
        "should not match line 2, got: {result}"
    );

    cleanup(&ws);
}

#[test]
fn test_grep_case_insensitive() {
    let ws = make_workspace();
    let reg = registry(&ws);

    reg.execute(
        "write_file",
        &serde_json::json!({"path": "c.txt", "content": "Hello\nWORLD"}),
    )
    .unwrap();

    let result = reg
        .execute(
            "grep",
            &serde_json::json!({"pattern": "world", "case_insensitive": true}),
        )
        .unwrap();
    assert!(result.contains("c.txt:2:WORLD"), "got: {result}");

    cleanup(&ws);
}

#[test]
fn test_run_command_echo() {
    let ws = make_workspace();
    let reg = registry(&ws);

    let result = reg
        .execute(
            "run_command",
            &serde_json::json!({"program": "echo", "args": ["hello"]}),
        )
        .unwrap();
    assert!(result.contains("hello"), "got: {result}");
    assert!(result.contains("退出码: 0"), "got: {result}");

    cleanup(&ws);
}

#[test]
fn test_run_command_timeout() {
    let ws = make_workspace();
    let reg = registry(&ws);

    let result = reg
        .execute(
            "run_command",
            &serde_json::json!({"program": "sleep", "args": ["10"], "timeout_ms": 1000}),
        )
        .unwrap_err();
    assert!(result.contains("超时"), "got: {result}");

    cleanup(&ws);
}
