// T59 P1 Task3/4：engine kind="project" 角色解析（coordinator→lead SOP + 成员 roster + PM SOP）
// 与 dispatch 成员校验（is_project_member）。
use silicon_worker::engine::Engine;
use silicon_worker::expert::ExpertService;
use silicon_worker::project::ProjectService;
use silicon_worker::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_worker::session::SessionStore;
use silicon_worker::skill::{SkillRecord, SkillService, SkillSource};
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::load_skill::LoadSkill;
use silicon_worker::tools::ToolRegistry;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct NoopClient;
impl ModelClient for NoopClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        _on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: String::new(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

struct CaptureSystemPromptClient {
    captured: Arc<Mutex<Option<String>>>,
}

impl ModelClient for CaptureSystemPromptClient {
    fn stream_model_with_events(
        &self,
        request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        _on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let system = request
            .messages
            .iter()
            .find(|message| {
                matches!(
                    message.role,
                    silicon_worker::provider::message::ModelMessageRole::System
                )
            })
            .map(|message| message.content.clone());
        *self.captured.lock().unwrap() = system;
        Ok(ModelCallResult {
            events: vec![ModelEvent::AssistantMessageCompleted {
                content: "done".into(),
            }],
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}

fn setup() -> (Arc<AppDatabase>, std::path::PathBuf) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dbp = std::env::temp_dir().join(format!("siw-t59role-{}-{}.db", std::process::id(), n));
    let _ = std::fs::remove_file(&dbp);
    let root = std::env::temp_dir().join(format!("siw-t59role-root-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    (Arc::new(AppDatabase::open(&dbp).unwrap()), root)
}

#[test]
fn project_role_resolution_and_dispatch_validation() {
    let (db, root) = setup();
    // 散装 agent 落盘：pm（协调者，正文含 PMPERSONA）+ writer（普通成员）。
    std::fs::write(
        root.join("pm.md"),
        "---\nname: pm\ndescription: 项目经理\n---\nPMPERSONA 正文",
    )
    .unwrap();
    std::fs::write(
        root.join("writer.md"),
        "---\nname: writer\ndescription: 写手\n---\n写作正文",
    )
    .unwrap();
    let agents = Arc::new(ExpertService::new(db.clone(), root));
    agents.sync().unwrap();

    let projects = Arc::new(ProjectService::new(db.clone(), agents.clone()));
    let p = projects.create("内容工作室", "做内容", "", None).unwrap();
    projects
        .add_member(&p.id, "pm", Some("PM"), None, true)
        .unwrap();
    projects
        .add_member(&p.id, "writer", Some("写手"), None, false)
        .unwrap();

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(NoopClient),
    )
    .with_experts(agents)
    .with_projects(projects);

    // 角色解析：label=项目名；sop 含 coordinator 正文 + PM 编排 SOP；roster 仅非 coordinator。
    let (roster, label, sop) = engine.project_run_roster(&p.id);
    assert_eq!(label.as_deref(), Some("内容工作室"));
    let sop = sop.expect("sop");
    assert!(sop.contains("PMPERSONA"), "coordinator 正文应注入 SOP");
    assert!(sop.contains("项目群聊的主持人"), "PM 编排 SOP 应注入");
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].name, "writer");

    // dispatch 成员校验。
    assert!(engine.is_project_member(&p.id, "writer"));
    assert!(engine.is_project_member(&p.id, "pm"));
    assert!(!engine.is_project_member(&p.id, "stranger"));
}

#[test]
fn project_without_coordinator_uses_synthetic_pm() {
    let (db, root) = setup();
    std::fs::write(
        root.join("writer.md"),
        "---\nname: writer\ndescription: 写手\n---\n写作正文",
    )
    .unwrap();
    let agents = Arc::new(ExpertService::new(db.clone(), root));
    agents.sync().unwrap();
    let projects = Arc::new(ProjectService::new(db.clone(), agents.clone()));
    let p = projects.create("无PM项目", "", "", None).unwrap();
    projects
        .add_member(&p.id, "writer", None, None, false)
        .unwrap();

    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(NoopClient),
    )
    .with_experts(agents)
    .with_projects(projects);

    let (roster, _label, sop) = engine.project_run_roster(&p.id);
    // 无 coordinator → 仅合成 PM SOP（不含任何成员人设前缀）；所有成员进 roster。
    let sop = sop.expect("sop");
    assert!(sop.contains("项目群聊的主持人"));
    assert!(
        sop.trim().ends_with("用中文与用户交流。"),
        "无 coordinator 时 SOP 末尾即 PM SOP 收尾，无追加人设"
    );
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].name, "writer");
}

