// 主动整理 apply 逻辑（W2b）：重建未置顶 fact、保留置顶、更新画像。
use silicon_worker::memory::curation::{apply_curation, parse_curation};
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-cur_{}_{}_{}",
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
fn apply_rebuilds_unpinned_keeps_pinned_and_sets_profile() {
    let s = open();
    s.add_memory("零散事实甲", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("零散事实乙", "101", MemoryScope::Global)
        .unwrap();
    let pinned = s
        .add_memory("锁定要点", "102", MemoryScope::Global)
        .unwrap();
    s.set_pinned(&pinned.id, true).unwrap();

    let result = parse_curation(r#"{"facts":["合并后的事实"],"profile":"用户是工程师"}"#).unwrap();
    let (after, profile_updated) = apply_curation(&s, &result, "200").unwrap();

    // 重建后：合并事实 + 保留的置顶 = 2 条。
    let facts: Vec<String> = s
        .list_memories()
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect();
    assert!(facts.contains(&"合并后的事实".to_string()));
    assert!(facts.contains(&"锁定要点".to_string()), "置顶 fact 应保留");
    assert!(
        !facts.contains(&"零散事实甲".to_string()),
        "未置顶旧事实应被清除"
    );
    assert_eq!(after, 1, "apply 返回新增的合并事实数");
    assert!(profile_updated);
    assert_eq!(s.get_profile().unwrap().as_deref(), Some("用户是工程师"));
}

#[test]
fn apply_empty_profile_does_not_update() {
    let s = open();
    s.set_profile("原画像", "100").unwrap();
    let result = parse_curation(r#"{"facts":["x"],"profile":""}"#).unwrap();
    let (_, profile_updated) = apply_curation(&s, &result, "200").unwrap();
    assert!(!profile_updated);
    assert_eq!(s.get_profile().unwrap().as_deref(), Some("原画像"));
}
