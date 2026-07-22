// 项目层记忆作用域（新增项目层）：写入路由、并集召回、删项目级联。
use silicon_worker::memory::{MemoryScope, MemoryStore};
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn open() -> MemoryStore {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "siw-projmem_{}_{}_{}",
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
fn project_recall_sees_global_and_project_not_agent() {
    let s = open();
    s.add_memory("全局事实 Kubernetes", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目P事实 Kubernetes", "101", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("项目Q事实 Kubernetes", "102", MemoryScope::Project("projQ"))
        .unwrap();
    s.add_memory("伴随体A Kubernetes", "103", MemoryScope::Agent("agentA"))
        .unwrap();

    // 项目 P 会话召回：全局 ∪ 项目P，不含项目Q、不含 agent。
    let hits = s
        .recall("Kubernetes", 12, MemoryScope::Project("projP"))
        .unwrap();
    let joined = hits
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(joined.contains("全局事实"), "应含全局：{joined}");
    assert!(joined.contains("项目P事实"), "应含本项目：{joined}");
    assert!(!joined.contains("项目Q事实"), "不应含他项目：{joined}");
    assert!(!joined.contains("伴随体A"), "不应含 agent 私有：{joined}");
}

#[test]
fn global_recall_excludes_project_and_agent() {
    let s = open();
    s.add_memory("全局事实", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目事实", "101", MemoryScope::Project("projP"))
        .unwrap();
    let hits = s.recall("事实", 12, MemoryScope::Global).unwrap();
    let joined = hits
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(joined.contains("全局事实"));
    assert!(
        !joined.contains("项目事实"),
        "全局会话不应见项目层：{joined}"
    );
}

#[test]
fn same_content_coexists_across_scopes() {
    let s = open();
    // 同一句话在不同作用域各存一条（作用域内去重，跨作用域不去重）。
    s.add_memory("同一句", "100", MemoryScope::Global).unwrap();
    s.add_memory("同一句", "101", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("同一句", "102", MemoryScope::Global).unwrap(); // 全局内重复→去重
    let proj = s
        .recall("同一句", 12, MemoryScope::Project("projP"))
        .unwrap();
    // 项目会话见：全局1条 + 项目1条 = 2。
    assert_eq!(proj.len(), 2, "全局+项目各一条");
}

#[test]
fn delete_by_project_cascades() {
    let s = open();
    s.add_memory("全局保留", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目P删除", "101", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("项目Q保留", "102", MemoryScope::Project("projQ"))
        .unwrap();

    s.delete_by_project("projP").unwrap();

    // P 没了，全局与 Q 还在。
    let p = s
        .recall("项目P删除", 12, MemoryScope::Project("projP"))
        .unwrap();
    assert!(
        p.iter().all(|m| m.content != "项目P删除"),
        "项目P记忆应已删除"
    );
    let q = s
        .recall("项目Q保留", 12, MemoryScope::Project("projQ"))
        .unwrap();
    assert!(q.iter().any(|m| m.content == "项目Q保留"), "项目Q应保留");
    let g = s.recall("全局保留", 12, MemoryScope::Global).unwrap();
    assert!(g.iter().any(|m| m.content == "全局保留"), "全局应保留");

    // 空 project_id 不误删全局。
    s.delete_by_project("").unwrap();
    let g2 = s.recall("全局保留", 12, MemoryScope::Global).unwrap();
    assert!(g2.iter().any(|m| m.content == "全局保留"));
}

#[test]
fn list_memories_is_global_only() {
    let s = open();
    s.add_memory("全局事实", "100", MemoryScope::Global)
        .unwrap();
    s.add_memory("项目事实", "101", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("伴随体事实", "102", MemoryScope::Agent("agentA"))
        .unwrap();
    // 设置页 list 只列全局层。
    let list = s.list_memories().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].content, "全局事实");
}

#[test]
fn list_and_count_scoped_facts_are_exact() {
    let s = open();
    s.add_memory("全局A", "100", MemoryScope::Global).unwrap();
    s.add_memory("项目P1", "101", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("项目P2", "102", MemoryScope::Project("projP"))
        .unwrap();
    s.add_memory("项目Q1", "103", MemoryScope::Project("projQ"))
        .unwrap();
    s.add_memory("伴随体X", "104", MemoryScope::Agent("agentX"))
        .unwrap();

    // 精确作用域：项目 P 只见自己两条（不含全局/他项目/agent）。
    let p = s.list_scoped_facts(MemoryScope::Project("projP")).unwrap();
    assert_eq!(p.len(), 2);
    assert!(p.iter().all(|m| m.content.starts_with("项目P")));
    assert_eq!(
        s.count_scoped_facts(MemoryScope::Project("projP")).unwrap(),
        2
    );

    // 全局精确作用域只见全局一条；agent 精确作用域一条。
    assert_eq!(s.count_scoped_facts(MemoryScope::Global).unwrap(), 1);
    assert_eq!(
        s.count_scoped_facts(MemoryScope::Agent("agentX")).unwrap(),
        1
    );
}

#[test]
fn from_session_routes_project_first() {
    // project_id 非空 → Project；否则 agent_id 非空 → Agent；角色定义不参与记忆归属。
    assert!(matches!(
        MemoryScope::from_session("projP", "agentA"),
        MemoryScope::Project("projP")
    ));
    assert!(matches!(
        MemoryScope::from_session("", "agentA"),
        MemoryScope::Agent("agentA")
    ));
    assert!(matches!(
        MemoryScope::from_session("", ""),
        MemoryScope::Global
    ));
}
