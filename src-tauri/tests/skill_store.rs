use silicon_worker::skill::model::{SkillRecord, SkillSource};
use silicon_worker::skill::store;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn temp_db() -> Arc<AppDatabase> {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("siw-skill-store_{}_{}", std::process::id(), seq));
    let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"));
    store::ensure_schema(&db).expect("schema");
    db
}

fn rec(name: &str, source: SkillSource, enabled: bool) -> SkillRecord {
    rec_owner(name, source, enabled, None, None)
}

fn rec_owner(
    name: &str,
    source: SkillSource,
    enabled: bool,
    plugin_id: Option<&str>,
    team_id: Option<&str>,
) -> SkillRecord {
    let owner = plugin_id.or(team_id).unwrap_or("");
    SkillRecord {
        id: if owner.is_empty() {
            format!("id-{name}")
        } else {
            format!("id-{owner}-{name}")
        },
        source,
        name: name.into(),
        description: format!("{name} 描述"),
        dir_name: name.into(),
        enabled,
        installed_at: "100".into(),
        updated_at: "100".into(),
        plugin_id: plugin_id.map(|s| s.to_string()),
        team_id: team_id.map(|s| s.to_string()),
        expert_id: None,
        user_invocable: true,
        argument_hint: None,
        group_id: None,
    }
}

#[test]
fn upsert_then_list_and_get() {
    let db = temp_db();
    store::upsert(&db, &rec("alpha", SkillSource::User, true)).expect("upsert");
    let list = store::list(&db).expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "alpha");
    let got = store::get_by_name(&db, "alpha")
        .expect("get")
        .expect("some");
    assert_eq!(got.id, "id-alpha");
}

#[test]
fn upsert_conflict_preserves_enabled() {
    let db = temp_db();
    store::upsert(&db, &rec("beta", SkillSource::User, true)).expect("first");
    store::set_enabled(&db, "id-beta", false, "200").expect("disable");
    // 再次 upsert（模拟 sync，enabled=true）应保留已有 enabled=false。
    store::upsert(&db, &rec("beta", SkillSource::User, true)).expect("re-upsert");
    let got = store::get_by_name(&db, "beta").expect("get").expect("some");
    assert!(!got.enabled, "sync upsert 必须保留用户的 enabled 状态");
}

#[test]
fn set_enabled_and_delete() {
    let db = temp_db();
    store::upsert(&db, &rec("gamma", SkillSource::User, true)).expect("upsert");
    store::set_enabled(&db, "id-gamma", false, "300").expect("toggle");
    assert!(!store::get_by_id(&db, "id-gamma").unwrap().unwrap().enabled);
    store::delete(&db, "id-gamma").expect("delete");
    assert!(store::get_by_id(&db, "id-gamma").unwrap().is_none());
}

#[test]
fn list_enabled_only() {
    let db = temp_db();
    store::upsert(&db, &rec("on", SkillSource::User, true)).expect("u1");
    store::upsert(&db, &rec("off", SkillSource::User, true)).expect("u2");
    store::set_enabled(&db, "id-off", false, "400").expect("disable");
    let enabled = store::list_enabled(&db).expect("list_enabled");
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].name, "on");
}

#[test]
fn team_private_skill_excluded_from_default_pool() {
    let db = temp_db();
    // 散装、plugin 提供 → 全局；team 私有 → 不进默认池。
    store::upsert(&db, &rec_owner("std", SkillSource::User, true, None, None)).expect("u1");
    store::upsert(
        &db,
        &rec_owner("from-plg", SkillSource::User, true, Some("plg-a"), None),
    )
    .expect("u2");
    store::upsert(
        &db,
        &rec_owner("priv", SkillSource::User, true, None, Some("t1")),
    )
    .expect("u3");

    let pool: Vec<_> = store::list_enabled(&db)
        .expect("list_enabled")
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert!(pool.contains(&"std".to_string()));
    assert!(pool.contains(&"from-plg".to_string()));
    assert!(
        !pool.contains(&"priv".to_string()),
        "team 私有 skill 不进默认池"
    );

    // 私有 skill 仅按 team 取。
    let by_team: Vec<_> = store::list_enabled_by_team(&db, "t1")
        .expect("by team")
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(by_team, vec!["priv".to_string()]);

    // team 级联删：priv 没了，其余不动。
    store::delete_by_team(&db, "t1").expect("del team");
    assert!(store::list_by_team(&db, "t1").expect("lt").is_empty());
    assert_eq!(store::list(&db).expect("list").len(), 2);
}
