use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::app_state::{now_string, AppState, RunOrigin};
use crate::engine::event::AgentStreamEvent;
use crate::engine::{EngineBuilder, RunGuard, RunRegistry};
use crate::provider::ProviderGateway;
use crate::session::SessionStore;
use crate::storage::AppDatabase;

/// 运行时编排：run 生命周期。拥有 run 运行时状态（cancel_flags / run_registry），
/// 构造引擎走内部持有的 `EngineBuilder`。后台线程不捕获 `self`：捕获 `AppHandle`，
/// 线程内 `app.state::<AppState>()` 取回后调同签名委派包装，故方法体内 `st.xxx(...)` 无需改动。
pub struct RunCoordinator {
    pub(crate) engine_builder: Arc<EngineBuilder>,
    pub(crate) session: Arc<SessionStore>,
    pub(crate) app: tauri::AppHandle,
    gateway: Arc<ProviderGateway>,
    db: Arc<AppDatabase>,
    remote_hub: Arc<crate::remote::RemoteHub>,
    /// per-session 取消标记。`stop_session` 命令 set true；submit/resume 开始时 reset false。
    /// 引擎在每轮/token 检查点读取，命中则停下并保留已产出。
    cancel_flags: std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    >,
    /// per-session 运行锁，保证同会话不并发跑 run（防刷新/重开导致重复提交与历史交错）。
    pub(crate) run_registry: RunRegistry,
    /// T70：会话任务队列 read-modify-write 串行锁。队列读改写很短，用单一全局锁足够，
    /// 与 run_registry 协同保证"队头 running ⇔ 在飞 run"不变式。
    pub(crate) task_queue_lock: std::sync::Mutex<()>,
}

impl RunCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine_builder: Arc<EngineBuilder>,
        session: Arc<SessionStore>,
        app: tauri::AppHandle,
        gateway: Arc<ProviderGateway>,
        db: Arc<AppDatabase>,
        remote_hub: Arc<crate::remote::RemoteHub>,
    ) -> Self {
        Self {
            engine_builder,
            session,
            app,
            gateway,
            db,
            remote_hub,
            cancel_flags: std::sync::Mutex::new(std::collections::HashMap::new()),
            run_registry: RunRegistry::default(),
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
                let (reason, pending) = {
                    let _guard = guard;
                    match engine.resume_with_heartbeat(&sid, cancel, heartbeat) {
                        Ok((_, Some(p))) => ("paused", Some(p)),
                        // 取消标记在 resume 返回**之后**读：被检查点退出的
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
                // paused → drain_decision 返回 Noop，不动队列（含队头保持 running 等就地续跑）。
                {
                    let st = app.state::<AppState>();
                    if let Err(e) = st.coordinator.drain_session_queue(&sid, reason) {
                        eprintln!("[run] 队列排空失败 会话={sid}：{e}");
                    }
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

    /// 驱动一条用户消息：占运行锁 → 升级草稿 → 落消息（首条生成标题）→ 后台跑引擎。
    /// Tauri 命令与远程接入共用同一执行路径。运行锁被占用返回 Err。
    pub fn spawn_user_message(&self, session_id: &str, content: &str) -> Result<(), String> {
        self.spawn_user_message_with_origin(session_id, content, RunOrigin::Local)
    }

    pub(crate) fn spawn_user_message_with_origin(
        &self,
        session_id: &str,
        content: &str,
        origin: RunOrigin,
    ) -> Result<(), String> {
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
            task_queue::enqueue_into_store(&self.session, session_id, item, is_member, &now)?
        };
        match outcome {
            EnqueueResult::Overflow => Err("队列已满，请稍候再发送。".to_string()),
            EnqueueResult::Queued => {
                // 已有在飞 run，仅入队；前端据投影显示"排队中"。
                emit_run_event(&self.app, "queued_tasks_updated", session_id, None);
                Ok(())
            }
            EnqueueResult::Promote(head) => {
                self.promote_and_run_user_message(session_id, &head, origin)
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
    pub(crate) fn drain_session_queue(&self, session_id: &str, reason: &str) -> Result<(), String> {
        use crate::session::task_queue::{self, DrainNext, TaskKind};
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

    /// 驱动权限决定（批准/拒绝）后续跑引擎。
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
        } else {
            self.session.append_tool_result(
                &crate::session::new_id("msg"),
                session_id,
                tool_call_id,
                &tool_name,
                "用户拒绝了该操作。",
                "done",
                &now,
            )?;
        }
        self.spawn_run_with_origin(session_id, guard, origin)
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
        let _guard = self
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
