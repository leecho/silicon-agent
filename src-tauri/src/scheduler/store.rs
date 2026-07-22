use std::sync::Arc;

use crate::scheduler::types::{ScheduledTask, TaskExecution, TaskInput};
use crate::storage::AppDatabase;

pub struct TaskStore {
    db: Arc<AppDatabase>,
}

impl TaskStore {
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "
                    create table if not exists scheduled_tasks (
                        id text primary key,
                        name text not null,
                        prompt text not null,
                        schedule_spec text not null,
                        schedule_display text,
                        working_dir text,
                        project_id text,
                        agent_id text,
                        role_kind text,
                        role_id text,
                        permission_mode text,
                        model_id text,
                        enabled integer not null default 1,
                        next_run_at integer,
                        last_run_at integer,
                        created_at integer not null,
                        updated_at integer not null
                    );
                    create table if not exists task_executions (
                        id text primary key,
                        task_id text not null,
                        session_id text not null,
                        status text not null,
                        trigger text not null,
                        started_at integer not null,
                        finished_at integer,
                        error text
                    );
                    create index if not exists idx_task_exec_started
                        on task_executions(started_at desc, id);
                    create index if not exists idx_task_exec_task_id
                        on task_executions(task_id);
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 既有库迁移：补 permission_mode / model_id 列；从旧 unattended 回填后删除该列。
        self.db
            .with_connection(|c| {
                let has = |name: &str| -> rusqlite::Result<bool> {
                    c.prepare("select 1 from pragma_table_info('scheduled_tasks') where name=?1")?
                        .exists([name])
                };
                if !has("permission_mode")? {
                    c.execute("alter table scheduled_tasks add column permission_mode text", [])?;
                }
                if !has("model_id")? {
                    c.execute("alter table scheduled_tasks add column model_id text", [])?;
                }
                if !has("project_id")? {
                    c.execute("alter table scheduled_tasks add column project_id text", [])?;
                }
                if !has("agent_id")? {
                    c.execute("alter table scheduled_tasks add column agent_id text", [])?;
                }
                if !has("role_kind")? {
                    c.execute("alter table scheduled_tasks add column role_kind text", [])?;
                }
                if !has("role_id")? {
                    c.execute("alter table scheduled_tasks add column role_id text", [])?;
                }
                if has("unattended")? {
                    // 回填：auto→full、pause→manual（仅 permission_mode 为空时）。
                    c.execute(
                        "update scheduled_tasks set permission_mode = \
                         case unattended when 'auto' then 'full' when 'pause' then 'manual' else 'full' end \
                         where permission_mode is null",
                        [],
                    )?;
                    c.execute("alter table scheduled_tasks drop column unattended", [])?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn create_task(
        &self,
        id: &str,
        input: &TaskInput,
        now: i64,
        next_run_at: Option<i64>,
    ) -> Result<ScheduledTask, String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into scheduled_tasks \
                     (id, name, prompt, schedule_spec, schedule_display, \
                      working_dir, project_id, agent_id, role_kind, role_id, permission_mode, model_id, enabled, next_run_at, created_at, updated_at) \
                     values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,1,?13,?14,?14)",
                    rusqlite::params![
                        id,
                        input.name,
                        input.prompt,
                        input.schedule_spec,
                        input.schedule_display,
                        input.working_dir,
                        input.project_id,
                        input.agent_id,
                        input.role_kind,
                        input.role_id,
                        input.permission_mode,
                        input.model_id,
                        next_run_at,
                        now,
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.get_task(id)?.ok_or_else(|| "创建后读取失败".into())
    }

    pub fn update_task(
        &self,
        id: &str,
        input: &TaskInput,
        next_run_at: Option<i64>,
        now: i64,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update scheduled_tasks set \
                     name=?2, prompt=?3, schedule_spec=?4, schedule_display=?5, \
                     working_dir=?6, project_id=?7, agent_id=?8, role_kind=?9, role_id=?10, \
                     permission_mode=?11, model_id=?12, next_run_at=?13, updated_at=?14 where id=?1",
                    rusqlite::params![
                        id,
                        input.name,
                        input.prompt,
                        input.schedule_spec,
                        input.schedule_display,
                        input.working_dir,
                        input.project_id,
                        input.agent_id,
                        input.role_kind,
                        input.role_id,
                        input.permission_mode,
                        input.model_id,
                        next_run_at,
                        now,
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
        next_run_at: Option<i64>,
        now: i64,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update scheduled_tasks set enabled=?2, next_run_at=?3, updated_at=?4 where id=?1",
                    rusqlite::params![id, enabled as i64, next_run_at, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// `last_run_at` 记录本次 dispatch 时间（即任务上次触发时刻，与 `updated_at` 相同），`next_run_at` 更新为下次计划时间。
    pub fn set_next_run(&self, id: &str, next_run_at: i64, now: i64) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update scheduled_tasks set next_run_at=?2, last_run_at=?3, updated_at=?3 where id=?1",
                    rusqlite::params![id, next_run_at, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 仅更新 `next_run_at` 和 `updated_at`，**不**触碰 `last_run_at`。
    /// 用于（重）排期但任务本次并未实际运行的场景（如应用启动时的初始化排期）。
    pub fn set_next_run_only(&self, id: &str, next_run_at: i64, now: i64) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update scheduled_tasks set next_run_at=?2, updated_at=?3 where id=?1",
                    rusqlite::params![id, next_run_at, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn delete_task(&self, id: &str) -> Result<(), String> {
        self.db
            .with_transaction(|tx| {
                tx.execute(
                    "delete from task_executions where task_id=?1",
                    rusqlite::params![id],
                )?;
                tx.execute(
                    "delete from scheduled_tasks where id=?1",
                    rusqlite::params![id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn get_task(&self, id: &str) -> Result<Option<ScheduledTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id,name,prompt,schedule_spec,schedule_display,\
                     working_dir,project_id,agent_id,role_kind,role_id,permission_mode,model_id,enabled,next_run_at,last_run_at,\
                     created_at,updated_at from scheduled_tasks where id=?1",
                )?;
                let mut rows = stmt.query_map([id], task_from_row)?;
                Ok(match rows.next() {
                    Some(r) => Some(r?),
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
            .and_then(|opt| match opt {
                Some(mut t) => {
                    self.attach_derived(&mut t)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            })
    }

    pub fn list_tasks(&self) -> Result<Vec<ScheduledTask>, String> {
        let mut tasks = self
            .db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id,name,prompt,schedule_spec,schedule_display,\
                     working_dir,project_id,agent_id,role_kind,role_id,permission_mode,model_id,enabled,next_run_at,last_run_at,\
                     created_at,updated_at from scheduled_tasks order by created_at desc",
                )?;
                let rows = stmt.query_map([], task_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())?;
        for t in &mut tasks {
            self.attach_derived(t)?;
        }
        Ok(tasks)
    }

    /// enabled 且 next_run_at <= now 的任务。
    pub fn due_tasks(&self, now: i64) -> Result<Vec<ScheduledTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id,name,prompt,schedule_spec,schedule_display,\
                     working_dir,project_id,agent_id,role_kind,role_id,permission_mode,model_id,enabled,next_run_at,last_run_at,\
                     created_at,updated_at from scheduled_tasks \
                     where enabled=1 and next_run_at is not null and next_run_at<=?1 \
                     order by next_run_at asc",
                )?;
                let rows = stmt.query_map([now], task_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 启用中的全部任务（启动 catch-up 用）。
    pub fn enabled_tasks(&self) -> Result<Vec<ScheduledTask>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id,name,prompt,schedule_spec,schedule_display,\
                     working_dir,project_id,agent_id,role_kind,role_id,permission_mode,model_id,enabled,next_run_at,last_run_at,\
                     created_at,updated_at from scheduled_tasks where enabled=1",
                )?;
                let rows = stmt.query_map([], task_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub fn create_execution(
        &self,
        id: &str,
        task_id: &str,
        session_id: &str,
        trigger: &str,
        now: i64,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into task_executions (id,task_id,session_id,status,trigger,started_at) \
                     values (?1,?2,?3,'running',?4,?5)",
                    rusqlite::params![id, task_id, session_id, trigger, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn create_skipped_execution(
        &self,
        id: &str,
        task_id: &str,
        trigger: &str,
        now: i64,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into task_executions (id,task_id,session_id,status,trigger,started_at,finished_at) \
                     values (?1,?2,'','skipped',?3,?4,?4)",
                    rusqlite::params![id, task_id, trigger, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn finish_execution(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update task_executions set status=?2, error=?3, finished_at=?4 where id=?1",
                    rusqlite::params![id, status, error, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn has_active_execution(&self, task_id: &str) -> Result<bool, String> {
        self.db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from task_executions where task_id=?1 and status='running'",
                    [task_id],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())
    }

    /// 原子性检查并声明执行权：在单个事务内检查是否已有 running 执行，
    /// 若无则插入新 running 执行行并返回 `true`（声明成功）；
    /// 若已有 running 执行则返回 `false`（已被抢占，调用方应写 skipped 执行）。
    /// 这是防 TOCTOU 竞态的核心：scheduler tick 与 run_task_now 不会同时声明同一任务。
    pub fn try_begin_execution(
        &self,
        exec_id: &str,
        task_id: &str,
        session_id: &str,
        trigger: &str,
        now: i64,
    ) -> Result<bool, String> {
        self.db
            .with_transaction(|tx| {
                let n: i64 = tx.query_row(
                    "select count(*) from task_executions where task_id=?1 and status='running'",
                    rusqlite::params![task_id],
                    |r| r.get(0),
                )?;
                if n > 0 {
                    return Ok(false);
                }
                tx.execute(
                    "insert into task_executions (id,task_id,session_id,status,trigger,started_at) \
                     values (?1,?2,?3,'running',?4,?5)",
                    rusqlite::params![exec_id, task_id, session_id, trigger, now],
                )?;
                Ok(true)
            })
            .map_err(|e| e.to_string())
    }

    /// 执行记录，可选按 task / status 过滤；按 started_at 倒序。带 task_name（join）。
    pub fn list_executions(
        &self,
        task_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<TaskExecution>, String> {
        self.db
            .with_connection(|c| {
                let mut sql = String::from(
                    "select e.id,e.task_id,coalesce(t.name,''),e.session_id,e.status,e.trigger,\
                     e.started_at,e.finished_at,e.error,ss.title \
                     from task_executions e \
                     left join scheduled_tasks t on t.id=e.task_id \
                     left join sessions ss on ss.id=e.session_id where 1=1",
                );
                if task_id.is_some() {
                    sql.push_str(" and e.task_id=?1");
                }
                if status.is_some() {
                    sql.push_str(if task_id.is_some() {
                        " and e.status=?2"
                    } else {
                        " and e.status=?1"
                    });
                }
                sql.push_str(" order by e.started_at desc, e.id");
                let mut stmt = c.prepare(&sql)?;
                let map = |r: &rusqlite::Row| {
                    Ok(TaskExecution {
                        id: r.get(0)?,
                        task_id: r.get(1)?,
                        task_name: r.get(2)?,
                        session_id: r.get(3)?,
                        status: r.get(4)?,
                        trigger: r.get(5)?,
                        started_at: r.get(6)?,
                        finished_at: r.get(7)?,
                        error: r.get(8)?,
                        session_title: r.get(9)?,
                    })
                };
                let rows = match (task_id, status) {
                    (Some(t), Some(s)) => stmt.query_map(rusqlite::params![t, s], map)?,
                    (Some(t), None) => stmt.query_map(rusqlite::params![t], map)?,
                    (None, Some(s)) => stmt.query_map(rusqlite::params![s], map)?,
                    (None, None) => stmt.query_map([], map)?,
                };
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 回填派生字段（执行计数 + 最近状态）。
    fn attach_derived(&self, task: &mut ScheduledTask) -> Result<(), String> {
        use rusqlite::OptionalExtension;
        self.db
            .with_connection(|c| {
                let count: i64 = c.query_row(
                    "select count(*) from task_executions where task_id=?1",
                    [task.id.as_str()],
                    |r| r.get(0),
                )?;
                let last: Option<String> = c
                    .query_row(
                        "select status from task_executions where task_id=?1 order by started_at desc limit 1",
                        [task.id.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                task.execution_count = count;
                task.last_status = last;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

fn task_from_row(r: &rusqlite::Row) -> rusqlite::Result<ScheduledTask> {
    let enabled: i64 = r.get(12)?;
    Ok(ScheduledTask {
        id: r.get(0)?,
        name: r.get(1)?,
        prompt: r.get(2)?,
        schedule_spec: r.get(3)?,
        schedule_display: r.get(4)?,
        working_dir: r.get(5)?,
        project_id: r.get(6)?,
        agent_id: r.get(7)?,
        role_kind: r.get(8)?,
        role_id: r.get(9)?,
        permission_mode: r.get(10)?,
        model_id: r.get(11)?,
        enabled: enabled != 0,
        next_run_at: r.get(13)?,
        last_run_at: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
        execution_count: 0,
        last_status: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::types::TaskInput;

    fn temp_store() -> TaskStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-sched-test-{nanos}.sqlite3"));
        let db = std::sync::Arc::new(crate::storage::AppDatabase::open(path).unwrap());
        // 生产中 sessions 表由 SessionStore 在同一 DB 建好；list_executions 会 LEFT JOIN sessions，
        // 故测试也需建 sessions 表（否则 JOIN 报 "no such table: sessions"）。
        crate::session::SessionStore::open(db.clone()).unwrap();
        TaskStore::open(db).unwrap()
    }

    fn sample_input() -> TaskInput {
        TaskInput {
            name: "每日报表".into(),
            prompt: "汇总昨日数据".into(),
            schedule_spec: "0 0 9 * * *".into(),
            schedule_display: Some("每天 09:00".into()),
            working_dir: None,
            project_id: None,
            agent_id: None,
            role_kind: None,
            role_id: None,
            permission_mode: Some("full".into()),
            model_id: None,
        }
    }

    #[test]
    fn create_then_get_roundtrips() {
        let store = temp_store();
        let task = store
            .create_task("task-1", &sample_input(), 1000, Some(1767258000))
            .unwrap();
        assert_eq!(task.id, "task-1");
        assert_eq!(task.name, "每日报表");
        assert!(task.enabled);
        assert_eq!(task.next_run_at, Some(1767258000));

        let got = store.get_task("task-1").unwrap().unwrap();
        assert_eq!(got.prompt, "汇总昨日数据");
        assert_eq!(got.permission_mode.as_deref(), Some("full"));
    }

    #[test]
    fn task_context_fields_roundtrip_and_update() {
        let store = temp_store();
        let mut input = sample_input();
        input.project_id = Some("project-1".into());
        input.role_kind = Some("team".into());
        input.role_id = Some("team-1".into());
        let created = store
            .create_task("ctx", &input, 1000, Some(1767258000))
            .unwrap();
        assert_eq!(created.project_id.as_deref(), Some("project-1"));
        assert!(created.agent_id.is_none());
        assert_eq!(created.role_kind.as_deref(), Some("team"));
        assert_eq!(created.role_id.as_deref(), Some("team-1"));

        let mut updated = sample_input();
        updated.agent_id = Some("agent-1".into());
        updated.role_kind = Some("expert".into());
        updated.role_id = Some("expert-1".into());
        store
            .update_task("ctx", &updated, Some(2000), 1100)
            .unwrap();

        let got = store.get_task("ctx").unwrap().unwrap();
        assert!(got.project_id.is_none());
        assert_eq!(got.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(got.role_kind.as_deref(), Some("expert"));
        assert_eq!(got.role_id.as_deref(), Some("expert-1"));
    }

    #[test]
    fn due_tasks_only_enabled_and_past() {
        let store = temp_store();
        store
            .create_task("due", &sample_input(), 1000, Some(500))
            .unwrap(); // 过期
        store
            .create_task("future", &sample_input(), 1000, Some(5000))
            .unwrap(); // 未来
        store
            .create_task("off", &sample_input(), 1000, Some(100))
            .unwrap();
        store.set_enabled("off", false, None, 1000).unwrap(); // 停用

        let due = store.due_tasks(1000).unwrap();
        let ids: Vec<_> = due.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["due"]);
    }

    #[test]
    fn set_next_run_updates() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        store.set_next_run("t", 9999, 2000).unwrap();
        assert_eq!(
            store.get_task("t").unwrap().unwrap().next_run_at,
            Some(9999)
        );
    }

    #[test]
    fn set_next_run_only_does_not_touch_last_run() {
        let store = temp_store();
        // 创建时 last_run_at 为 None。
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        let before = store.get_task("t").unwrap().unwrap();
        assert_eq!(before.last_run_at, None, "初始状态 last_run_at 应为 None");

        // 调用 set_next_run_only：仅更新 next_run_at，不写 last_run_at。
        store.set_next_run_only("t", 9999, 2000).unwrap();
        let after = store.get_task("t").unwrap().unwrap();
        assert_eq!(after.next_run_at, Some(9999), "next_run_at 应已更新");
        assert_eq!(
            after.last_run_at, None,
            "set_next_run_only 不应修改 last_run_at"
        );
    }

    #[test]
    fn executions_record_and_list() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        store
            .create_execution("e1", "t", "sess-1", "schedule", 1000)
            .unwrap();
        store
            .finish_execution("e1", "completed", None, 1300)
            .unwrap();
        store
            .create_execution("e2", "t", "sess-2", "manual", 1400)
            .unwrap();

        let list = store.list_executions(None, None).unwrap();
        assert_eq!(list.len(), 2);
        // 按 started_at 倒序：e2 先。
        assert_eq!(list[0].id, "e2");
        assert_eq!(list[0].status, "running");
        assert_eq!(list[1].id, "e1");
        assert_eq!(list[1].status, "completed");
        assert_eq!(list[1].finished_at, Some(1300));

        // 按 task 过滤。
        let by_task = store.list_executions(Some("t"), None).unwrap();
        assert_eq!(by_task.len(), 2);
        // 按状态过滤。
        let running = store.list_executions(None, Some("running")).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "e2");
    }

    #[test]
    fn has_active_execution_detects_running() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        assert!(!store.has_active_execution("t").unwrap());
        store
            .create_execution("e1", "t", "sess-1", "schedule", 1000)
            .unwrap();
        assert!(store.has_active_execution("t").unwrap());
        store
            .finish_execution("e1", "completed", None, 1300)
            .unwrap();
        assert!(!store.has_active_execution("t").unwrap());
    }

    #[test]
    fn delete_task_removes_executions() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        store
            .create_execution("e1", "t", "sess-1", "schedule", 1000)
            .unwrap();
        store.delete_task("t").unwrap();
        assert!(store.get_task("t").unwrap().is_none());
        assert_eq!(store.list_executions(Some("t"), None).unwrap().len(), 0);
    }

    #[test]
    fn skipped_execution_does_not_count_as_active() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        store
            .create_skipped_execution("e1", "t", "schedule", 1000)
            .unwrap();
        assert!(!store.has_active_execution("t").unwrap());
        let execs = store.list_executions(Some("t"), None).unwrap();
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].status, "skipped");
    }

    #[test]
    fn update_task_changes_fields() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();
        let new_input = TaskInput {
            name: "新名称".into(),
            prompt: "新提示词".into(),
            schedule_spec: "0 0 10 * * *".into(),
            schedule_display: Some("每天 10:00".into()),
            working_dir: None,
            project_id: None,
            agent_id: None,
            role_kind: None,
            role_id: None,
            permission_mode: Some("manual".into()),
            model_id: None,
        };
        store
            .update_task("t", &new_input, Some(9999), 2000)
            .unwrap();
        let got = store.get_task("t").unwrap().unwrap();
        assert_eq!(got.name, "新名称");
        assert_eq!(got.prompt, "新提示词");
        assert_eq!(got.schedule_spec, "0 0 10 * * *");
        assert_eq!(got.next_run_at, Some(9999));
    }

    /// try_begin_execution 是原子性 TOCTOU 守护：第一次调用声明成功（返回 true），
    /// 第二次在同一任务仍 running 时返回 false；完成后再次调用应返回 true。
    #[test]
    fn try_begin_execution_is_atomic_guard() {
        let store = temp_store();
        store
            .create_task("t", &sample_input(), 1000, Some(500))
            .unwrap();

        // 第一次声明：无 running → 插入并返回 true。
        let claimed = store
            .try_begin_execution("e1", "t", "sess-1", "schedule", 1000)
            .unwrap();
        assert!(claimed, "第一次声明应成功");
        assert!(
            store.has_active_execution("t").unwrap(),
            "声明后应有 running 执行"
        );

        // 第二次声明（模拟 run_task_now 与 tick 竞态）：已有 running → 返回 false，不插入。
        let claimed2 = store
            .try_begin_execution("e2", "t", "sess-2", "manual", 1001)
            .unwrap();
        assert!(!claimed2, "已有 running 时第二次声明应失败");
        // e2 不应被插入。
        let execs = store.list_executions(Some("t"), None).unwrap();
        assert_eq!(execs.len(), 1, "竞态失败时不应写入新执行记录");

        // 完成第一次执行后，再次声明应成功。
        store
            .finish_execution("e1", "completed", None, 1300)
            .unwrap();
        assert!(!store.has_active_execution("t").unwrap());
        let claimed3 = store
            .try_begin_execution("e3", "t", "sess-3", "schedule", 1400)
            .unwrap();
        assert!(claimed3, "执行完成后下一次声明应成功");
    }
}