#[test]
fn expert_role_uses_expert_id_not_name() {
    let (db, root) = setup();
    std::fs::write(
        root.join("writer.md"),
        "---\nname: writer\ndescription: 写手\n---\nEXPERT-ID-PERSONA",
    )
    .unwrap();
    let experts = Arc::new(ExpertService::new(db.clone(), root));
    experts.sync().unwrap();
    let expert = experts
        .list_standalone()
        .unwrap()
        .into_iter()
        .find(|item| item.name == "writer")
        .expect("expert");

    let store = SessionStore::open(db.clone()).unwrap();
    let by_id = store
        .create_session("s-expert-id", "expert id", "1", false)
        .unwrap();
    store
        .set_role(&by_id.id, Some("expert"), Some(&expert.id), "2")
        .unwrap();

    let captured_by_id = Arc::new(Mutex::new(None));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(CaptureSystemPromptClient {
            captured: captured_by_id.clone(),
        }),
    )
    .with_experts(experts.clone());
    engine
        .submit_user_message(
            &by_id.id,
            "hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
    let prompt = captured_by_id
        .lock()
        .unwrap()
        .clone()
        .expect("system prompt");
    assert!(
        prompt.contains("EXPERT-ID-PERSONA"),
        "expert role id should resolve by ExpertSummary.id"
    );

    let by_name = store
        .create_session("s-expert-name", "expert name", "3", false)
        .unwrap();
    store
        .set_role(&by_name.id, Some("expert"), Some("writer"), "4")
        .unwrap();

    let captured_by_name = Arc::new(Mutex::new(None));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(CaptureSystemPromptClient {
            captured: captured_by_name.clone(),
        }),
    )
    .with_experts(experts);
    engine
        .submit_user_message(
            &by_name.id,
            "hello",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
    let prompt = captured_by_name
        .lock()
        .unwrap()
        .clone()
        .expect("system prompt");
    assert!(
        !prompt.contains("EXPERT-ID-PERSONA"),
        "expert role must not fall back from id to name"
    );
}

/// 调一次 load_skill(self.skill) 再收尾的桩 client（用于断言项目运行能解析到私有 skill 正文）。
struct ProjectSkillClient {
    calls: AtomicUsize,
    skill: &'static str,
}
impl ModelClient for ProjectSkillClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        _cancel: &std::sync::atomic::AtomicBool,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "name": self.skill }).to_string();
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "load_skill".into(),
                arguments_json: args.clone(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "load_skill".into(),
                    arguments_json: args,
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "done".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

/// 方案A 核心回归：从团队导入的项目成员（快照 origin_team_id=team-1）在项目运行（project_id 归属，
/// PM/lead 路径）中应仍能解析到其源团队的私有 skill —— 软引用，让被复制进项目的成员不丢团队技能。
#[test]
fn project_inherits_member_origin_team_private_skill() {
    let (db, root) = setup();
    let skills_root = root.join("skills");
    let team_dir = skills_root.join("team-kb-private");
    std::fs::create_dir_all(&team_dir).unwrap();
    std::fs::write(
        team_dir.join("SKILL.md"),
        "---\nname: team-kb\ndescription: 团队知识库\n---\nTEAM BODY",
    )
    .unwrap();
    let skills = Arc::new(SkillService::new(db.clone(), skills_root));
    skills.sync().unwrap();
    // team 私有 skill（owner=team-1）：默认不入池，仅其 team/继承方被选中时注入。
    silicon_worker::skill::store::upsert(
        &db,
        &SkillRecord {
            id: "skill-team-kb".into(),
            source: SkillSource::User,
            name: "team-kb".into(),
            description: "团队知识库".into(),
            dir_name: team_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: Some("team-1".into()),
            expert_id: None,
            user_invocable: true,
            argument_hint: None,
            group_id: None,
        },
    )
    .unwrap();

    let agents = Arc::new(ExpertService::new(db.clone(), root.join("agents")));
    let projects = Arc::new(ProjectService::new(db.clone(), agents));
    let p = projects.create("内容项目", "", "", None).unwrap();
    // 快照成员，记录来源团队 team-1。
    projects
        .add_member_snapshot(
            &p.id,
            "writer",
            Some("写手"),
            Some("写作"),
            None,
            Some("写手"),
            "你是写手。",
            vec!["load_skill".into()],
            "main",
            Some("team-1"),
        )
        .unwrap();
    // origin_team_ids / member_origin_team_id 落库正确。
    assert_eq!(
        projects.origin_team_ids(&p.id).unwrap(),
        vec!["team-1".to_string()]
    );
    assert_eq!(
        projects
            .member_origin_team_id(&p.id, "writer")
            .unwrap()
            .as_deref(),
        Some("team-1")
    );

    let store = SessionStore::open(db.clone()).unwrap();
    let s = store
        .create_session("s-project", "project", "1", false)
        .unwrap();
    store.set_project_id(&s.id, &p.id, "2").unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(LoadSkill));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ProjectSkillClient {
            calls: AtomicUsize::new(0),
            skill: "team-kb",
        }),
    )
    .with_projects(projects)
    .with_skills(skills)
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &s.id,
            "读取团队知识库",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();

    assert!(pending.is_none());
    let tool_msg = detail
        .messages
        .iter()
        .find(|m| m.tool_name.as_deref() == Some("load_skill"))
        .expect("应有 load_skill 结果");
    assert_eq!(
        tool_msg.content.trim(),
        "TEAM BODY",
        "项目成员来源团队的私有 skill 应在项目运行时可解析"
    );
}

