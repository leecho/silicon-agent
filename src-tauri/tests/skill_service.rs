use silicon_worker::skill::builtin;
use silicon_worker::skill::SkillService;
use silicon_worker::storage::AppDatabase;
use std::sync::Arc;

/// 建一个隔离的 (db, skills_root) 测试环境。
fn env() -> (Arc<AppDatabase>, std::path::PathBuf) {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("siw-skillsvc_{}_{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&base);
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    (db, base.join("skills"))
}

/// 在技能根目录手写一个技能目录（仅 SKILL.md）。
fn write_skill(root: &std::path::Path, name: &str, desc: &str, body: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let md = format!("---\nname: {name}\ndescription: {desc}\n---\n{body}");
    std::fs::write(dir.join("SKILL.md"), md).expect("write");
}

#[test]
fn builtin_names_not_empty() {
    let names = builtin::builtin_names();
    assert!(!names.is_empty(), "应内嵌至少一个内置技能");
}

#[test]
fn materialize_writes_first_builtin_skill_md() {
    let names = builtin::builtin_names();
    let first = names.first().expect("至少一个内置技能").clone();
    let root = std::env::temp_dir().join(format!("siw-builtin-mat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    builtin::materialize(&root).expect("materialize");
    let md = root.join(&first).join("SKILL.md");
    assert!(
        md.is_file(),
        "内置技能 {first} 的 SKILL.md 应被物化到根目录"
    );
    let content = std::fs::read_to_string(&md).expect("read");
    assert!(content.contains("name:"), "SKILL.md 应含 frontmatter name");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sync_indexes_builtin_and_user_skills() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "user-one", "用户技能", "用户正文");
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let all = svc.list().expect("list");
    // 任取一个内置技能（来自内嵌目录的真实集合）验证 source/enabled。
    let builtin_name = builtin::builtin_names()
        .into_iter()
        .next()
        .expect("至少一个内置技能");
    let builtin = all
        .iter()
        .find(|s| s.name == builtin_name)
        .expect("builtin");
    assert_eq!(builtin.source, silicon_worker::skill::SkillSource::Builtin);
    assert!(builtin.enabled, "内置默认启用");
    let user = all.iter().find(|s| s.name == "user-one").expect("user");
    assert_eq!(user.source, silicon_worker::skill::SkillSource::User);
}

#[test]
fn sync_removes_orphan_rows() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "temp-skill", "临时", "正文");
    let svc = SkillService::new(db, root.clone());
    svc.sync().expect("sync1");
    assert!(svc.list().unwrap().iter().any(|s| s.name == "temp-skill"));
    std::fs::remove_dir_all(root.join("temp-skill")).unwrap();
    svc.sync().expect("sync2");
    assert!(!svc.list().unwrap().iter().any(|s| s.name == "temp-skill"));
}

#[test]
fn toggle_and_list_enabled() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "tog", "可切换", "正文");
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let id = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "tog")
        .unwrap()
        .id;
    let updated = svc.toggle(&id, false).expect("toggle");
    assert!(!updated.enabled);
    assert!(!svc.list_enabled().unwrap().iter().any(|s| s.name == "tog"));
}

#[test]
fn load_body_strips_frontmatter() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "loadme", "desc", "这是正文。");
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let body = svc.load_body("loadme").expect("load").expect("some");
    assert_eq!(body.trim(), "这是正文。");
    assert!(svc.load_body("missing").expect("ok").is_none());
}

use std::io::Write;

/// 用 zip crate 写一个含 `<top>/SKILL.md` 的压缩包，返回 zip 路径。
fn make_zip(dir: &std::path::Path, top: &str, name: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let zip_path = dir.join(format!("{name}.zip"));
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = Default::default();
    zip.start_file(format!("{top}/SKILL.md"), opts).unwrap();
    let md = format!("---\nname: {name}\ndescription: 来自zip\n---\nzip正文");
    zip.write_all(md.as_bytes()).unwrap();
    zip.finish().unwrap();
    zip_path
}

