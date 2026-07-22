use silicon_worker::storage::AppDatabase;

#[test]
fn open_creates_storage_migration_ledger() {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-storage_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let db = AppDatabase::open(dir.join("app.sqlite3")).expect("open db");
    assert!(db.table_exists("schema_migrations").expect("table check"));
    let ledger = db.migration_ledger().expect("ledger");
    assert!(ledger.iter().any(|record| record.module == "storage"));
    let _ = std::fs::remove_dir_all(dir);
}
