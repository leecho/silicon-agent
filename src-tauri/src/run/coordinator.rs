use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::app_settings::AppSettingsStore;
use crate::app_state::{now_string, AppState, RunOrigin};
use crate::engine::event::AgentStreamEvent;
use crate::engine::{EngineBuilder, RunGuard, RunRegistry};
use crate::provider::ProviderGateway;
use crate::session::SessionStore;
use crate::storage::AppDatabase;

/// 运行时编排：run 生命周期 + 子代理编排。拥有 run 运行时状态（cancel_flags / run_registry /
/// child_retries），构造引擎走内部持有的 `EngineBuilder`。后台线程不捕获 `self`：捕获 `AppHandle`，
/// 线程内 `app.state::<AppState>()` 取回后调同签名委派包装，故方法体内 `st.xxx(...)` 无需改动。
pub struct RunCoordinator {
    pub(crate) engine_builder: Arc<EngineBuilder>,
    pub(crate) session: Arc<SessionStore>,
    pub(crate) projects: Arc<crate::project::ProjectService>,
    pub(crate) app: tauri::AppHandle,
    gateway: Arc<ProviderGateway>,
    db: Arc<AppDatabase>,
    remote_hub: Arc<crate::remote::RemoteHub>,
    app_settings: Arc<AppSettingsStore>,
    /// per-session 取消标记。`stop_session` 命令 set true；submit/resume 开始时 reset false。
    /// 引擎在每轮/token 检查点读取，命中则停下并保留已产出。
    cancel_flags: std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    >,
    /// per-session 运行锁，保证同会话不并发跑 run（防刷新/重开导致重复提交与历史交错）。
    pub(crate) run_registry: RunRegistry,
    /// 子代理失败重试计数：按父 dispatch tool_call_id 计，达上限才把失败回填父。本进程内有效。
    child_retries: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    /// T70：会话任务队列 read-modify-write 串行锁。队列读改写很短，用单一全局锁足够，
    /// 与 run_registry 协同保证"队头 running ⇔ 在飞 run"不变式。
    pub(crate) task_queue_lock: std::sync::Mutex<()>,
}

