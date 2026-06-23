use std::sync::Arc;

use rusqlite::{params, OptionalExtension};

use crate::call_log::{CallLogDetail, CallLogFilter, CallLogRecord, CallLogRow, CallLogStats};
use crate::storage::AppDatabase;

/// 单条 request_json 超过此字节数即截断（保留前缀）。
const DEFAULT_MAX_PAYLOAD_BYTES: usize = 256 * 1024;
/// 表行数上限：插入后超出则淘汰最旧。
const DEFAULT_MAX_ROWS: i64 = 5000;

/// model_call_log 表的 owner：采集与查询。
pub struct CallLogStore {
    db: Arc<AppDatabase>,
    max_payload_bytes: usize,
    max_rows: i64,
}

impl CallLogStore {
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self {
            db,
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_rows: DEFAULT_MAX_ROWS,
        };
        store.ensure_schema()?;
        Ok(store)
    }

    /// 测试/配置注入护栏阈值。
    #[cfg(test)]
    pub fn with_limits(mut self, max_payload_bytes: usize, max_rows: i64) -> Self {
        self.max_payload_bytes = max_payload_bytes;
        self.max_rows = max_rows;
        self
    }

    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "
                    create table if not exists model_call_log (
                        id text primary key,
                        created_at text not null,
                        session_id text,
                        message_id text,
                        parent_session_id text,
                        parent_tool_call_id text,
                        expert_name text,
                        usage_type text not null default 'other',
                        provider text not null,
                        model text not null,
                        request_json text not null,
                        response_text text,
                        response_tool_calls_json text,
                        reasoning_text text,
                        finish_reason text,
                        input_tokens integer not null default 0,
                        output_tokens integer not null default 0,
                        cache_read_tokens integer not null default 0,
                        cache_create_tokens integer not null default 0,
                        latency_ms integer not null default 0,
                        status text not null default 'ok',
                        error_message text,
                        error_class text,
                        http_status integer,
                        request_bytes integer not null default 0,
                        truncated integer not null default 0
                    );
                    create index if not exists idx_call_log_created on model_call_log(created_at);
                    create index if not exists idx_call_log_session on model_call_log(session_id, created_at);
                    create index if not exists idx_call_log_model on model_call_log(model);
                    create index if not exists idx_call_log_status on model_call_log(status);
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 写一行调用日志，并在超出行数上限时淘汰最旧。request_json 超阈值截断。
    pub fn record(&self, id: &str, rec: &CallLogRecord) -> Result<(), String> {
        let raw_bytes = rec.request_json.len() as i64;
        let (request_json, truncated) = if rec.request_json.len() > self.max_payload_bytes {
            // 按字节安全截断到 char 边界。
            let mut end = self.max_payload_bytes;
            while end > 0 && !rec.request_json.is_char_boundary(end) {
                end -= 1;
            }
            (format!("{}…[truncated]", &rec.request_json[..end]), 1i64)
        } else {
            (rec.request_json.clone(), 0i64)
        };
        let max_rows = self.max_rows;
        self.db
            .with_transaction(|tx| {
                tx.execute(
                    "insert into model_call_log
                        (id, created_at, session_id, message_id, parent_session_id,
                         parent_tool_call_id, expert_name, usage_type, provider, model,
                         request_json, response_text, response_tool_calls_json, reasoning_text,
                         finish_reason, input_tokens, output_tokens, cache_read_tokens,
                         cache_create_tokens, latency_ms, status, error_message, error_class,
                         http_status, request_bytes, truncated)
                     values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                             ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        id,
                        rec.created_at,
                        rec.session_id,
                        rec.message_id,
                        rec.parent_session_id,
                        rec.parent_tool_call_id,
                        rec.expert_name,
                        rec.usage_type,
                        rec.provider,
                        rec.model,
                        request_json,
                        rec.response_text,
                        rec.response_tool_calls_json,
                        rec.reasoning_text,
                        rec.finish_reason,
                        rec.input_tokens as i64,
                        rec.output_tokens as i64,
                        rec.cache_read_tokens as i64,
                        rec.cache_create_tokens as i64,
                        rec.latency_ms as i64,
                        rec.status,
                        rec.error_message,
                        rec.error_class,
                        rec.http_status.map(|s| s as i64),
                        raw_bytes,
                        truncated,
                    ],
                )?;
                // 淘汰最旧：超出上限的多余行按 created_at 升序删除。
                tx.execute(
                    "delete from model_call_log where id in (
                        select id from model_call_log
                        order by cast(created_at as integer) asc, id asc
                        limit max(0, (select count(*) from model_call_log) - ?)
                    )",
                    params![max_rows],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn list(&self, f: &CallLogFilter) -> Result<Vec<CallLogRow>, String> {
        let mut sql = String::from(
            "select id, created_at, session_id, usage_type, provider, model,
                    input_tokens, output_tokens, cache_read_tokens, cache_create_tokens,
                    latency_ms, status, truncated
             from model_call_log where 1=1",
        );
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        Self::push_filters(f, &mut sql, &mut args);
        sql.push_str(" order by cast(created_at as integer) desc, id desc limit ? offset ?");
        let limit = f.limit.unwrap_or(50).min(500) as i64;
        let offset = f.offset.unwrap_or(0) as i64;
        args.push(Box::new(limit));
        args.push(Box::new(offset));
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&sql)?;
                let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(params.as_slice(), |r| {
                        Ok(CallLogRow {
                            id: r.get(0)?,
                            created_at: r.get(1)?,
                            session_id: r.get(2)?,
                            usage_type: r.get(3)?,
                            provider: r.get(4)?,
                            model: r.get(5)?,
                            input_tokens: r.get::<_, i64>(6)? as u64,
                            output_tokens: r.get::<_, i64>(7)? as u64,
                            cache_read_tokens: r.get::<_, i64>(8)? as u64,
                            cache_create_tokens: r.get::<_, i64>(9)? as u64,
                            latency_ms: r.get::<_, i64>(10)? as u64,
                            status: r.get(11)?,
                            truncated: r.get::<_, i64>(12)? != 0,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<CallLogDetail>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select id, created_at, session_id, message_id, parent_session_id,
                                parent_tool_call_id, expert_name, usage_type, provider, model,
                                request_json, response_text, response_tool_calls_json, reasoning_text,
                                finish_reason, input_tokens, output_tokens, cache_read_tokens,
                                cache_create_tokens, latency_ms, status, error_message, error_class,
                                http_status, request_bytes, truncated
                         from model_call_log where id = ?",
                        params![id],
                        |r| {
                            Ok(CallLogDetail {
                                id: r.get(0)?,
                                created_at: r.get(1)?,
                                session_id: r.get(2)?,
                                message_id: r.get(3)?,
                                parent_session_id: r.get(4)?,
                                parent_tool_call_id: r.get(5)?,
                                expert_name: r.get(6)?,
                                usage_type: r.get(7)?,
                                provider: r.get(8)?,
                                model: r.get(9)?,
                                request_json: r.get(10)?,
                                response_text: r.get(11)?,
                                response_tool_calls_json: r.get(12)?,
                                reasoning_text: r.get(13)?,
                                finish_reason: r.get(14)?,
                                input_tokens: r.get::<_, i64>(15)? as u64,
                                output_tokens: r.get::<_, i64>(16)? as u64,
                                cache_read_tokens: r.get::<_, i64>(17)? as u64,
                                cache_create_tokens: r.get::<_, i64>(18)? as u64,
                                latency_ms: r.get::<_, i64>(19)? as u64,
                                status: r.get(20)?,
                                error_message: r.get(21)?,
                                error_class: r.get(22)?,
                                http_status: r.get::<_, Option<i64>>(23)?.map(|s| s as u16),
                                request_bytes: r.get::<_, i64>(24)? as u64,
                                truncated: r.get::<_, i64>(25)? != 0,
                            })
                        },
                    )
                    .optional()?;
                Ok(row)
            })
            .map_err(|e| e.to_string())
    }

    pub fn clear(&self, f: &CallLogFilter) -> Result<usize, String> {
        let mut sql = String::from("delete from model_call_log where 1=1");
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        Self::push_filters(f, &mut sql, &mut args);
        self.db
            .with_connection(|c| {
                let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
                Ok(c.execute(&sql, params.as_slice())?)
            })
            .map_err(|e| e.to_string())
    }

    pub fn stats(&self) -> Result<CallLogStats, String> {
        self.db
            .with_connection(|c| {
                let (count, bytes) = c.query_row(
                    "select count(*),
                            coalesce(sum(length(request_json) + coalesce(length(response_text),0)
                                     + coalesce(length(response_tool_calls_json),0)
                                     + coalesce(length(reasoning_text),0)), 0)
                     from model_call_log",
                    [],
                    |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
                )?;
                Ok(CallLogStats { count, bytes })
            })
            .map_err(|e| e.to_string())
    }

    /// 把 CallLogFilter 的等值/区间/搜索条件追加进 SQL（匿名 `?` 占位符，按 args 顺序绑定）。
    fn push_filters(
        f: &CallLogFilter,
        sql: &mut String,
        args: &mut Vec<Box<dyn rusqlite::ToSql>>,
    ) {
        fn eq(
            col: &str,
            v: &Option<String>,
            sql: &mut String,
            args: &mut Vec<Box<dyn rusqlite::ToSql>>,
        ) {
            if let Some(val) = v {
                sql.push_str(&format!(" and {col} = ?"));
                args.push(Box::new(val.clone()));
            }
        }
        eq("session_id", &f.session_id, sql, args);
        eq("model", &f.model, sql, args);
        eq("provider", &f.provider, sql, args);
        eq("usage_type", &f.usage_type, sql, args);
        eq("status", &f.status, sql, args);
        if let Some(since) = f.since {
            sql.push_str(" and cast(created_at as integer) >= ?");
            args.push(Box::new(since));
        }
        if let Some(until) = f.until {
            sql.push_str(" and cast(created_at as integer) <= ?");
            args.push(Box::new(until));
        }
        if let Some(s) = &f.search {
            if !s.is_empty() {
                let like = format!("%{s}%");
                sql.push_str(" and (request_json like ? or response_text like ?)");
                args.push(Box::new(like.clone()));
                args.push(Box::new(like));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AppDatabase;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// 唯一临时目录：nanos + 进程内自增计数，避免并行测试同纳秒撞库。
    fn unique_db() -> Arc<AppDatabase> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("sw-calllog-{nanos}-{seq}"));
        std::fs::create_dir_all(&dir).unwrap();
        Arc::new(AppDatabase::open(dir.join("c.sqlite3")).unwrap())
    }

    fn mem_store() -> CallLogStore {
        CallLogStore::open(unique_db()).unwrap()
    }

    fn limited_store(max_payload_bytes: usize, max_rows: i64) -> CallLogStore {
        CallLogStore::open(unique_db())
            .unwrap()
            .with_limits(max_payload_bytes, max_rows)
    }

    fn rec(created_at: &str, model: &str, status: &str) -> CallLogRecord {
        CallLogRecord {
            created_at: created_at.to_string(),
            session_id: Some("s1".into()),
            message_id: None,
            parent_session_id: None,
            parent_tool_call_id: None,
            expert_name: None,
            usage_type: "main_agent".into(),
            provider: "openai".into(),
            model: model.into(),
            request_json: "{\"messages\":[]}".into(),
            response_text: Some("hi".into()),
            response_tool_calls_json: None,
            reasoning_text: None,
            finish_reason: Some("stop".into()),
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 2,
            cache_create_tokens: 0,
            latency_ms: 123,
            status: status.into(),
            error_message: None,
            error_class: None,
            http_status: None,
        }
    }

    #[test]
    fn record_then_list_and_get() {
        let store = mem_store();
        store.record("c1", &rec("1000", "gpt-x", "ok")).unwrap();
        let rows = store.list(&CallLogFilter::default()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "gpt-x");
        assert_eq!(rows[0].latency_ms, 123);
        let detail = store.get("c1").unwrap().unwrap();
        assert_eq!(detail.response_text.as_deref(), Some("hi"));
        assert_eq!(detail.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn filters_by_model_and_status() {
        let store = mem_store();
        store.record("c1", &rec("1000", "a", "ok")).unwrap();
        store.record("c2", &rec("1001", "b", "error")).unwrap();
        let f = CallLogFilter {
            model: Some("b".into()),
            ..Default::default()
        };
        let rows = store.list(&f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "c2");
        let f = CallLogFilter {
            status: Some("error".into()),
            ..Default::default()
        };
        assert_eq!(store.list(&f).unwrap().len(), 1);
    }

    #[test]
    fn search_matches_request_or_response() {
        let store = mem_store();
        let mut r = rec("1000", "a", "ok");
        r.response_text = Some("needle here".into());
        store.record("c1", &r).unwrap();
        store.record("c2", &rec("1001", "b", "ok")).unwrap();
        let f = CallLogFilter {
            search: Some("needle".into()),
            ..Default::default()
        };
        let rows = store.list(&f).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "c1");
    }

    #[test]
    fn truncates_oversized_payload() {
        let store = limited_store(16, 5000);
        let mut r = rec("1000", "a", "ok");
        r.request_json = "x".repeat(1000);
        store.record("c1", &r).unwrap();
        let d = store.get("c1").unwrap().unwrap();
        assert!(d.truncated);
        assert_eq!(d.request_bytes, 1000);
        assert!(d.request_json.len() < 1000);
    }

    #[test]
    fn evicts_oldest_beyond_cap() {
        let store = limited_store(usize::MAX, 2);
        store.record("c1", &rec("1000", "a", "ok")).unwrap();
        store.record("c2", &rec("1001", "a", "ok")).unwrap();
        store.record("c3", &rec("1002", "a", "ok")).unwrap();
        let rows = store.list(&CallLogFilter::default()).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(store.get("c1").unwrap().is_none()); // 最旧被淘汰
    }

    #[test]
    fn clear_and_stats() {
        let store = mem_store();
        store.record("c1", &rec("1000", "a", "ok")).unwrap();
        store.record("c2", &rec("1001", "b", "ok")).unwrap();
        assert_eq!(store.stats().unwrap().count, 2);
        let deleted = store
            .clear(&CallLogFilter {
                model: Some("a".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.stats().unwrap().count, 1);
    }
}
