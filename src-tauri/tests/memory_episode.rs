// 情景记忆 add_episode + recall_episodes（W1 切片5）。
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-ep_{}_{}_{}",
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
fn add_and_recall_episode() {
    let s = open();
    s.add_episode(
        "sess-1",
        "讨论了把部署从 Docker 迁移到 Kubernetes 的方案",
        "100",
        MemoryScope::Global,
    )
    .unwrap();
    s.add_episode(
        "sess-1",
        "确定用 Rust 重写解析器以提升性能",
        "101",
        MemoryScope::Global,
    )
    .unwrap();

    // 按 query 召回相关情景（trigram ≥3 字符共享子串「Kubernetes」）。
    let hits = s
        .recall_episodes("继续 Kubernetes 的迁移", 3, MemoryScope::Global)
        .unwrap();
    assert!(
        hits.iter().any(|m| m.content.contains("Kubernetes")),
        "应召回 Kubernetes 相关情景"
    );
}

#[test]
fn episode_not_in_fact_recall() {
    let s = open();
    s.add_episode(
        "sess-1",
        "过去讨论 Kubernetes 迁移",
        "100",
        MemoryScope::Global,
    )
    .unwrap();
    s.add_memory("项目用 Rust", "101", MemoryScope::Global)
        .unwrap();
    // recall 只返回 fact，不含 episode。
    let facts = s.recall("Kubernetes", 12, MemoryScope::Global).unwrap();
    assert!(facts.iter().all(|m| !m.content.contains("Kubernetes")));
}

#[test]
fn empty_episode_ignored_and_deduped() {
    let s = open();
    s.add_episode("s", "   ", "100", MemoryScope::Global)
        .unwrap(); // 空摘要忽略
    s.add_episode("s", "同一摘要", "101", MemoryScope::Global)
        .unwrap();
    s.add_episode("s", "同一摘要", "102", MemoryScope::Global)
        .unwrap(); // 去重
    let all = s.recall_episodes("", 10, MemoryScope::Global).unwrap();
    assert_eq!(all.len(), 1);
}
