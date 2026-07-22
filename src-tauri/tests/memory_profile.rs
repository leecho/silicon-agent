// 用户画像（kind='profile' 单例）+ Tier1 渲染（W1 切片3）。
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-prof_{}_{}_{}",
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
fn profile_is_singleton_upsert() {
    let s = open();
    assert!(s.get_profile().unwrap().is_none());

    s.set_profile("用户是 Rust 工程师，偏好简洁", "100")
        .unwrap();
    assert_eq!(
        s.get_profile().unwrap().as_deref(),
        Some("用户是 Rust 工程师，偏好简洁")
    );

    // 再写覆盖（单例，不新增行）。
    s.set_profile("用户改用 TypeScript", "101").unwrap();
    assert_eq!(
        s.get_profile().unwrap().as_deref(),
        Some("用户改用 TypeScript")
    );

    // 画像不进 fact 召回（recall 只返回 fact）。
    let facts = s.recall("Rust", 12, MemoryScope::Global).unwrap();
    assert!(
        facts.iter().all(|m| m.content != "用户改用 TypeScript"),
        "画像不应出现在 fact 召回里"
    );
}

#[test]
fn empty_profile_clears() {
    let s = open();
    s.set_profile("临时画像", "100").unwrap();
    assert!(s.get_profile().unwrap().is_some());
    s.set_profile("   ", "101").unwrap();
    assert!(s.get_profile().unwrap().is_none(), "空白画像视为清空");
}