#[test]
fn install_from_directory() {
    let (db, root) = env();
    let svc = SkillService::new(db, root.clone());
    svc.sync().expect("sync");
    let src = std::env::temp_dir().join(format!("siw-src-{}-{}", std::process::id(), "dir"));
    let _ = std::fs::remove_dir_all(&src);
    write_skill(&src, "from-dir", "目录安装", "目录正文");
    let summary = svc
        .install_from_path(src.join("from-dir").to_str().unwrap())
        .expect("install");
    assert_eq!(summary.name, "from-dir");
    assert!(root.join("from-dir").join("SKILL.md").is_file());
}

#[test]
fn install_from_zip_with_top_folder() {
    let (db, root) = env();
    let svc = SkillService::new(db, root.clone());
    svc.sync().expect("sync");
    let tmp = std::env::temp_dir().join(format!("siw-zip-{}-{}", std::process::id(), "z"));
    let zip_path = make_zip(&tmp, "zipped", "zipskill");
    let summary = svc
        .install_from_path(zip_path.to_str().unwrap())
        .expect("install");
    assert_eq!(summary.name, "zipskill");
    assert!(root.join("zipskill").join("SKILL.md").is_file());
}

#[test]
fn install_duplicate_name_errors() {
    let (db, root) = env();
    let svc = SkillService::new(db, root.clone());
    svc.sync().expect("sync");
    let src = std::env::temp_dir().join(format!("siw-dup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    write_skill(&src, "dupe", "x", "y");
    svc.install_from_path(src.join("dupe").to_str().unwrap())
        .expect("first");
    let err = svc
        .install_from_path(src.join("dupe").to_str().unwrap())
        .unwrap_err();
    assert!(err.contains("技能名已存在"), "实际错误：{err}");
}

#[test]
fn install_missing_skill_md_errors() {
    let (db, root) = env();
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let src = std::env::temp_dir().join(format!("siw-nomd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("readme.txt"), "no skill md").unwrap();
    let err = svc.install_from_path(src.to_str().unwrap()).unwrap_err();
    assert!(err.contains("SKILL.md"), "实际错误：{err}");
}

#[test]
fn uninstall_user_removes_dir_and_row() {
    let (db, root) = env();
    let svc = SkillService::new(db, root.clone());
    svc.sync().expect("sync");
    let src = std::env::temp_dir().join(format!("siw-uninst-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    write_skill(&src, "removeme", "x", "y");
    let s = svc
        .install_from_path(src.join("removeme").to_str().unwrap())
        .expect("install");
    svc.uninstall(&s.id).expect("uninstall");
    assert!(!root.join("removeme").exists());
    assert!(!svc.list().unwrap().iter().any(|x| x.name == "removeme"));
}

#[test]
fn uninstall_builtin_errors() {
    let (db, root) = env();
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let builtin_name = builtin::builtin_names()
        .into_iter()
        .next()
        .expect("至少一个内置技能");
    let id = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.name == builtin_name)
        .unwrap()
        .id;
    assert!(svc.uninstall(&id).is_err(), "内置技能不可卸载");
}

#[test]
fn detail_lists_files_and_skill_md() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "detailed", "详情", "详情正文");
    std::fs::write(root.join("detailed").join("notes.txt"), "附属文本").unwrap();
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let id = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "detailed")
        .unwrap()
        .id;
    let detail = svc.detail(&id).expect("detail");
    assert!(detail.skill_md.contains("详情正文"));
    let rels: Vec<&str> = detail.files.iter().map(|f| f.rel_path.as_str()).collect();
    assert!(rels.contains(&"SKILL.md"));
    assert!(rels.contains(&"notes.txt"));
}

#[test]
fn read_file_markdown_and_text() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "rf", "x", "y");
    std::fs::write(root.join("rf").join("data.json"), "{\"a\":1}").unwrap();
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let id = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "rf")
        .unwrap()
        .id;
    let md = svc.read_file(&id, "SKILL.md").expect("md");
    assert_eq!(md.kind, "markdown");
    assert!(md.text.unwrap().contains("name: rf"));
    let json = svc.read_file(&id, "data.json").expect("json");
    assert_eq!(json.kind, "text");
    assert!(json.text.unwrap().contains("\"a\":1"));
}

#[test]
fn read_file_rejects_path_traversal() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(&root, "guard", "x", "y");
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let id = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.name == "guard")
        .unwrap()
        .id;
    assert!(
        svc.read_file(&id, "../guard/SKILL.md").is_err(),
        "应拒绝 .. 逃逸"
    );
    assert!(svc.read_file(&id, "/etc/hosts").is_err(), "应拒绝绝对路径");
}