impl RunCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine_builder: Arc<EngineBuilder>,
        session: Arc<SessionStore>,
        projects: Arc<crate::project::ProjectService>,
        app: tauri::AppHandle,
        gateway: Arc<ProviderGateway>,
        db: Arc<AppDatabase>,
        remote_hub: Arc<crate::remote::RemoteHub>,
        app_settings: Arc<AppSettingsStore>,
    ) -> Self {
        Self {
            engine_builder,
            session,
            projects,
            app,
            gateway,
            db,
            remote_hub,
            app_settings,
            cancel_flags: std::sync::Mutex::new(std::collections::HashMap::new()),
            run_registry: RunRegistry::default(),
            child_retries: std::sync::Mutex::new(std::collections::HashMap::new()),
            task_queue_lock: std::sync::Mutex::new(()),
        }
    }

    /// per-session 运行锁。
    pub(crate) fn run_registry(&self) -> &RunRegistry {
        &self.run_registry
    }

    /// 取（不存在则创建）指定 session 的取消标记。引擎检查点与 `stop_session` 命令共用同一标记。
    pub fn cancel_flag(&self, session_id: &str) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let mut m = self.cancel_flags.lock().unwrap();
        m.entry(session_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
            .clone()
    }

    /// 移除某 session 的取消标记（删除会话时调用，避免 cancel_flags 随建删单调增长）。
    pub fn clear_cancel_flag(&self, session_id: &str) {
        self.cancel_flags.lock().unwrap().remove(session_id);
    }

    fn subagent_execution_mode(&self) -> String {
        self.app_settings
            .get_subagent_execution_mode()
            .unwrap_or_else(|_| "parallel".to_string())
    }

    fn start_child_run_now(&self, child_id: &str) -> Result<(), String> {
        if let Some(g) = self.run_registry.try_begin(child_id) {
            self.spawn_child_run(child_id, g)?;
        }
        Ok(())
    }

    pub(crate) fn request_start_child_batch(&self, child_ids: Vec<String>) -> Result<(), String> {
        let mode = self.subagent_execution_mode();
        for child_id in select_child_start_batch(&mode, child_ids) {
            self.request_start_child_run(&child_id)?;
        }
        Ok(())
    }

    pub(crate) fn request_start_child_run(&self, child_id: &str) -> Result<(), String> {
        if self.subagent_execution_mode() != "serial" {
            return self.start_child_run_now(child_id);
        }
        let info = self
            .session
            .get_session(child_id)?
            .ok_or_else(|| format!("child 会话不存在：{child_id}"))?;
        let parent = info.parent_session_id.clone().ok_or("child 缺 parent")?;
        if self.has_running_child(&parent, info.is_background)? {
            return Ok(());
        }
        let queue = self.pending_child_queue(&parent, info.is_background)?;
        if queue.first().map(String::as_str) == Some(child_id) {
            self.start_child_run_now(child_id)?;
        }
        Ok(())
    }

    fn start_next_pending_child(&self, parent: &str, background: bool) -> Result<bool, String> {
        if self.subagent_execution_mode() != "serial"
            || self.has_running_child(parent, background)?
        {
            return Ok(false);
        }
        let Some(next) = self
            .pending_child_queue(parent, background)?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        self.session
            .set_awaiting_subagent(parent, &next, &now_string())?;
        self.start_child_run_now(&next)?;
        Ok(true)
    }

    fn has_running_child(&self, parent: &str, background: bool) -> Result<bool, String> {
        Ok(self
            .session
            .list_children(parent)?
            .into_iter()
            .any(|c| c.is_background == background && self.run_registry.is_running(&c.id)))
    }

    fn pending_child_queue(&self, parent: &str, background: bool) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for c in self.session.list_children(parent)? {
            if c.is_background != background || c.run_outcome.is_some() {
                continue;
            }
            if self.run_registry.is_running(&c.id) {
                continue;
            }
            let Some(tc) = c.parent_tool_call_id.as_deref() else {
                continue;
            };
            if !background && self.session.tool_result_status(parent, tc)?.is_some() {
                continue;
            }
            out.push(c.id);
        }
        Ok(out)
    }

    /// 后台 detached 线程跑引擎一轮 run，统一收口提交类命令与远程接入的运行编排。
    ///
    /// `guard` 由调用方先 `run_registry.try_begin` 取得并 move 进来：限定在 resume 期间，
    /// run 结束即析构解锁（`is_running` 转 false），之后才 emit `run_finished`。线程 detached，
    /// 不随 WebView 刷新/重开被杀。completed 时触发快捷建议；paused 时把编号提示推给绑定的 IM peer。
    pub(crate) fn spawn_run_with_origin(
        &self,
        session_id: &str,
        guard: RunGuard,
        origin: RunOrigin,
    ) -> Result<(), String> {
        // 子运行（origin="subagent"）的任何 run（含权限/提问应答、用户纠偏后的续跑）都必须走
        // 专用编排：受限引擎（engine_for_child）+ 完成后回填父 + 续跑父。否则会用默认引擎（全工具/
        // 无 agent prompt）且父永不续跑。
        if let Ok(Some(info)) = self.session.get_session(session_id) {
            if info.origin == "subagent" {
                return self.spawn_child_run(session_id, guard);
            }
        }
        self.engine_builder.ensure_session_workspace(session_id)?;
        let engine = self.engine_builder.engine(session_id)?;
        let app = self.app.clone();
        let gateway = self.gateway.clone();
        let db = self.db.clone();
        let hub = self.remote_hub.clone();
        let is_remote_origin = origin == RunOrigin::Remote;
        if is_remote_origin {
            self.remote_hub.begin_remote_run(session_id);
        }
        let cancel = self.cancel_flag(session_id);
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel_check = cancel.clone();
        let heartbeat = guard.heartbeat_handle();
        let sid = session_id.to_string();
        emit_run_event(&self.app, "run_started", session_id, None);
        std::thread::spawn(move || {
            let sid_panic = sid.clone();
            let app_panic = app.clone();
            let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                // resume 返回 (_, Some(pending)) 即暂停；pending 直接取自结果，无需二次查询。
                let mut parked_children: Vec<String> = Vec::new();
                let (reason, pending) = {
                    let _guard = guard;
                    match engine.resume_with_heartbeat(&sid, cancel, heartbeat) {
                        // 派发一批 child：父已停泊，记下待启动的整批（guard 在此块末释放，父转 idle）。
                        Ok((
                            _,
                            Some(crate::engine::PendingInteraction::Subagent { child_session_ids }),
                        )) => {
                            parked_children = child_session_ids;
                            ("parked", None)
                        }
                        Ok((_, Some(p))) => ("paused", Some(p)),
                        // 取消标记在 resume 返回**之后**读（同子运行 §spawn_child_run）：被检查点退出的
                        // run 引擎也返回 Ok((_,None))，提前读会读到起始 false 而误判成 completed——那会触发
                        // 后续建议、并让队列 PopAndPromote 续跑下一条排队消息（"停了又自己继续"）。
                        Ok((_, None)) => {
                            if cancel_check.load(std::sync::atomic::Ordering::Relaxed) {
                                ("cancelled", None)
                            } else {
                                ("completed", None)
                            }
                        }
                        Err(err) => {
                            eprintln!("[run] 引擎错误 会话={sid}：{err}");
                            ("failed", None)
                        }
                    }
                };
                emit_run_event(&app, "run_finished", &sid, Some(reason));
                eprintln!("[run] finish 会话={sid} reason={reason}");
                // T70：主 session 任务队列排空——按本轮终态弹队头/续跑下一个/halt/清空。
                // parked/paused → drain_decision 返回 Noop，不动队列（含队头保持 running 等就地续跑）。
                {
                    let st = app.state::<AppState>();
                    if let Err(e) = st.coordinator.drain_session_queue(&sid, reason) {
                        eprintln!("[run] 队列排空失败 会话={sid}：{e}");
                    }
                }
                // 子运行派发：父已停泊，并行启动整批 child run（经 AppHandle 取回 AppState 做编排）。
                // collect 停泊：parked_children 是「仍在运行」的后台 child（已在跑）→ try_begin 跳过；
                // 启动后调一次 advance_pending_collect 收口竞态（若窗口内已全部完成则立即续跑父）。
                if !parked_children.is_empty() {
                    let st = app.state::<AppState>();
                    if let Err(e) = st.coordinator.request_start_child_batch(parked_children) {
                        eprintln!("[agent] 启动 child run 批次失败 parent={sid}：{e}");
                    }
                    let _ = st.coordinator.advance_pending_collect(&sid);
                }
                if is_remote_origin {
                    hub.end_remote_run(&sid);
                }
                // 用户主动停止 → 不生成「下一步」建议（即使本轮刚好完成、cancel 在完成后置位的竞态也挡掉）。
                if reason == "completed"
                    && !is_remote_origin
                    && !app
                        .state::<AppState>()
                        .coordinator
                        .cancel_flag(&sid)
                        .load(std::sync::atomic::Ordering::Relaxed)
                {
                    crate::aux_gen::suggestions::spawn(
                        app.clone(),
                        gateway.clone(),
                        db.clone(),
                        sid.clone(),
                    );
                }
                // 远程：暂停时把编号提示推给绑定的 IM peer（无远程绑定则 notify_pending 内部直接返回）。
                if is_remote_origin {
                    if let Some(p) = pending {
                        let rp = match p {
                            crate::engine::PendingInteraction::Permission(pp) => {
                                Some(crate::remote::RemotePending::Permission {
                                    tool_call_id: pp.tool_call_id,
                                    tool_name: pp.tool_name,
                                    input: pp.input,
                                })
                            }
                            crate::engine::PendingInteraction::Ask(a) => {
                                Some(crate::remote::RemotePending::Ask {
                                    tool_call_id: a.tool_call_id,
                                    questions: a.questions,
                                })
                            }
                            crate::engine::PendingInteraction::Plan(pl) => {
                                let plan_text = if pl.plan_markdown.trim().is_empty() {
                                    pl.summary
                                } else {
                                    pl.plan_markdown
                                };
                                Some(crate::remote::RemotePending::Plan {
                                    tool_call_id: pl.tool_call_id,
                                    plan_text,
                                })
                            }
                            // 子运行派发信号是内部态，不推远程。
                            crate::engine::PendingInteraction::Subagent { .. } => None,
                        };
                        if let Some(rp) = rp {
                            hub.notify_pending(&sid, &rp, &now_string());
                        }
                    }
                }
            }));
            if panic_result.is_err() {
                // 线程 panic：guard 已在展开中析构(释放租约)，但收尾被跳过 → 主动收敛，避免父永久停泊/卡 UI。
                eprintln!("[run] 线程 panic 会话={sid_panic}，转收敛");
                emit_run_event(&app_panic, "run_finished", &sid_panic, Some("failed"));
                app_panic
                    .state::<AppState>()
                    .coordinator
                    .reconcile(&sid_panic);
            }
        });
        Ok(())
    }

    pub fn spawn_run(&self, session_id: &str, guard: RunGuard) -> Result<(), String> {
        self.spawn_run_with_origin(session_id, guard, RunOrigin::Local)
    }

    /// 启动一个 child（origin="subagent"）子运行（受限引擎）。child 跑完（completed/failed）→
    /// 取摘要回填父 dispatch tool_call、清父停泊、续跑父 run（异步派生续跑，§6.2）。
    /// child 暂停（权限/ask）→ 落在 child 会话由 UI 处理，父保持停泊，child 续跑到 completed 再回填。
    pub(crate) fn spawn_child_run(
        &self,
        child_session_id: &str,
        guard: RunGuard,
    ) -> Result<(), String> {
        let engine = self.engine_builder.engine_for_child(child_session_id)?;
        let app = self.app.clone();
        let cancel = self.cancel_flag(child_session_id);
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);
        let cid = child_session_id.to_string();
        emit_run_event(&self.app, "run_started", child_session_id, None);
        let cancel_check = cancel.clone();
        let heartbeat = guard.heartbeat_handle();
        std::thread::spawn(move || {
            let cid_panic = cid.clone();
            let app_panic = app.clone();
            let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let reason = {
                    let _guard = guard;
                    let result = engine.resume_with_heartbeat(&cid, cancel, heartbeat);
                    // 取消标记要在 resume 返回**之后**读：停止发生在运行期间，提前读会读到起始值(false)，
                    // 导致被检查点退出误判为 completed、把 cancelled 覆写回 done（“取消没反应、照常完成”）。
                    let cancelled = cancel_check.load(std::sync::atomic::Ordering::Relaxed);
                    match result {
                        Ok((_, None)) => {
                            if cancelled {
                                "cancelled"
                            } else {
                                "completed"
                            }
                        }
                        // child 权限/ask 暂停：落 child 会话，UI 处理；父保持停泊。
                        Ok((_, Some(_))) => "paused",
                        Err(e) => {
                            eprintln!("[agent] child run 错误 会话={cid}：{e}");
                            if cancelled {
                                "cancelled"
                            } else {
                                "failed"
                            }
                        }
                    }
                };
                emit_run_event(&app, "run_finished", &cid, Some(reason));
                if reason == "paused" {
                    return;
                }
                let st = app.state::<AppState>();
                let outcome = match reason {
                    "failed" => "failed",
                    "cancelled" => "cancelled",
                    _ => "done",
                };
                let _ = st.session.set_run_outcome(&cid, outcome, &now_string());
                // T61：任务台账联动——若本 child 关联了某任务，按运行终态置其状态。
                let _ = st.projects.set_task_status_by_run(&cid, outcome);
                // 取消收口：若父也在被整体停止（cancel_flag 置位）→ 交父 reconcile 收敛（最后一个子退出 →
                // ConvergeParked 回填+清停泊+标记）；否则（单子取消）父的续跑由 cancel_child_run 处理。
                if reason == "cancelled" {
                    if let Some(parent) = st
                        .session
                        .get_session(&cid)
                        .ok()
                        .flatten()
                        .and_then(|s| s.parent_session_id)
                    {
                        if st
                            .coordinator
                            .cancel_flag(&parent)
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            st.coordinator.reconcile(&parent);
                        }
                    }
                    return;
                }
                // child completed/failed → 后台 child 走 collect 收口路径,前台 child 回填父。
                let is_bg = st
                    .session
                    .get_session(&cid)
                    .ok()
                    .flatten()
                    .map(|s| s.is_background)
                    .unwrap_or(false);
                if is_bg {
                    if let Err(e) = st.coordinator.finish_background_child(&cid, reason) {
                        eprintln!("[agent] 后台 child 收口失败 child={cid}：{e}");
                    }
                } else if let Err(e) = st.coordinator.finish_child_into_parent(&cid, reason) {
                    eprintln!("[agent] child 回填父失败 child={cid}：{e}");
                }
            }));
            if panic_result.is_err() {
                // 子线程 panic：先把父 dispatch 回填 failed，再收敛子，避免父永久停泊。
                eprintln!("[agent] 子 run 线程 panic 会话={cid_panic}，转收敛");
                let st = app_panic.state::<AppState>();
                let _ = st
                    .coordinator
                    .finish_child_into_parent(&cid_panic, "failed");
                st.coordinator.reconcile(&cid_panic);
            }
        });
        Ok(())
    }

    /// T57：后台 child 完成收口——失败可重试(#4);否则若父在 collect 等它则推进 collect;不回填 dispatch。
    pub(crate) fn finish_background_child(
        &self,
        child_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let info = self.session.get_session(child_id)?.ok_or("child 不存在")?;
        let parent = info.parent_session_id.clone().ok_or("child 缺 parent")?;
        let tc = info
            .parent_tool_call_id
            .clone()
            .ok_or("child 缺 tool_call")?;
        // #4 失败有限重试（仅非取消）：现造新后台 child（同 handle）重跑。
        const MAX_CHILD_RETRIES: u32 = 1;
        if reason == "failed" {
            let attempts = {
                let mut m = self.child_retries.lock().unwrap();
                let e = m.entry(tc.clone()).or_insert(0);
                *e += 1;
                *e
            };
            if attempts <= MAX_CHILD_RETRIES {
                match self.retry_child(&info, &parent, &tc) {
                    Ok(()) => return Ok(()),
                    Err(e) => eprintln!("[agent] 后台 child 重试失败 child={child_id}：{e}"),
                }
            }
        }
        self.child_retries.lock().unwrap().remove(&tc);
        let _ = self.start_next_pending_child(&parent, true);
        // 若父正等待一个包含本 child 的 collect → 推进收口。
        self.advance_pending_collect(&parent)
    }

    /// T57：推进父的 collect 停泊。若其所有目标 handle 的最新 child 均已终态 → 汇总写 collect 结果、
    /// 清停泊、续跑父。仍有运行中则继续等。幂等：collect 结果已写则只清停泊。
    pub(crate) fn advance_pending_collect(&self, parent: &str) -> Result<(), String> {
        let p = match self.session.get_session(parent)? {
            Some(p) => p,
            None => return Ok(()),
        };
        // 父已被停止：不因子运行收口而被重新拉起（spawn_run 会重置取消标记，导致 PM 续跑/再派发）。
        // 清掉停泊态收口，让会话停在已停止。
        if self
            .cancel_flag(parent)
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            let now = now_string();
            let _ = self.session.set_pending_collect(parent, None, &now);
            let _ = self.session.clear_awaiting_subagent(parent, &now);
            return Ok(());
        }
        let Some(pc_json) = p.pending_collect else {
            return Ok(());
        };
        let v: serde_json::Value =
            serde_json::from_str(&pc_json).unwrap_or(serde_json::Value::Null);
        let collect_call_id = v
            .get("collectCallId")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let handles: Vec<String> = v
            .get("handles")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        if collect_call_id.is_empty() || handles.is_empty() {
            return Ok(());
        }
        let (text, terminal, running) = self.session.collect_summary(parent, &handles)?;
        if !running.is_empty() {
            return Ok(()); // 还有未完成，继续等
        }
        let now = now_string();
        // 幂等：collect 结果已写 → 只清停泊。
        if self
            .session
            .tool_result_status(parent, &collect_call_id)?
            .is_some()
        {
            let _ = self.session.set_pending_collect(parent, None, &now);
            return Ok(());
        }
        for cid in &terminal {
            let _ = self.session.mark_collected(cid);
        }
        self.session.append_tool_result(
            &crate::session::new_id("msg"),
            parent,
            &collect_call_id,
            crate::tools::collect_agents::COLLECT_AGENTS_TOOL,
            &text,
            "done",
            &now,
        )?;
        let _ = self.app.emit(
            "agent_stream_event",
            AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: parent.to_string(),
                message_id: collect_call_id.clone(),
                sequence: 0,
                text: Some(text),
                status: Some("done".into()),
                tool_name: Some(crate::tools::collect_agents::COLLECT_AGENTS_TOOL.into()),
                tool_label: Some("收取子代理结论".into()),
                tool_call_id: Some(collect_call_id),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: now.clone(),
            },
        );
        self.session.set_pending_collect(parent, None, &now)?;
        self.session.clear_awaiting_subagent(parent, &now)?;
        if let Some(g) = self.run_registry.try_begin(parent) {
            self.spawn_run(parent, g)?;
        }
        Ok(())
    }

    /// 取消单个子代理：置取消标志停其运行线程，并把「已取消」结果回填父（幂等，防与收尾线程并发重复）。
    /// 取消不触发重试；父在该批所有 child 都有结果后续跑。
    pub fn cancel_child_run(&self, child_id: &str) -> Result<(), String> {
        let info = self.session.get_session(child_id)?.ok_or("child 不存在")?;
        if info.origin != "subagent" {
            return Err("该会话不是子代理".into());
        }
        // 停其运行线程（若在跑）。
        self.cancel_flag(child_id)
            .store(true, std::sync::atomic::Ordering::Relaxed);
        // 不再重试该 dispatch。
        if let Some(tc) = info.parent_tool_call_id.as_deref() {
            self.child_retries
                .lock()
                .unwrap()
                .insert(tc.to_string(), u32::MAX);
        }
        let now = now_string();
        let _ = self.session.set_run_outcome(child_id, "cancelled", &now);
        // 暂停态的子（无 run 循环）：补「已手动停止」标记到其自身 feed（运行中的由其线程/引擎落标记）。
        // T91：保留 mark_session_stopped_if_idle 而非走 settle_session——child（origin="subagent"）
        // 不入主 session 任务队列，无 Running 队头需复位，仅需「标记 + emit」这一窄语义。
        self.mark_session_stopped_if_idle(child_id, &now);
        // T61：即时把关联的任务标为已取消（否则要等子运行跑到检查点退出才联动，任务会滞留「进行中」）。
        let _ = self.projects.set_task_status_by_run(child_id, "cancelled");
        // 通知该线程的任务台账即时刷新（emit tasks_updated，sessionId=父线程）。
        if let Some(parent) = info.parent_session_id.as_deref() {
            emit_run_event(&self.app, "tasks_updated", parent, None);
        }
        if info.is_background {
            // 后台 child：标记取消终态 → 若父在 collect 等它则推进收口。
            if let Some(parent) = info.parent_session_id.as_deref() {
                let _ = self.start_next_pending_child(parent, true);
                return self.advance_pending_collect(parent);
            }
            return Ok(());
        }
        // 前台 child：直接回填取消结果（幂等：若线程已抢先回填则跳过）。
        self.finish_child_into_parent(child_id, "cancelled")
    }

    /// 给一个**没有在跑 run 循环**的会话补「已手动停止」标记，并 emit `run_finished(cancelled)` 让前端
    /// 重建其 feed（显示该分隔线）+ 清相关卡片。活动运行中的会话由引擎 `finish_stopped` 落标记、其线程
    /// 退出时自带 run_finished，这里跳过以免重复。停泊/暂停态的会话（含被级联取消的子代理）靠它得到反馈。
    fn mark_session_stopped_if_idle(&self, session_id: &str, now: &str) {
        if self.run_registry.is_running(session_id) {
            return;
        }
        self.session.append_stopped_marker(session_id, now);
        emit_run_event(&self.app, "run_finished", session_id, Some("cancelled"));
    }

    /// 失败重试：用失败 child 的 spec + 原始首条任务消息现造一个新 child（同父、同 tool_call），并启动。
    fn retry_child(
        &self,
        failed: &crate::session::SessionInfo,
        parent: &str,
        tc: &str,
    ) -> Result<(), String> {
        let failed_id = failed.id.clone();
        let name = failed.expert_name.clone().unwrap_or_default();
        let task_display = failed.agent_task.clone().unwrap_or_default();
        // 原始首条任务消息（含 inputs 增补）作为重试 child 的任务；取不到则回退展示任务。
        let task_msg = self
            .session
            .list_messages(&failed_id)?
            .into_iter()
            .find(|m| m.role == "user")
            .map(|m| m.content)
            .unwrap_or_else(|| task_display.clone());
        let now = now_string();
        let new_id = crate::session::new_id("session");
        // 子会话标题用成员展示名（按父会话角色解析）；解析不到回退原始 name。
        let (rk, ri) = self
            .session
            .get_session(parent)
            .ok()
            .flatten()
            .map(|s| {
                if let Some(project_id) = s.project_id {
                    ("project".to_string(), project_id)
                } else {
                    (
                        s.role_kind.unwrap_or_default(),
                        s.role_id.unwrap_or_default(),
                    )
                }
            })
            .unwrap_or_default();
        let display = self
            .engine_builder
            .resolve_role_summary(&rk, &ri, &name)
            .and_then(|s| s.display_name);
        self.session.create_child_session(
            &new_id,
            parent,
            tc,
            &name,
            &task_display,
            failed.expert_system_prompt.as_deref(),
            failed.expert_tools.as_deref(),
            failed.is_background,
            &now,
            display.as_deref(),
        )?;
        self.session.append_message(
            &crate::session::new_id("msg"),
            &new_id,
            "user",
            &task_msg,
            None,
            &now,
        )?;
        // T61：重试 → 把指向旧 run 的任务改指新 run（状态回到 in_progress）。
        let _ = self.projects.reassign_task_run(&failed_id, &new_id);
        eprintln!("[agent] 子代理 {name} 失败，自动重试 → {new_id}");
        if let Some(g) = self.run_registry.try_begin(&new_id) {
            self.spawn_child_run(&new_id, g)?;
        }
        Ok(())
    }

    /// child 完成后：取 child 摘要写回父 dispatch tool_call、清父停泊、续跑父 run。
    pub(crate) fn finish_child_into_parent(
        &self,
        child_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let info = self.session.get_session(child_id)?.ok_or("child 不存在")?;
        let parent = info.parent_session_id.clone().ok_or("child 缺 parent")?;
        let tc = info
            .parent_tool_call_id
            .clone()
            .ok_or("child 缺 parent_tool_call_id")?;

        // 幂等：该 dispatch 已有结果（如取消与 run 线程收尾并发、或重复回填）→ 直接返回，避免重复 tool 结果。
        if self.session.tool_result_status(&parent, &tc)?.is_some() {
            return Ok(());
        }

        // #4 失败有限重试：child failed 且未达上限 → 现造一个全新 child（同 spec/任务、同父 tool_call）
        // 再跑一次，不回填失败、不清停泊。失败 child 本身保留可查。达上限才落 failed。
        // 取消（reason="cancelled"）不重试。
        const MAX_CHILD_RETRIES: u32 = 1;
        if reason == "failed" {
            let attempts = {
                let mut m = self.child_retries.lock().unwrap();
                let e = m.entry(tc.clone()).or_insert(0);
                *e += 1;
                *e
            };
            if attempts <= MAX_CHILD_RETRIES {
                if let Err(e) = self.retry_child(&info, &parent, &tc) {
                    eprintln!("[agent] child 重试失败 child={child_id}：{e}");
                } else {
                    return Ok(()); // 重试已启动，等其结果。
                }
            }
        }
        // 不再重试（成功 / 已达上限 / 重试启动失败）：清理计数。
        self.child_retries.lock().unwrap().remove(&tc);

        let summary = if reason == "cancelled" {
            let partial = self
                .session
                .last_assistant_text(child_id)?
                .unwrap_or_default();
            if partial.trim().is_empty() {
                "（用户已取消该子代理，无产出）".to_string()
            } else {
                format!("（用户已取消该子代理）此前进展：\n{partial}")
            }
        } else {
            self.session
                .last_assistant_text(child_id)?
                .unwrap_or_else(|| "（子运行无文本产出）".into())
        };
        let now = now_string();
        let status = if reason == "failed" || reason == "cancelled" {
            "failed"
        } else {
            "done"
        };
        self.session.append_tool_result(
            &crate::session::new_id("msg"),
            &parent,
            &tc,
            crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL,
            &summary,
            status,
            &now,
        )?;
        // 实时更新父 feed 的 dispatch 卡：running → done/failed。父续跑后多为 parked（再派下一个），
        // 前端 parked 不重建 feed，故必须靠这条事件把卡片从「运行中」翻成已回禀（否则卡片永久卡住）。
        let _ = self.app.emit(
            "agent_stream_event",
            AgentStreamEvent {
                kind: "tool_result".into(),
                session_id: parent.clone(),
                message_id: tc.clone(),
                sequence: 0,
                text: Some(summary.clone()),
                status: Some(status.into()),
                tool_name: Some(crate::tools::dispatch_agent::DISPATCH_AGENT_TOOL.into()),
                tool_label: Some("指派专家".into()),
                tool_call_id: Some(tc.clone()),
                todos: None,
                artifacts: None,
                parent_session_id: None,
                parent_tool_call_id: None,
                expert_name: None,
                created_at: now.clone(),
            },
        );
        // 并行派发：本批可能有多个 child。只有当父名下所有 child 都已回填结果时才清停泊 + 续跑父，
        // 否则保持停泊等其余 child。并发收尾的多个 child 线程靠 try_begin(parent) 原子去重，仅一个续跑。
        let remaining = self.session.pending_child_count(&parent).unwrap_or(0);
        if remaining > 0 {
            let _ = self.start_next_pending_child(&parent, false);
            return Ok(());
        }
        self.session.clear_awaiting_subagent(&parent, &now)?;
        // 父已被停止则不再续跑（否则 spawn_run 重置取消标记，PM 又被拉起/再派发）。
        if self
            .cancel_flag(&parent)
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(());
        }
        // 续跑父 run：父 run_loop 见全部 dispatch tool result 都有了 → 继续。
        if let Some(g) = self.run_registry.try_begin(&parent) {
            self.spawn_run(&parent, g)?;
        }
        Ok(())
    }

    /// 驱动一条用户消息：占运行锁 → 升级草稿 → 落消息（首条生成标题）→ 后台跑引擎。
    /// Tauri 命令与远程接入共用同一执行路径。运行锁被占用返回 Err。
    /// 返回 `true`=消息入队（忙时，未起跑、不进 feed）；`false`=即时起跑（空闲时，已落 feed）。
    pub fn spawn_user_message(&self, session_id: &str, content: &str) -> Result<bool, String> {
        self.spawn_user_message_with_origin(session_id, content, RunOrigin::Local)
    }

    /// 返回 `true`=入队、`false`=起跑。前端据此对账乐观气泡（T70 队列与乐观 UI 同步）。
    /// （Promote 后竞态未抢到运行槽的罕见情形按起跑计，由 drain 续跑、下一个 run_finished
    /// 重建 feed 自愈，与既有行为一致。）
    pub(crate) fn spawn_user_message_with_origin(
        &self,
        session_id: &str,
        content: &str,
        origin: RunOrigin,
    ) -> Result<bool, String> {
        use crate::session::task_queue::{
            self, EnqueueResult, SessionTaskItem, TaskKind, TaskStatus,
        };
        // T70：主 session 新用户消息走队列——忙时入队（不再拒收报错），空闲立即提升运行。
        let is_member = self
            .session
            .get_session(session_id)?
            .map(|s| s.parent_session_id.is_some())
            .unwrap_or(false);
        let now = now_string();
        let item = SessionTaskItem {
            item_id: crate::session::new_id("qtask"),
            kind: TaskKind::UserMessage,
            payload: content.to_string(),
            tool_call_id: None,
            parent_session_id: None,
            status: TaskStatus::Queued,
            enqueued_at: now.clone(),
        };
        let outcome = {
            let _lock = self.task_queue_lock.lock().unwrap();
            // T91 P2：忙态以 RAII 运行锁为唯一真相（持久化队头只用于 FIFO/reconcile）。
            // 在 task_queue_lock 临界区内读 is_running：与 settle/drain 的队列写互斥，
            // 最坏竞态是 run 刚结束被读成 false → PromoteNow 提升最老 queued（FIFO 正确，不丢消息）。
            // is_running 仅瞬时取放 registry 锁，不与 try_begin 嵌套死锁。
            let is_busy = self.run_registry.is_running(session_id);
            task_queue::enqueue_into_store(&self.session, session_id, item, is_member, is_busy, &now)?
        };
        match outcome {
            EnqueueResult::Overflow => Err("队列已满，请稍候再发送。".to_string()),
            EnqueueResult::Queued => {
                // 已有在飞 run，仅入队；前端据投影显示"排队中"。
                emit_run_event(&self.app, "queued_tasks_updated", session_id, None);
                Ok(true)
            }
            EnqueueResult::Promote(head) => {
                self.promote_and_run_user_message(session_id, &head, origin)?;
                Ok(false)
            }
        }
    }

    /// T70：把一条 user_message 队头落为会话消息并起 run（提升草稿、首条生成标题）。
    /// 与旧 `spawn_user_message` 落消息逻辑一致，差别是消息内容取自队头工作项。
    fn promote_and_run_user_message(
        &self,
        session_id: &str,
        item: &crate::session::task_queue::SessionTaskItem,
        origin: RunOrigin,
    ) -> Result<(), String> {
        let Some(guard) = self.run_registry.try_begin(session_id) else {
            // 竞态：已有 run 在跑（队头 running 已落库），等其收尾 drain 再提升。
            return Ok(());
        };
        self.session.promote_draft(session_id)?;
        let now = now_string();
        let first = self
            .session
            .get_session_detail(session_id)?
            .map(|d| d.messages.is_empty())
            .unwrap_or(true);
        self.session.append_message(
            &crate::session::new_id("msg"),
            session_id,
            "user",
            &item.payload,
            None,
            &now,
        )?;
        let _ = self.session.set_last_suggestions(session_id, &[]);
        if first {
            crate::aux_gen::title::spawn(
                self.app.clone(),
                self.gateway.clone(),
                self.db.clone(),
                session_id.to_string(),
                item.payload.clone(),
            );
        }
        emit_run_event(&self.app, "queued_tasks_updated", session_id, None);
        self.spawn_run_with_origin(session_id, guard, origin)
    }

    /// T70：run 收尾排空会话任务队列。按本轮终态多态收口（见 spec §4.3），续跑下一个 user_message。
    /// 只服务主 session 路径（child 走 spawn_child_run，不入此队列）。
    pub(crate) fn drain_session_queue(&self, session_id: &str, reason: &str) -> Result<(), String> {
        use crate::session::task_queue::{self, DrainNext, SettleOutcome, TaskKind};
        // T91 P1-T4：把终态收口统一委派唯一收口点 settle_session。
        // 仅 completed/failed 走委派——这两者与 settle_session 的事件/标记契约逐项一致：
        //   - settle_session(Completed)：仅 queued_tasks_updated + pop 队头 + 提升续跑，**不** emit run_finished
        //     （run 线程已在收尾前 emit run_finished("completed")，settle_session 的 run_finished 只覆盖
        //      Cancelled|Rejected|Interrupted，故不重复）、不落标记、不收口 tool_call（settle_text=None）。
        //     与原 drain PopAndPromote 同义（同样 pop 队头、提升下一 UserMessage 续跑）。
        //   - settle_session(Failed)：仅 queued_tasks_updated + 队列收口（UserMessage halt-and-hold / AgentTask 清空），
        //     不落标记、不 emit run_finished、不收口 tool_call，与原 drain failed 分支逐项一致。
        // cancelled **不**走 settle_session：run 线程已 emit run_finished("cancelled") 且引擎 finish_stopped
        //   已落「已手动停止」标记；settle_session(Cancelled) 会再补 stopped_marker + 二次 run_finished + 收口
        //   tool_call，属重复/回归，故保留原 DrainAll（清空整队）路径。
        // paused/parked → Noop：不动队列（队头保持 running 等就地续跑），settle_session 无对应 outcome，亦保留原路径。
        let settle_outcome = match reason {
            "completed" => Some(SettleOutcome::Completed),
            "failed" => Some(SettleOutcome::Failed),
            _ => None,
        };
        if let Some(outcome) = settle_outcome {
            return self.settle_session(session_id, outcome);
        }
        let now = now_string();
        let next = {
            let _lock = self.task_queue_lock.lock().unwrap();
            task_queue::drain_after_finish(&self.session, session_id, reason, &now)?
        };
        emit_run_event(&self.app, "queued_tasks_updated", session_id, None);
        match next {
            DrainNext::Idle => Ok(()),
            DrainNext::Promote(item) => match item.kind {
                TaskKind::UserMessage => {
                    self.promote_and_run_user_message(session_id, &item, RunOrigin::Local)
                }
                // T68 消费：成员 agent_task 的落地与回填在 T68 接入，本期不在此起 child run。
                TaskKind::AgentTask => Ok(()),
            },
        }
    }

    /// 会话停止/结束的**唯一终态收口点**（T91）：原子收口队头 + 悬空 tool_call + 标记 + 事件。幂等。
    /// 由所有「停止/结束」路径委派（P1-T3/T4 接线）；保证终态后队头不再 Running（除 Completed 续跑）。
    pub(crate) fn settle_session(
        &self,
        sid: &str,
        outcome: crate::session::task_queue::SettleOutcome,
    ) -> Result<(), String> {
        use crate::session::task_queue::{self, SettleOutcome, TaskKind};
        let now = now_string();
        // 1) 收口队头（task_queue_lock 下 read-modify-write，与 enqueue 互斥）。
        let next = {
            let _lock = self.task_queue_lock.lock().unwrap();
            let mut items = task_queue::parse_queue(self.session.get_pending_tasks(sid)?.as_deref());
            let next = task_queue::settle_queue(&mut items, outcome);
            // 队列空则写 None（清空），否则写回序列化结果。
            let serialized = task_queue::serialize_queue(&items);
            let payload = if items.is_empty() {
                None
            } else {
                serialized.as_deref()
            };
            self.session.set_pending_tasks(sid, payload, &now)?;
            next
        };
        // 2) 收口悬空 tool_call（仅停止类终态；Completed/Failed 由 run 自身落结果，不在此收口）。
        let settle_text = match outcome {
            SettleOutcome::Rejected => Some("用户拒绝了该操作，已停止会话。"),
            SettleOutcome::Cancelled => Some("会话已停止，未执行该操作。"),
            SettleOutcome::Interrupted => Some("上一轮因进程退出未完成。"),
            SettleOutcome::Completed | SettleOutcome::Failed => None,
        };
        if let Some(text) = settle_text {
            if let Ok(Some((tc, _name))) = self.session.first_dangling_tool_call(sid) {
                let _ = self.session.settle_pending_tool_call(sid, &tc, text, &now);
            }
        }
        // 3) 落标记（停止/孤儿）。Completed/Failed 不在此落（run 线程已 emit run_finished + 自带标记）。
        match outcome {
            SettleOutcome::Cancelled | SettleOutcome::Rejected => {
                self.session.append_stopped_marker(sid, &now)
            }
            SettleOutcome::Interrupted => self.session.append_interrupted_marker(sid, &now),
            SettleOutcome::Completed | SettleOutcome::Failed => {}
        }
        // 4) 事件：队列变化恒发；停止类补 run_finished。
        emit_run_event(&self.app, "queued_tasks_updated", sid, None);
        if matches!(
            outcome,
            SettleOutcome::Cancelled | SettleOutcome::Rejected | SettleOutcome::Interrupted
        ) {
            emit_run_event(&self.app, "run_finished", sid, Some("cancelled"));
        }
        // 5) Completed 续跑下一队头（如有）——复用既有提升路径（settle_queue 仅 Completed 返回 next）。
        if let Some(item) = next {
            // promote_and_run_user_message 内部 try_begin，竞态安全；与 drain_session_queue 同路。
            if item.kind == TaskKind::UserMessage {
                self.promote_and_run_user_message(sid, &item, RunOrigin::Local)?;
            }
        }
        Ok(())
    }

    /// T70：列会话任务队列投影（前端排队条数据源）。
    pub fn list_queue(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::session::task_queue::SessionTaskItem>, String> {
        Ok(crate::session::task_queue::parse_queue(
            self.session.get_pending_tasks(session_id)?.as_deref(),
        ))
    }

    /// T70：取消一个 queued 项（不影响 running 队头；取 running 走 stop_session）。
    /// 返回取消后的队列投影。
    pub fn cancel_queued_item(
        &self,
        session_id: &str,
        item_id: &str,
    ) -> Result<Vec<crate::session::task_queue::SessionTaskItem>, String> {
        let now = now_string();
        let items = {
            let _lock = self.task_queue_lock.lock().unwrap();
            let mut items = crate::session::task_queue::parse_queue(
                self.session.get_pending_tasks(session_id)?.as_deref(),
            );
            if crate::session::task_queue::remove_queued(&mut items, item_id) {
                self.session.set_pending_tasks(
                    session_id,
                    crate::session::task_queue::serialize_queue(&items).as_deref(),
                    &now,
                )?;
            }
            items
        };
        emit_run_event(&self.app, "queued_tasks_updated", session_id, None);
        Ok(items)
    }

    /// 驱动权限决定：批准 → 会话级授权该工具并续跑引擎；拒绝 → 收口该 tool_call 并**立即停止会话**（不续跑）。
    pub fn spawn_permission_decision(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
    ) -> Result<(), String> {
        self.spawn_permission_decision_with_origin(
            session_id,
            tool_call_id,
            approved,
            RunOrigin::Local,
        )
    }

    /// 决策恢复前的停止守卫：会话已被停止（`cancel_flag` 置位）时，权限批准/回答/计划决策都**不得复活**
    /// run——恢复路径会 `spawn_run` 重置 cancel_flag 并续跑，造成「停了又被确认复活」（用户报的现象）。
    /// 命中则收口该 pending（写 cancelled 结果解析掉它）+ emit `run_finished(cancelled)` 让前端清卡/重建，
    /// 返回 true 表示已拦截，调用方应直接返回。这是 stop 收口之外的第二道防线，覆盖卡片已显示时的点击。
    fn guard_stopped_decision(&self, session_id: &str, tool_call_id: &str) -> bool {
        if !self
            .cancel_flag(session_id)
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        let _ = self.session.settle_pending_tool_call(
            session_id,
            tool_call_id,
            "会话已停止，未执行该操作。",
            &now_string(),
        );
        emit_run_event(&self.app, "run_finished", session_id, Some("cancelled"));
        true
    }

    pub(crate) fn spawn_permission_decision_with_origin(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
        origin: RunOrigin,
    ) -> Result<(), String> {
        if self.guard_stopped_decision(session_id, tool_call_id) {
            return Ok(());
        }
        let guard = self
            .run_registry
            .try_begin(session_id)
            .ok_or_else(|| "该会话正在处理中，请稍候。".to_string())?;
        let tool_name = self
            .session
            .find_pending_tool_name(session_id, tool_call_id)?
            .ok_or_else(|| format!("找不到 tool_call_id={tool_call_id} 对应的工具名"))?;
        let now = now_string();
        if approved {
            self.session.grant_tool(session_id, &tool_name, &now)?;
            self.spawn_run_with_origin(session_id, guard, origin)
        } else {
            // 拒绝 = 立即停止会话：先收口该悬空 tool_call（落拒绝结果，避免 dangling tool_call），
            // 释放运行槽后补「已停止」标记 + emit run_finished，**不续跑**——不让 agent 改道继续
            // （例如 computer 失败后转用 AppleScript 等）。用户拒绝即视为「停手」。
            self.session.append_tool_result(
                &crate::session::new_id("msg"),
                session_id,
                tool_call_id,
                &tool_name,
                "用户拒绝了该操作，已停止会话。",
                "done",
                &now,
            )?;
            drop(guard);
            // T91：经唯一收口点复位队头 + 落「已停止」标记 + emit（取代 mark_session_stopped_if_idle，
            // 后者漏复位队头导致拒绝后仍「忙」、新消息误入队）。tool_call 已上行收口，settle 内为 no-op。
            let _ =
                self.settle_session(session_id, crate::session::task_queue::SettleOutcome::Rejected);
            Ok(())
        }
    }

    /// 驱动 ask_user 回答后续跑引擎。
    pub fn spawn_ask_response(
        &self,
        session_id: &str,
        tool_call_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), String> {
        self.spawn_ask_response_with_origin(session_id, tool_call_id, answers, RunOrigin::Local)
    }

    pub(crate) fn spawn_ask_response_with_origin(
        &self,
        session_id: &str,
        tool_call_id: &str,
        answers: Vec<Vec<String>>,
        origin: RunOrigin,
    ) -> Result<(), String> {
        if self.guard_stopped_decision(session_id, tool_call_id) {
            return Ok(());
        }
        let guard = self
            .run_registry
            .try_begin(session_id)
            .ok_or_else(|| "该会话正在处理中，请稍候。".to_string())?;
        let now = now_string();
        let questions = match self
            .engine_builder
            .engine(session_id)?
            .pending_interaction(session_id)?
        {
            Some(crate::engine::PendingInteraction::Ask(p)) if p.tool_call_id == tool_call_id => {
                p.questions
            }
            _ => Vec::new(),
        };
        let result_text = crate::commands::format_ask_answers(&questions, &answers);
        self.session.append_tool_result(
            &crate::session::new_id("msg"),
            session_id,
            tool_call_id,
            "ask_user",
            &result_text,
            "done",
            &now,
        )?;
        self.spawn_run_with_origin(session_id, guard, origin)
    }

    /// 取消一条待回答的 ask：落一条「已取消」工具结果（解析掉 pending，避免 reload 重现），
    /// 不再续跑引擎——本轮就此停止。仅当确有匹配的 pending ask 时才落结果（防重复点击）。
    pub fn cancel_pending_ask(&self, session_id: &str, tool_call_id: &str) -> Result<(), String> {
        // 用 run 锁串行化，避免与并发的回答提交相互抢占。
        let guard = self
            .run_registry
            .try_begin(session_id)
            .ok_or_else(|| "该会话正在处理中，请稍候。".to_string())?;
        let matched = matches!(
            self.engine_builder.engine(session_id)?.pending_interaction(session_id)?,
            Some(crate::engine::PendingInteraction::Ask(p)) if p.tool_call_id == tool_call_id
        );
        if !matched {
            return Ok(());
        }
        let now = now_string();
        // 状态置 "cancelled" 作为结构化判别（前端据此渲染分隔线，而非嗅探正文）；
        // 正文是模型读到的自然语言，保持干净。
        self.session.append_tool_result(
            &crate::session::new_id("msg"),
            session_id,
            tool_call_id,
            "ask_user",
            "用户取消了本次提问并停止了会话。",
            "cancelled",
            &now,
        )?;
        drop(guard);
        // T91：取消提问即停止会话——经唯一收口点复位队头 + 落标记 + emit（原先漏复位队头，
        // 停止后队头仍 Running，新消息误入队）。ask tool_call 已置 "cancelled" 非悬空，settle 内为 no-op。
        let _ =
            self.settle_session(session_id, crate::session::task_queue::SettleOutcome::Cancelled);
        Ok(())
    }

    /// 驱动 plan 批准/拒绝后续跑引擎。
    pub fn spawn_plan_decision(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
        comment: Option<String>,
    ) -> Result<(), String> {
        self.spawn_plan_decision_with_origin(
            session_id,
            tool_call_id,
            approved,
            comment,
            RunOrigin::Local,
        )
    }

    pub(crate) fn spawn_plan_decision_with_origin(
        &self,
        session_id: &str,
        tool_call_id: &str,
        approved: bool,
        comment: Option<String>,
        origin: RunOrigin,
    ) -> Result<(), String> {
        if self.guard_stopped_decision(session_id, tool_call_id) {
            return Ok(());
        }
        let guard = self
            .run_registry
            .try_begin(session_id)
            .ok_or_else(|| "该会话正在处理中，请稍候。".to_string())?;
        let now = now_string();
        if approved {
            self.session.set_session_mode(session_id, "normal", &now)?;
            self.session.append_tool_result(
                &crate::session::new_id("msg"),
                session_id,
                tool_call_id,
                "propose_plan",
                "[计划已批准] 用户已批准你的计划。现在已切换到执行模式，请按计划逐步实施。",
                "done",
                &now,
            )?;
        } else {
            let c = comment.unwrap_or_default();
            self.session.append_tool_result(
                &crate::session::new_id("msg"),
                session_id,
                tool_call_id,
                "propose_plan",
                &format!("[用户评论] {c}\n请保持计划模式，据此修订计划并再次调用 propose_plan。"),
                "done",
                &now,
            )?;
        }
        self.spawn_run_with_origin(session_id, guard, origin)
    }
}