/// 手动加进项目的散装智能体若自带 agent 私有 skill（owner=agent name），项目运行（PM/lead 路径）
/// 也应能解析到 —— 与 team 私有 skill 对称。
#[test]
fn project_injects_member_agent_private_skill() {
    let (db, root) = setup();
    // 散装 agent「researcher」落盘（非快照成员，按名解析）。
    let agents_root = root.join("agents");
    std::fs::create_dir_all(&agents_root).unwrap();
    std::fs::write(
        agents_root.join("researcher.md"),
        "---\nname: researcher\ndescription: 研究员\n---\n你是研究员。",
    )
    .unwrap();
    // agent 私有 skill「deep-search」（owner=researcher）：默认不入池，仅该 agent 在场时注入。
    let skills_root = root.join("skills");
    let skill_dir = skills_root.join("deep-search");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deep-search\ndescription: 深度检索\n---\nAGENT BODY",
    )
    .unwrap();
    let skills = Arc::new(SkillService::new(db.clone(), skills_root));
    skills.sync().unwrap();
    silicon_worker::skill::store::upsert(
        &db,
        &SkillRecord {
            id: "skill-deep-search".into(),
            source: SkillSource::User,
            name: "deep-search".into(),
            description: "深度检索".into(),
            dir_name: skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: None,
            expert_id: Some("researcher".into()),
            user_invocable: true,
            argument_hint: None,
            group_id: None,
        },
    )
    .unwrap();

    let agents = Arc::new(ExpertService::new(db.clone(), agents_root));
    agents.sync().unwrap();
    let projects = Arc::new(ProjectService::new(db.clone(), agents.clone()));
    let p = projects.create("研究项目", "", "", None).unwrap();
    // 手动加入散装成员（非快照，无 origin_team_id）。
    projects
        .add_member(&p.id, "researcher", Some("研究员"), None, false)
        .unwrap();
    assert_eq!(
        projects.member_expert_names(&p.id).unwrap(),
        vec!["researcher".to_string()]
    );

    let store = SessionStore::open(db.clone()).unwrap();
    let s = store
        .create_session("s-proj-agent", "project", "1", false)
        .unwrap();
    store.set_project_id(&s.id, &p.id, "2").unwrap();

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(LoadSkill));
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(ProjectSkillClient {
            calls: AtomicUsize::new(0),
            skill: "deep-search",
        }),
    )
    .with_experts(agents)
    .with_projects(projects)
    .with_skills(skills)
    .with_registry(registry);

    let (detail, pending) = engine
        .submit_user_message(
            &s.id,
            "用深度检索",
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();

    assert!(pending.is_none());
    let tool_msg = detail
        .messages
        .iter()
        .find(|m| m.tool_name.as_deref() == Some("load_skill"))
        .expect("应有 load_skill 结果");
    assert_eq!(
        tool_msg.content.trim(),
        "AGENT BODY",
        "手动加进项目的散装成员的 agent 私有 skill 应在项目运行时可解析"
    );
}

