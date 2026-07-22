//! T69：per-伴随体私有记忆——memories.agent_id 作用域召回 + 加列迁移。
use silicon_worker::memory::MemoryScope;
use silicon_worker::memory::MemoryStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn base() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "siw-agentscope_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn open() -> MemoryStore {
    let db = Arc::new(AppDatabase::open(base().join("app.sqlite3")).expect("db"));
    MemoryStore::open(db).expect("store")
}

#[test]
fn recall_scope_global_union_private_and_isolated() {
    let s = open();
    s.add_memory("全局事实 Kubernetes", "100", MemoryScope::Global)
        .unwrap(); // 全局
    s.add_memory("A 私有 Kubernetes", "101", MemoryScope::Agent("agentA"))
        .unwrap(); // A 私有
    s.add_memory("B 私有 Kubernetes", "102", MemoryScope::Agent("agentB"))
        .unwrap(); // B 私有

    // A 视角：含全局 + A 私有，不含 B。
    let a = s
        .recall("Kubernetes", 12, MemoryScope::Agent("agentA"))
        .unwrap();
    let a_txt: Vec<&str> = a.iter().map(|m| m.content.as_str()).collect();
    assert!(
        a_txt.contains(&"全局事实 Kubernetes"),
        "A 应见全局: {a_txt:?}"
    );
    assert!(
        a_txt.contains(&"A 私有 Kubernetes"),
        "A 应见自己私有: {a_txt:?}"
    );
    assert!(
        !a_txt.contains(&"B 私有 Kubernetes"),
        "A 不应见 B 私有: {a_txt:?}"
    );

    // None 视角：仅全局。
    let g = s.recall("Kubernetes", 12, MemoryScope::Global).unwrap();
    let g_txt: Vec<&str> = g.iter().map(|m| m.content.as_str()).collect();
    assert!(g_txt.contains(&"全局事实 Kubernetes"));
    assert!(
        !g_txt.contains(&"A 私有 Kubernetes"),
        "全局视角不应见私有: {g_txt:?}"
    );
    assert!(!g_txt.contains(&"B 私有 Kubernetes"));
}

#[test]
fn migrates_legacy_memories_without_agent_id() {
    let dir = base();
    let path = dir.join("legacy.sqlite3");
    let db = Arc::new(AppDatabase::open(&path).expect("db"));
    // 造旧库：memories 无 agent_id 列 + 一行。
    db.with_connection(|c| {
        c.execute_batch(
            "create table memories (id text primary key, content text not null, created_at text not null,
                kind text not null default 'fact', tier integer not null default 2, pinned integer not null default 0,
                source text, tags text, dedup_hash text, session_id text, updated_at text);
             insert into memories (id, content, created_at, kind) values ('m1','存量事实','100','fact');",
        )?;
        Ok(())
    })
    .unwrap();
    // open → ensure_schema 加 agent_id 列、存量行归全局、不丢数据。
    let s = MemoryStore::open(db.clone()).expect("open migrates");
    db.with_connection(|c| {
        let n: i64 = c.query_row("select count(*) from memories", [], |r| r.get(0))?;
        assert_eq!(n, 1, "存量行不丢");
        let aid: String = c.query_row("select agent_id from memories where id='m1'", [], |r| {
            r.get(0)
        })?;
        assert_eq!(aid, "", "存量行 agent_id 归全局");
        Ok(())
    })
    .unwrap();
    // 迁移后写入私有 + 召回正常。
    s.add_memory("迁移后私有", "200", MemoryScope::Agent("agentZ"))
        .unwrap();
    let z = s
        .recall("迁移后私有", 12, MemoryScope::Agent("agentZ"))
        .unwrap();
    assert!(z.iter().any(|m| m.content == "迁移后私有"));
}