/// 在临时目录写一个"源"技能目录（含 SKILL.md），返回该目录路径，用于 install_or_update。
fn write_source_skill(name: &str, desc: &str, body: &str) -> std::path::PathBuf {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("siw-src_{}_{}/{name}", std::process::id(), seq));
    std::fs::create_dir_all(&dir).expect("mkdir src");
    let md = format!("---\nname: {name}\ndescription: {desc}\n---\n{body}");
    std::fs::write(dir.join("SKILL.md"), md).expect("write src");
    dir
}

#[test]
fn install_then_overwrite_update_preserves_enabled_and_id() {
    let (db, root) = env();
    let svc = SkillService::new(db, root);
    let src = write_source_skill("upd-skill", "原描述", "原正文");
    let first = svc
        .install_or_update_from_path(src.to_str().unwrap(), false)
        .expect("install");
    assert_eq!(first.description, "原描述");
    // 关闭启用，验证更新保留该状态。
    svc.toggle(&first.id, false).expect("toggle off");

    // 改源内容后覆盖更新。
    let src2 = write_source_skill("upd-skill", "新描述", "新正文");
    let updated = svc
        .install_or_update_from_path(src2.to_str().unwrap(), true)
        .expect("update");
    assert_eq!(updated.id, first.id, "应保留原 id");
    assert_eq!(updated.description, "新描述", "描述应更新");
    assert!(!updated.enabled, "应保留禁用状态");
    assert_eq!(svc.list().unwrap().len(), 1, "更新不应新增行");
}

#[test]
fn install_without_overwrite_errors_on_existing_name() {
    let (db, root) = env();
    let svc = SkillService::new(db, root);
    let src = write_source_skill("dup-skill", "x", "y");
    svc.install_or_update_from_path(src.to_str().unwrap(), false)
        .expect("install");
    let again = write_source_skill("dup-skill", "x2", "y2");
    let err = svc
        .install_or_update_from_path(again.to_str().unwrap(), false)
        .unwrap_err();
    assert!(err.contains("已存在"), "同名非覆盖应报错：{err}");
}

#[test]
fn cannot_overwrite_builtin_skill() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync"); // 物化 + 索引内置（source=Builtin）。
    let builtin_name = builtin::builtin_names()
        .into_iter()
        .next()
        .expect("至少一个内置技能");
    let src = write_source_skill(&builtin_name, "冒充内置", "正文");
    let err = svc
        .install_or_update_from_path(src.to_str().unwrap(), true)
        .unwrap_err();
    assert!(err.contains("内置技能不可覆盖"), "应拒绝覆盖内置：{err}");
}

#[test]
fn hidden_skill_excluded_from_list_enabled_but_loadable() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    // 手写一个 user-invocable: false 的内部知识库技能。
    let dir = root.join("kb-internal");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: kb-internal\ndescription: 内部知识库\nuser-invocable: false\n---\n知识库正文",
    )
    .unwrap();
    write_skill(&root, "visible-one", "可见技能", "正文");
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");

    let enabled = svc.list_enabled().expect("list_enabled");
    assert!(
        enabled.iter().any(|s| s.name == "visible-one"),
        "可见技能应在 list_enabled"
    );
    assert!(
        !enabled.iter().any(|s| s.name == "kb-internal"),
        "隐藏技能不应进 list_enabled"
    );
    // 但隐藏技能仍可被 load_body 加载（供其他 skill 引用）。
    let body = svc.load_body("kb-internal").expect("load").expect("some");
    assert!(body.contains("知识库正文"));
}

#[test]
fn load_body_substitutes_data_dir_name() {
    let (db, root) = env();
    std::fs::create_dir_all(&root).unwrap();
    write_skill(
        &root,
        "with-var",
        "占位符",
        "技能保存到 ~/{{.DataDirName}}/skills/ 下。",
    );
    let svc = SkillService::new(db, root);
    svc.sync().expect("sync");
    let body = svc.load_body("with-var").expect("load").expect("some");
    assert!(
        body.contains("~/.siliconworker/skills/"),
        "应替换 DataDirName：{body}"
    );
    assert!(!body.contains("{{.DataDirName}}"), "占位符不应残留：{body}");
}
