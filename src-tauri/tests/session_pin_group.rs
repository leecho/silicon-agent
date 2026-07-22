use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

fn temp_db() -> Arc<AppDatabase> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "siw-pingrp_{}_{}_{}",
        std::process::id(),
        seq,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"))
}

#[test]
fn new_session_defaults_unpinned_ungrouped() {
    let store = SessionStore::open(temp_db()).expect("store");
    let s = store
        .create_session("s1", "标题", "100", false)
        .expect("create");
    assert!(!s.pinned);
    assert_eq!(s.group_id, None);
    let got = store.get_session("s1").expect("get").expect("present");
    assert!(!got.pinned);
    assert_eq!(got.group_id, None);
}

#[test]
fn set_session_pinned_persists() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "标题", "100", false)
        .expect("create");
    store
        .set_session_pinned("s1", true, "101")
        .expect("set pinned");
    let got = store.get_session("s1").expect("get").expect("present");
    assert!(got.pinned, "置顶后 get_session.pinned 应为 true");

    store
        .set_session_pinned("s1", false, "102")
        .expect("unset pinned");
    let got = store.get_session("s1").expect("get").expect("present");
    assert!(!got.pinned, "取消置顶后应为 false");
}

/// ensure_schema 后 list_session_groups 应含 6 个内建分组，且各字段符合预期。
#[test]
fn builtin_groups_seeded_on_open() {
    let store = SessionStore::open(temp_db()).expect("store");
    let groups = store.list_session_groups().expect("list");
    // 6 彩色组（定时任务会话改用 origin 标记，不再有内置「定时任务」分组）
    assert_eq!(groups.len(), 6, "应有 6 个内建分组");

    // 按 sort_order 排列：red(10) orange(20) yellow(30) green(40) blue(50) purple(60)
    let expected = [
        ("red", "红色", "red", 10i64),
        ("orange", "橙色", "orange", 20),
        ("yellow", "黄色", "yellow", 30),
        ("green", "绿色", "green", 40),
        ("blue", "蓝色", "blue", 50),
        ("purple", "紫色", "purple", 60),
    ];
    for (i, (id, label, color_key, sort_order)) in expected.iter().enumerate() {
        assert_eq!(groups[i].id, *id, "id[{i}] 应为 {id}");
        assert_eq!(groups[i].label, *label, "label[{i}] 应为 {label}");
        assert_eq!(
            groups[i].color_key, *color_key,
            "color_key[{i}] 应为 {color_key}"
        );
        assert!(groups[i].built_in, "groups[{i}] 应为内建");
        assert_eq!(
            groups[i].sort_order, *sort_order,
            "sort_order[{i}] 应为 {sort_order}"
        );
    }
}

/// 内建分组 seed 幂等：多次 open 仍只有 6 个分组（insert or ignore 保证幂等）。
#[test]
fn builtin_groups_seed_idempotent() {
    let db = temp_db();
    // 第一次打开
    let _ = SessionStore::open(db.clone()).expect("store1");
    // 第二次 open 同一个 DB
    let store2 = SessionStore::open(db).expect("store2");
    let groups = store2.list_session_groups().expect("list");
    assert_eq!(groups.len(), 6, "二次 open 后仍应只有 6 个分组（幂等）");
}

/// create_session_group 用户新建组：color_key==gray，built_in==false，排在内建之后。
#[test]
fn create_group_defaults_gray_not_builtin() {
    let store = SessionStore::open(temp_db()).expect("store");
    let g = store
        .create_session_group("我的组", "gray", "200")
        .expect("create group");
    assert_eq!(g.color_key, "gray", "用户新建组颜色应为 gray");
    assert!(!g.built_in, "用户新建组 built_in 应为 false");
    assert_eq!(g.sort_order, 1000, "用户新建组 sort_order 应为 1000");

    let groups = store.list_session_groups().expect("list");
    // 6 内建（彩色）+ 1 用户 = 7
    assert_eq!(groups.len(), 7);
    // 最后一个是用户组
    let last = &groups[6];
    assert_eq!(last.id, g.id);
    assert!(!last.built_in);
    assert_eq!(last.color_key, "gray");
}

/// create_group_lists_with_nonempty_color（调整 len 为 8：7 内建 + 1 用户）
#[test]
fn create_group_lists_with_nonempty_color() {
    let store = SessionStore::open(temp_db()).expect("store");
    let g = store
        .create_session_group("工作", "blue", "100")
        .expect("create group");
    assert_eq!(g.label, "工作");
    assert_eq!(g.color_key, "blue");

    let groups = store.list_session_groups().expect("list");
    // 6 内建（彩色）+ 1 用户 = 7
    assert_eq!(groups.len(), 7);
    // 用户组排在末尾
    let user_group = groups
        .iter()
        .find(|x| x.id == g.id)
        .expect("user group present");
    assert!(!user_group.color_key.is_empty(), "color_key 应非空");
}

#[test]
fn set_session_group_some_then_none() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "标题", "100", false)
        .expect("create");
    let g = store
        .create_session_group("工作", "green", "100")
        .expect("create group");

    store
        .set_session_group("s1", Some(&g.id), "101")
        .expect("set group");
    let got = store.get_session("s1").expect("get").expect("present");
    assert_eq!(got.group_id, Some(g.id.clone()), "应归入该分组");

    store
        .set_session_group("s1", None, "102")
        .expect("clear group");
    let got = store.get_session("s1").expect("get").expect("present");
    assert_eq!(got.group_id, None, "移出分组后应为 None");
}

/// 删除内建分组应报错。
#[test]
fn delete_builtin_group_is_rejected() {
    let store = SessionStore::open(temp_db()).expect("store");
    let err = store
        .delete_session_group("red")
        .expect_err("应拒绝删除内建分组");
    assert!(
        err.contains("内建分组不可删除"),
        "错误信息应包含提示: {err}"
    );
}

/// 删除用户分组成功，智能体会话 group_id 归 None。
#[test]
fn delete_group_clears_members() {
    let store = SessionStore::open(temp_db()).expect("store");
    store
        .create_session("s1", "标题", "100", false)
        .expect("create");
    let g = store
        .create_session_group("工作", "amber", "100")
        .expect("create group");
    store
        .set_session_group("s1", Some(&g.id), "101")
        .expect("set group");

    store
        .delete_session_group(&g.id)
        .expect("delete user group");

    let groups = store.list_session_groups().expect("list");
    // 只剩 6 个内建（彩色）
    assert_eq!(groups.len(), 6, "删除用户组后只剩 6 个内建分组");

    let got = store.get_session("s1").expect("get").expect("present");
    assert_eq!(got.group_id, None, "智能体会话 group_id 应被置空");
}
