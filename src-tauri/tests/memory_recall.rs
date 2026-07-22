// MemoryStore::recall —— FTS5 trigram 检索式召回（W1 切片2）。
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-recall_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    MemoryStore::open(db).expect("store")
}

#[test]
fn recall_returns_only_relevant_facts() {
    let s = open();
    s.add_memory("用户喜欢简洁的回答", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目使用 Rust 与 Tauri", "101", MemoryScope::Global)
        .unwrap();
    s.add_memory("部署在 Kubernetes 集群", "102", MemoryScope::Global)
        .unwrap();

    // query 命中「Rust」相关，应召回该条，不应包含无关的「简洁」「Kubernetes」。
    let hits = s
        .recall("我们项目用的 Rust 怎么编译", 12, MemoryScope::Global)
        .unwrap();
    let joined = hits
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(joined.contains("Rust"), "应召回 Rust 相关，实际：{joined}");
    assert!(
        !joined.contains("Kubernetes"),
        "不应召回无关项，实际：{joined}"
    );
}

#[test]
fn recall_cjk_substring() {
    let s = open();
    s.add_memory("用户喜欢简洁的回答", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目使用 Rust", "101", MemoryScope::Global)
        .unwrap();
    // trigram 需 ≥3 字符共同子串；query 与记忆共享「简洁的」。
    let hits = s
        .recall("回答请简洁的一点", 12, MemoryScope::Global)
        .unwrap();
    assert!(hits.iter().any(|m| m.content.contains("简洁")));
}

#[test]
fn empty_query_falls_back_to_recent_facts() {
    let s = open();
    s.add_memory("事实一", "100", MemoryScope::Global).unwrap();
    s.add_memory("事实二", "101", MemoryScope::Global).unwrap();
    // 空 query → 回退最近 facts（不报错、有返回）。
    let hits = s.recall("", 12, MemoryScope::Global).unwrap();
    assert_eq!(hits.len(), 2);
}

#[test]
fn pinned_tier1_always_recalled() {
    let s = open();
    s.add_memory("普通无关事实", "100", MemoryScope::Global)
        .unwrap();
    let m = s
        .add_memory("锁定要点", "101", MemoryScope::Global)
        .unwrap();
    s.set_pinned(&m.id, true).unwrap();
    // query 与 pinned 内容完全无关，pinned 仍应被召回（Tier1 始终注入）。
    let hits = s
        .recall("xyz 无关查询内容", 12, MemoryScope::Global)
        .unwrap();
    assert!(
        hits.iter().any(|m| m.content.contains("锁定要点")),
        "Tier1（pinned）应始终被召回"
    );
}
