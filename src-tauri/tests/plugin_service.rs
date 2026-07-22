use silicon_worker::plugin::PluginService;
use silicon_worker::skill::{store as skill_store, SkillService};
use silicon_worker::storage::AppDatabase;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 隔离测试环境：(db, plugins 用户根, builtin-plugins 内置根, skills 根)。
fn env() -> (Arc<AppDatabase>, PathBuf, PathBuf, PathBuf) {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("siw-plugin_{}_{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&base);
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    // 卸载/覆盖安装会连带清理插件带来的专家（T108），故测试库也得有 experts 表。
    // 生产里 ExpertService 在启动时建表，这里没有它。
    silicon_worker::expert::store::ensure_schema(&db).expect("experts schema");
    (
        db,
        base.join("plugins"),
        base.join("builtin-plugins"),
        base.join("skills"),
    )
}

/// 在 dir 下造一个插件源目录（.qoder-plugin/plugin.json + 两个 skill：一个可见、一个隐藏）。
fn write_plugin_src(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = format!(
        r#"{{"name":"{name}","displayName":"法务助手","version":"1.0.0",
            "description":"Legal","descriptionZh":"法务工具箱","category":"legal",
            "skills":["skills/draft-contract","skills/legal-kb"]}}"#
    );
    // 本项目约定：plugin.json 放在插件根目录。
    std::fs::write(dir.join("plugin.json"), manifest).unwrap();
    // 可见 skill。
    let s1 = dir.join("skills").join("draft-contract");
    std::fs::create_dir_all(&s1).unwrap();
    std::fs::write(
        s1.join("SKILL.md"),
        "---\nname: draft-contract\ndescription: 起草合同\n---\n起草合同正文",
    )
    .unwrap();
    // 隐藏的内部知识库 skill。
    let s2 = dir.join("skills").join("legal-kb");
    std::fs::create_dir_all(&s2).unwrap();
    std::fs::write(
        s2.join("SKILL.md"),
        "---\nname: legal-kb\ndescription: 法律知识库\nuser-invocable: false\n---\n知识库正文",
    )
    .unwrap();
    dir
}

#[test]
fn install_indexes_plugin_and_its_skills() {
    let (db, root, builtin, skills_root) = env();
    let src_parent = root.parent().unwrap().join("src");
    let src = write_plugin_src(&src_parent, "legal-assistant");

    let psvc = PluginService::new(db.clone(), root.clone(), builtin);
    let summary = psvc
        .install_from_path(src.to_str().unwrap())
        .expect("install");
    assert_eq!(summary.name, "legal-assistant");
    assert_eq!(summary.display_name, "法务助手");
    assert_eq!(summary.skill_count, 2, "应索引 2 个 skill");

    // skill 行带 plugin_id。
    let list = psvc.list().expect("list");
    let pid = &list[0].id;
    let pskills = skill_store::list_by_plugin(&db, pid).expect("by plugin");
    assert_eq!(pskills.len(), 2);
    assert!(pskills
        .iter()
        .all(|s| s.plugin_id.as_deref() == Some(pid.as_str())));

    // 插件 skill 可被 SkillService 按 name 加载（dir_name 为绝对路径）。
    let ssvc = SkillService::new(db.clone(), skills_root);
    let body = ssvc
        .load_body("draft-contract")
        .expect("load")
        .expect("some");
    assert!(body.contains("起草合同正文"));
    // 隐藏 skill 也能被引用加载，但不进 list_enabled。
    let kb = ssvc.load_body("legal-kb").expect("load").expect("some");
    assert!(kb.contains("知识库正文"));
    let enabled = ssvc.list_enabled().expect("enabled");
    assert!(enabled.iter().any(|s| s.name == "draft-contract"));
    assert!(
        !enabled.iter().any(|s| s.name == "legal-kb"),
        "隐藏 skill 不进 list_enabled"
    );
}

#[test]
fn toggle_and_disabled_ids() {
    let (db, root, builtin, _skills) = env();
    let src = write_plugin_src(&root.parent().unwrap().join("src2"), "p-toggle");
    let psvc = PluginService::new(db.clone(), root, builtin);
    let s = psvc
        .install_from_path(src.to_str().unwrap())
        .expect("install");
    psvc.toggle(&s.id, false).expect("toggle");
    let disabled = silicon_worker::plugin::store::disabled_ids(&db).expect("disabled");
    assert!(disabled.contains(&s.id), "禁用插件应在 disabled_ids");
}

