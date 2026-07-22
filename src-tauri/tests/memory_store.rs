// MemoryStore CRUD + 写时去重（W0 切片1）。
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-mem_{}_{}_{}",
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
fn add_list_update_delete_clear() {
    let s = open();
    let m1 = s
        .add_memory("用户喜欢简洁回答", "100", MemoryScope::Global)
        .expect("add1");
    let m2 = s
        .add_memory("项目用 Rust", "101", MemoryScope::Global)
        .expect("add2");

    let all = s.list_memories().expect("list");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].content, "用户喜欢简洁回答");
    assert_eq!(all[1].content, "项目用 Rust");

    s.update_memory(&m1.id, "用户喜欢非常简洁的回答")
        .expect("update");
    let after = s.list_memories().expect("list2");
    assert_eq!(after[0].id, m1.id);
    assert_eq!(after[0].content, "用户喜欢非常简洁的回答");
    assert_eq!(after[0].created_at, "100");

    s.delete_memory(&m1.id).expect("del");
    let after2 = s.list_memories().expect("list3");
    assert_eq!(after2.len(), 1);
    assert_eq!(after2[0].id, m2.id);

    s.clear_memories().expect("clear");
    assert!(s.list_memories().expect("list4").is_empty());
}

#[test]
fn add_is_deduped_by_content() {
    let s = open();
    s.add_memory("同一条事实", "100", MemoryScope::Global)
        .expect("a");
    s.add_memory("同一条事实", "101", MemoryScope::Global)
        .expect("b");
    // 相同内容写时去重：只保留一条。
    assert_eq!(s.list_memories().expect("list").len(), 1);
}

#[test]
fn dedup_ignores_whitespace_differences() {
    let s = open();
    s.add_memory("用户  喜欢 简洁", "100", MemoryScope::Global)
        .expect("a");
    s.add_memory("用户 喜欢  简洁", "101", MemoryScope::Global)
        .expect("b");
    // 规范化空白后同内容 → 去重。
    assert_eq!(s.list_memories().expect("list").len(), 1);
}
