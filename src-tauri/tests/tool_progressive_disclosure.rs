// 渐进式披露（T83）激活闭环——用真实组件验证「Deferred 工具激活前不进 tools[]、
// 经 activate_tools 后进 tools[]」这一闭环。
//
// 不伪造 Engine：复用真实 ToolRegistry（注册真实 WebFetch=Deferred）+ 真实 SessionStore，
// 并照搬 engine::run_loop_inner 里那段「核心集 ∪ 会话已激活集」的过滤表达式，
// 证明披露闸与 activate_tools/list_activated_tools 的回环在真实接缝处成立。

use std::collections::HashSet;
use std::sync::Arc;

use silicon_worker::session::SessionStore;
use silicon_worker::storage::AppDatabase;
use silicon_worker::tools::web_fetch::WebFetch;
use silicon_worker::tools::{include_in_tools, ToolRegistry};

fn temp_store() -> SessionStore {
    let dir = std::env::temp_dir().join(format!(
        "siw-t83_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let db = Arc::new(AppDatabase::open(dir.join("app.sqlite3")).expect("db"));
    SessionStore::open(db).expect("store")
}

/// 与 engine::run_loop_inner 同款过滤：返回本轮会进 tools[] 的工具名集合。
fn visible_tool_names(registry: &ToolRegistry, activated: &HashSet<String>, mode: &str) -> HashSet<String> {
    registry
        .specs()
        .into_iter()
        .filter(|spec| {
            let requires_confirmation = registry
                .get(&spec.name)
                .map(|t| t.requires_confirmation())
                .unwrap_or(false);
            include_in_tools(
                spec.disclosure,
                activated.contains(&spec.name),
                requires_confirmation,
                mode,
                &spec.name,
            )
        })
        .map(|spec| spec.name)
        .collect()
}

#[test]
fn activation_makes_deferred_tool_visible_to_model() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(WebFetch));
    let store = temp_store();
    let sid = "sess-t83-loop";

    // 前置：web_fetch 确实是 Deferred（披露闸的前提）。
    let spec = registry
        .specs()
        .into_iter()
        .find(|s| s.name == "web_fetch")
        .expect("web_fetch 已注册");
    assert_eq!(spec.disclosure, silicon_worker::tools::Disclosure::Deferred);

    // 1) 未激活：Deferred 工具不进 tools[]。
    let activated: HashSet<String> = store
        .list_activated_tools(sid)
        .expect("list")
        .into_iter()
        .collect();
    assert!(activated.is_empty());
    let before = visible_tool_names(&registry, &activated, "normal");
    assert!(
        !before.contains("web_fetch"),
        "激活前 web_fetch 不应进 tools[]，实得: {before:?}"
    );

    // 2) 激活（模拟模型调 find_tools 命中后引擎写库）。
    store.activate_tools(sid, &["web_fetch".into()]).expect("activate");

    // 3) 激活后：list_activated_tools 回环 + 披露闸放行，web_fetch 进 tools[]。
    let activated: HashSet<String> = store
        .list_activated_tools(sid)
        .expect("list")
        .into_iter()
        .collect();
    assert!(activated.contains("web_fetch"));
    let after = visible_tool_names(&registry, &activated, "normal");
    assert!(
        after.contains("web_fetch"),
        "激活后 web_fetch 应进 tools[]，实得: {after:?}"
    );
}