/// 发 run 生命周期事件（run_started / run_finished）。复用 agent_stream_event 通道；
/// reason 放 status（completed | paused | failed）。前端据此切换「运行中」与重新同步控制态。
pub(crate) fn emit_run_event(
    app: &tauri::AppHandle,
    kind: &str,
    session_id: &str,
    reason: Option<&str>,
) {
    let _ = app.emit(
        "agent_stream_event",
        AgentStreamEvent {
            kind: kind.into(),
            session_id: session_id.into(),
            message_id: String::new(),
            sequence: 0,
            text: None,
            status: reason.map(|r| r.into()),
            tool_name: None,
            tool_label: None,
            tool_call_id: None,
            todos: None,
            artifacts: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            created_at: now_string(),
        },
    );
}

pub(crate) fn select_child_start_batch(mode: &str, child_ids: Vec<String>) -> Vec<String> {
    if mode == "serial" {
        child_ids.into_iter().take(1).collect()
    } else {
        child_ids
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn child_start_batch_uses_all_children_in_parallel_mode() {
        let child_ids = vec!["c1".to_string(), "c2".to_string(), "c3".to_string()];
        assert_eq!(
            super::select_child_start_batch("parallel", child_ids.clone()),
            child_ids
        );
    }

    #[test]
    fn child_start_batch_uses_only_first_child_in_serial_mode() {
        let child_ids = vec!["c1".to_string(), "c2".to_string(), "c3".to_string()];
        assert_eq!(
            super::select_child_start_batch("serial", child_ids),
            vec!["c1".to_string()]
        );
    }
}