#[test]
fn uninstall_cascades_skills_and_dir() {
    let (db, root, builtin, _skills) = env();
    let src = write_plugin_src(&root.parent().unwrap().join("src3"), "p-del");
    let psvc = PluginService::new(db.clone(), root.clone(), builtin);
    let s = psvc
        .install_from_path(src.to_str().unwrap())
        .expect("install");
    let pid = s.id.clone();
    assert!(root.join("p-del").is_dir());
    psvc.uninstall(&pid).expect("uninstall");
    assert!(!root.join("p-del").exists(), "插件目录应删除");
    assert!(
        skill_store::list_by_plugin(&db, &pid).unwrap().is_empty(),
        "skill 行应级联删除"
    );
    assert!(psvc.list().unwrap().is_empty(), "插件行应删除");
}

#[test]
fn install_without_overwrite_errors_on_existing() {
    let (db, root, builtin, _skills) = env();
    let src = write_plugin_src(&root.parent().unwrap().join("src5"), "p-dup");
    let psvc = PluginService::new(db.clone(), root, builtin);
    psvc.install_from_path(src.to_str().unwrap())
        .expect("install");
    let err = psvc
        .install_or_update_from_path(src.to_str().unwrap(), false)
        .unwrap_err();
    assert!(err.contains("已存在"), "同名非覆盖应报错：{err}");
}

#[test]
fn overwrite_update_replaces_plugin_and_skills() {
    let (db, root, builtin, _skills) = env();
    let parent = root.parent().unwrap().join("src6");
    let src = write_plugin_src(&parent, "p-upd");
    let psvc = PluginService::new(db.clone(), root, builtin);
    psvc.install_from_path(src.to_str().unwrap())
        .expect("install");

    // 改源 manifest：去掉一个 skill（只留 draft-contract），覆盖更新。
    let manifest = r#"{"name":"p-upd","displayName":"改名后","version":"2.0.0",
        "description":"v2","skills":["skills/draft-contract"]}"#;
    std::fs::write(src.join("plugin.json"), manifest).unwrap();
    let updated = psvc
        .install_or_update_from_path(src.to_str().unwrap(), true)
        .expect("update");
    assert_eq!(updated.display_name, "改名后");
    assert_eq!(updated.skill_count, 1, "skill 应随 manifest 收缩为 1");
    assert_eq!(psvc.list().unwrap().len(), 1, "更新不应新增插件行");
}

#[test]
fn install_compat_claude_plugin_dir() {
    // 导入兼容：plugin.json 放在 .claude-plugin/ 下（Claude 目录结构），无根 plugin.json。
    let (db, root, builtin, _skills) = env();
    let parent = root.parent().unwrap().join("src-claude");
    let dir = parent.join("claude-plug");
    std::fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
    std::fs::write(
        dir.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"claude-plug","displayName":"Claude 插件","skills":["skills/one"]}"#,
    )
    .unwrap();
    let s = dir.join("skills").join("one");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::write(
        s.join("SKILL.md"),
        "---\nname: one\ndescription: d\n---\n正文",
    )
    .unwrap();

    let psvc = PluginService::new(db, root, builtin);
    let summary = psvc
        .install_from_path(dir.to_str().unwrap())
        .expect("install claude");
    assert_eq!(summary.name, "claude-plug");
    assert_eq!(summary.skill_count, 1);
}

#[test]
fn sync_cleans_orphan_plugin() {
    let (db, root, builtin, _skills) = env();
    let src = write_plugin_src(&root.parent().unwrap().join("src4"), "p-orphan");
    let psvc = PluginService::new(db.clone(), root.clone(), builtin);
    psvc.install_from_path(src.to_str().unwrap())
        .expect("install");
    // 手动删磁盘目录后 sync → 索引清理。
    std::fs::remove_dir_all(root.join("p-orphan")).unwrap();
    psvc.sync().expect("sync");
    assert!(psvc.list().unwrap().is_empty(), "孤儿插件应被清理");
}

#[test]
fn install_from_zip_with_top_folder() {
    use std::io::Write;
    let (db, root, builtin, _skills) = env();
    // 造一个含顶层包裹目录的 zip：legal/plugin.json + legal/skills/draft/SKILL.md。
    let tmp = root.parent().unwrap().join("zipsrc");
    std::fs::create_dir_all(&tmp).unwrap();
    let zip_path = tmp.join("legal.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = Default::default();
    zip.start_file("legal/plugin.json", opts).unwrap();
    zip.write_all(r#"{"name":"legal","displayName":"法务","skills":["skills/draft"]}"#.as_bytes())
        .unwrap();
    zip.start_file("legal/skills/draft/SKILL.md", opts).unwrap();
    zip.write_all("---\nname: draft\ndescription: 起草\n---\n正文".as_bytes())
        .unwrap();
    zip.finish().unwrap();

    let psvc = PluginService::new(db, root.clone(), builtin);
    let summary = psvc
        .install_from_path(zip_path.to_str().unwrap())
        .expect("install zip");
    assert_eq!(summary.name, "legal");
    assert_eq!(summary.skill_count, 1);
    assert!(
        root.join("legal").join("plugin.json").is_file(),
        "应复制到 plugins/legal"
    );
}
