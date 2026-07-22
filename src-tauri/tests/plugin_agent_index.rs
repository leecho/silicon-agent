// plugin 承载智能体：声明了 agents 的 plugin 被 sync 后，其 agents/ 经 list_agent_plugins +
// ExpertService.index_plugin_expert 索引进 agents 表（带 plugin_id 命名空间）。不再按 type gate。

use std::sync::Arc;

use silicon_worker::expert::ExpertService;
use silicon_worker::plugin::PluginService;
use silicon_worker::storage::AppDatabase;

fn temp(tag: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "siw-plgagent-{tag}-{}-{}",
        std::process::id(),
        nanos
    ))
}

#[test]
fn plugin_agents_indexed_with_plugin_id() {
    let base = temp("idx");
    let plugins_root = base.join("plugins");
    let builtin_root = base.join("builtin-plugins");
    let agents_root = base.join("agents");
    std::fs::create_dir_all(&agents_root).unwrap();

    // 写一个声明了 agents 的 plugin：plugin.json + agents/lead.md + agents/m1.md。
    let pdir = plugins_root.join("trading");
    std::fs::create_dir_all(pdir.join("agents")).unwrap();
    std::fs::write(
        pdir.join("plugin.json"),
        r#"{"name":"trading","displayName":"交易团队",
            "agents":["agents/lead.md","agents/m1.md"]}"#,
    )
    .unwrap();
    std::fs::write(
        pdir.join("agents/lead.md"),
        "---\nname: lead\ndescription: 主理\ndisplay_name: 何执舟\n---\n你是主理人。\n",
    )
    .unwrap();
    std::fs::write(
        pdir.join("agents/m1.md"),
        "---\nname: m1\ndescription: 成员一\n---\n你是成员。\n",
    )
    .unwrap();

    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));
    let plugins = PluginService::new(db.clone(), plugins_root, builtin_root);
    let agents = ExpertService::new(db.clone(), agents_root);
    plugins.sync().expect("plugin sync");
    agents.sync().expect("agent sync");

    // 复刻 app_state 的串接逻辑。
    let entries = plugins.list_agent_plugins().expect("list");
    assert_eq!(entries.len(), 1, "应识别一个 agent/team plugin");
    let (plugin_id, plugin_dir, manifest) = &entries[0];
    assert_eq!(manifest.agents, vec!["agents/lead.md", "agents/m1.md"]);
    let mut names = Vec::new();
    for rel in &manifest.agents {
        names.push(
            agents
                .index_plugin_expert(plugin_id, &plugin_dir.join(rel), "1")
                .expect("index"),
        );
    }
    agents
        .clear_plugin_experts_except(plugin_id, &names)
        .unwrap();

    // 该 plugin 命名空间下两位成员就绪，带展示身份。
    let roster = agents.list_enabled_by_plugin(plugin_id).expect("roster");
    let listed: Vec<_> = roster.iter().map(|s| s.name.clone()).collect();
    assert!(listed.contains(&"lead".to_string()));
    assert!(listed.contains(&"m1".to_string()));
    let lead = roster.iter().find(|s| s.name == "lead").unwrap();
    assert_eq!(lead.display_name.as_deref(), Some("何执舟"));
    assert_eq!(lead.plugin_id, *plugin_id);

    // 命名空间解析：active=plugin → 命中；自由模式（None）查不到 plugin 智能体。
    assert!(agents
        .load_spec("lead", Some(plugin_id))
        .expect("ls")
        .is_some());
    assert!(agents.load_spec("lead", None).expect("ls2").is_none());
}