#[test]
fn project_runtime_skills_match_project_run_private_skill_pool() {
    let (db, root) = setup();
    let skills_root = root.join("skills");
    let team_skill_dir = skills_root.join("team-kb-private");
    let agent_skill_dir = skills_root.join("agent-research-private");
    let disabled_skill_dir = skills_root.join("agent-disabled-private");
    let hidden_skill_dir = skills_root.join("agent-hidden-private");
    let global_skill_dir = skills_root.join("global-skill");
    for dir in [
        &team_skill_dir,
        &agent_skill_dir,
        &disabled_skill_dir,
        &hidden_skill_dir,
        &global_skill_dir,
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(
        team_skill_dir.join("SKILL.md"),
        "---\nname: team-kb\ndescription: 团队知识库\n---\nTEAM BODY",
    )
    .unwrap();
    std::fs::write(
        agent_skill_dir.join("SKILL.md"),
        "---\nname: agent-research\ndescription: 专家研究\n---\nAGENT BODY",
    )
    .unwrap();
    std::fs::write(
        disabled_skill_dir.join("SKILL.md"),
        "---\nname: disabled-private\ndescription: 禁用私有\n---\nDISABLED",
    )
    .unwrap();
    std::fs::write(
        hidden_skill_dir.join("SKILL.md"),
        "---\nname: hidden-private\ndescription: 内部私有\n---\nHIDDEN",
    )
    .unwrap();
    std::fs::write(
        global_skill_dir.join("SKILL.md"),
        "---\nname: global-public\ndescription: 全局技能\n---\nGLOBAL",
    )
    .unwrap();
    let skills = Arc::new(SkillService::new(db.clone(), skills_root));
    skills.sync().unwrap();

    for rec in [
        SkillRecord {
            id: "skill-team-kb".into(),
            source: SkillSource::User,
            name: "team-kb".into(),
            description: "团队知识库".into(),
            dir_name: team_skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: Some("team-1".into()),
            expert_id: None,
            user_invocable: true,
            argument_hint: None,
            group_id: None,
        },
        SkillRecord {
            id: "skill-agent-research".into(),
            source: SkillSource::User,
            name: "agent-research".into(),
            description: "专家研究".into(),
            dir_name: agent_skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: None,
            expert_id: Some("researcher".into()),
            user_invocable: true,
            argument_hint: None,
            group_id: None,
        },
        SkillRecord {
            id: "skill-disabled-private".into(),
            source: SkillSource::User,
            name: "disabled-private".into(),
            description: "禁用私有".into(),
            dir_name: disabled_skill_dir.to_string_lossy().into_owned(),
            enabled: false,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: None,
            expert_id: Some("researcher".into()),
            user_invocable: true,
            argument_hint: None,
            group_id: None,
        },
        SkillRecord {
            id: "skill-hidden-private".into(),
            source: SkillSource::User,
            name: "hidden-private".into(),
            description: "内部私有".into(),
            dir_name: hidden_skill_dir.to_string_lossy().into_owned(),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            plugin_id: None,
            team_id: None,
            expert_id: Some("researcher".into()),
            user_invocable: false,
            argument_hint: None,
            group_id: None,
        },
    ] {
        silicon_worker::skill::store::upsert(&db, &rec).unwrap();
    }

    let agents = Arc::new(ExpertService::new(db.clone(), root.join("agents")));
    let projects = Arc::new(ProjectService::new(db.clone(), agents));
    let p = projects.create("研究项目", "", "", None).unwrap();
    projects
        .add_member(&p.id, "researcher", Some("研究员"), None, false)
        .unwrap();
    projects
        .add_member_snapshot(
            &p.id,
            "writer",
            Some("写手"),
            Some("写作"),
            None,
            Some("写手"),
            "你是写手。",
            vec!["load_skill".into()],
            "main",
            Some("team-1"),
        )
        .unwrap();

    let project_skills =
        silicon_worker::project::runtime_skills::list_project_runtime_skill_items(
            &projects,
            &skills,
            |team_id| match team_id {
                "team-1" => Some("内容团队".to_string()),
                _ => None,
            },
            &p.id,
        )
        .unwrap();
    let labels: Vec<(String, String, String)> = project_skills
        .into_iter()
        .map(|item| (item.skill.name, item.source_kind, item.source_name))
        .collect();
    assert_eq!(
        labels,
        vec![
            (
                "agent-research".to_string(),
                "expert".to_string(),
                "researcher".to_string(),
            ),
            (
                "team-kb".to_string(),
                "team".to_string(),
                "内容团队".to_string(),
            ),
        ]
    );
}
