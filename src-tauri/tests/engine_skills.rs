// 引擎技能注入与 load_skill 拦截（文件型 skill）。
// ① system_prompt(&[SkillSummary]) 有/无技能时的「可用技能」段。
// ② load_skill{name} 被引擎按名拦截：从磁盘 SKILL.md 读正文回灌 tool 结果，不暂停。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use silicon_agent::context::prompt::system_prompt;
use silicon_agent::engine::event::AgentStreamEvent;
use silicon_agent::engine::Engine;
use silicon_agent::provider::client::{
    ModelCallRequest, ModelCallResult, ModelClient, ModelEvent, ProviderCallError,
};
use silicon_agent::session::SessionStore;
use silicon_agent::skill::types::SkillSummary;
use silicon_agent::skill::SkillService;
use silicon_agent::skill::SkillSource;
use silicon_agent::storage::AppDatabase;
use silicon_agent::tools::load_skill::LoadSkill;
use silicon_agent::tools::ToolRegistry;

fn sample_summary() -> SkillSummary {
    SkillSummary {
        id: "skill-1".into(),
        source: SkillSource::User,
        name: "weather-style".into(),
        description: "天气回答风格".into(),
        enabled: true,
        installed_at: "100".into(),
        plugin_id: None,
        team_id: None,
        user_invocable: true,
        argument_hint: None,
        group_id: None,
    }
}

#[test]
fn system_prompt_lists_enabled_skills() {
    let s = sample_summary();
    let prompt = system_prompt(std::slice::from_ref(&s), "normal", "");
    assert!(prompt.contains("可用技能"));
    assert!(prompt.contains("weather-style"));
    assert!(prompt.contains("天气回答风格"));
    assert!(prompt.contains("load_skill"));
}

#[test]
fn system_prompt_without_skills_has_no_section() {
    let prompt = system_prompt(&[], "normal", "");
    assert!(!prompt.contains("可用技能"));
}

struct LoadSkillClient {
    calls: AtomicUsize,
}

impl ModelClient for LoadSkillClient {
    fn stream_model_with_events(
        &self,
        _request: ModelCallRequest,
        on_event: &mut dyn FnMut(ModelEvent) -> bool,
    ) -> Result<ModelCallResult, ProviderCallError> {
        let turn = self.calls.fetch_add(1, Ordering::SeqCst);
        if turn == 0 {
            let args = serde_json::json!({ "name": "x" });
            on_event(ModelEvent::ToolCallCreated {
                id: "call-1".into(),
                name: "load_skill".into(),
                arguments_json: String::new(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::ToolCallCreated {
                    id: "call-1".into(),
                    name: "load_skill".into(),
                    arguments_json: args.to_string(),
                }],
                usage: None,
                finish_reason: Some("tool_calls".into()),
            })
        } else {
            on_event(ModelEvent::Delta {
                text: "按技能正文作答。".into(),
            });
            Ok(ModelCallResult {
                events: vec![ModelEvent::AssistantMessageCompleted {
                    content: "按技能正文作答。".into(),
                }],
                usage: None,
                finish_reason: Some("stop".into()),
            })
        }
    }
}

fn temp_base() -> std::path::PathBuf {
    static C: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = C.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("siw-engine-skills_{}_{}", std::process::id(), seq))
}

#[test]
fn load_skill_reads_body_from_disk_inline() {
    let base = temp_base();
    let _ = std::fs::remove_dir_all(&base);
    let db = Arc::new(AppDatabase::open(base.join("app.sqlite3")).expect("db"));

    // 在技能根目录手写技能 x（SKILL.md 正文 "技能正文"）。
    let root = base.join("skills");
    let dir = root.join("x");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: x\ndescription: 示例\n---\n技能正文",
    )
    .unwrap();
    let skills = Arc::new(SkillService::new(db.clone(), root));
    skills.sync().expect("sync");

    let store = SessionStore::open(db.clone()).expect("store");
    let session = store
        .create_session("s1", "skills", "100", false)
        .expect("session");

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(LoadSkill));

    let events: Arc<Mutex<Vec<AgentStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let engine = Engine::new(
        SessionStore::open(db.clone()).unwrap(),
        Arc::new(LoadSkillClient {
            calls: AtomicUsize::new(0),
        }),
    )
    .with_registry(registry)
    .with_skills(skills)
    .with_emitter(Arc::new(move |e| ev.lock().unwrap().push(e)));

    let (detail, pending) = engine
        .submit_user_message(&session.id, "用技能回答", Arc::new(AtomicBool::new(false)))
        .expect("submit");

    assert!(pending.is_none());
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);
    let tool_msg = &detail.messages[2];
    assert_eq!(tool_msg.tool_name.as_deref(), Some("load_skill"));
    assert_eq!(tool_msg.content.trim(), "技能正文");
}
