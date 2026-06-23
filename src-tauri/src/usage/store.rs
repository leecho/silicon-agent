use std::sync::Arc;

use rusqlite::{params, OptionalExtension};

use crate::storage::AppDatabase;
use crate::usage::UsageRecord;

/// 会话→归属智能体映射的递归 CTE（不含前导 `with`）。
/// 产出关系 `session_owner(session_id, agent_id)`：会话自身或其最近祖先
/// （沿 parent_session_id）中的 agent_id。
const SESSION_OWNER_CTE: &str = "
recursive owner_walk(origin, node, agent_id, depth) as (
    select id, id, agent_id, 0 from sessions
  union all
    select w.origin, p.id, p.agent_id, w.depth + 1
    from owner_walk w
    join sessions cur on cur.id = w.node
    join sessions p on p.id = cur.parent_session_id
    where cur.parent_session_id is not null
),
session_owner(session_id, agent_id) as (
    select origin, agent_id from (
        select origin, agent_id,
               row_number() over (partition by origin order by depth) as rn
        from owner_walk
        where agent_id is not null and agent_id <> ''
    ) where rn = 1
)
";

/// token_usage 表的 owner：采集与聚合。
pub struct UsageStore {
    db: Arc<AppDatabase>,
}

impl UsageStore {
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
                    create table if not exists token_usage (
                        id text primary key,
                        session_id text not null,
                        message_id text,
                        provider text not null,
                        model text not null,
                        usage_type text not null default 'main_agent',
                        input_tokens integer not null default 0,
                        output_tokens integer not null default 0,
                        cache_read_tokens integer not null default 0,
                        cache_create_tokens integer not null default 0,
                        -- created_at 存 Unix epoch 秒的字符串（与全库一致），范围查询用 cast(... as integer)
                        created_at text not null
                    );
                    create index if not exists idx_token_usage_created on token_usage(created_at);
                    create index if not exists idx_token_usage_session on token_usage(session_id, created_at);
                    create index if not exists idx_token_usage_model on token_usage(model);
                    ",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 聚合 range 内用量。`now_epoch` 由调用方注入（命令层取当前时间），便于测试。
    pub fn analytics(
        &self,
        range: &str,
        now_epoch: i64,
    ) -> Result<crate::usage::UsageAnalyticsView, String> {
        let cutoff = crate::usage::resolve_cutoff(range, now_epoch);
        self.db
            .with_connection(|c| {
                let where_cut = "cast(created_at as integer) >= ?1";

                let totals = c.query_row(
                    &format!(
                        "select
                            coalesce(sum(input_tokens),0),
                            coalesce(sum(output_tokens),0),
                            coalesce(sum(cache_read_tokens),0),
                            coalesce(sum(cache_create_tokens),0),
                            count(*)
                         from token_usage where {where_cut}"
                    ),
                    params![cutoff],
                    |r| {
                        let input: i64 = r.get(0)?;
                        let output: i64 = r.get(1)?;
                        let cread: i64 = r.get(2)?;
                        let ccreate: i64 = r.get(3)?;
                        let calls: i64 = r.get(4)?;
                        Ok(crate::usage::UsageTotals {
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    },
                )?;

                let mut by_date = Vec::new();
                {
                    let mut stmt = c.prepare(&format!(
                        "select date(cast(created_at as integer),'unixepoch','localtime') as d,
                                coalesce(sum(input_tokens),0), coalesce(sum(output_tokens),0),
                                coalesce(sum(cache_read_tokens),0), coalesce(sum(cache_create_tokens),0),
                                count(*)
                         from token_usage where {where_cut} group by d order by d"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(1)?;
                        let output: i64 = r.get(2)?;
                        let cread: i64 = r.get(3)?;
                        let ccreate: i64 = r.get(4)?;
                        let calls: i64 = r.get(5)?;
                        Ok(crate::usage::UsageDateBucket {
                            date: r.get(0)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    })?;
                    for row in rows { by_date.push(row?); }
                }

                let mut by_model = Vec::new();
                {
                    let mut stmt = c.prepare(&format!(
                        "select provider, model,
                                coalesce(sum(input_tokens),0), coalesce(sum(output_tokens),0),
                                coalesce(sum(cache_read_tokens),0), coalesce(sum(cache_create_tokens),0),
                                count(*)
                         from token_usage where {where_cut}
                         group by provider, model
                         order by (sum(input_tokens)+sum(output_tokens)+sum(cache_read_tokens)+sum(cache_create_tokens)) desc"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(2)?;
                        let output: i64 = r.get(3)?;
                        let cread: i64 = r.get(4)?;
                        let ccreate: i64 = r.get(5)?;
                        let calls: i64 = r.get(6)?;
                        Ok(crate::usage::UsageModelRow {
                            provider: r.get(0)?,
                            model: r.get(1)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    })?;
                    for row in rows { by_model.push(row?); }
                }

                let mut by_session = Vec::new();
                {
                    let mut stmt = c.prepare(&format!(
                        "select u.session_id, coalesce(s.title,''),
                                coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                                coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                                count(*)
                         from token_usage u left join sessions s on s.id = u.session_id
                         where cast(u.created_at as integer) >= ?1
                         group by u.session_id, s.title
                         order by (sum(u.input_tokens)+sum(u.output_tokens)+sum(u.cache_read_tokens)+sum(u.cache_create_tokens)) desc"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(2)?;
                        let output: i64 = r.get(3)?;
                        let cread: i64 = r.get(4)?;
                        let ccreate: i64 = r.get(5)?;
                        let calls: i64 = r.get(6)?;
                        Ok(crate::usage::UsageSessionRow {
                            session_id: r.get(0)?,
                            title: r.get(1)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    })?;
                    for row in rows { by_session.push(row?); }
                }

                // 关联表可能因调用方未初始化对应 store 而缺失（如部分集成测试）；
                // 缺表时这两个维度返回空，不影响其余聚合。
                let has_table = |name: &str| -> rusqlite::Result<bool> {
                    c.query_row(
                        "select 1 from sqlite_master where type='table' and name=?1",
                        params![name],
                        |_| Ok(()),
                    )
                    .optional()
                    .map(|o| o.is_some())
                };
                let has_sessions = has_table("sessions")?;

                let mut by_project = Vec::new();
                if has_sessions && has_table("projects")? {
                    let mut stmt = c.prepare(
                        "select s.project_id, coalesce(p.name,''),
                                coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                                coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                                count(*)
                         from token_usage u
                         join sessions s on s.id = u.session_id
                         left join projects p on p.id = s.project_id
                         where s.project_id is not null and s.project_id <> ''
                           and cast(u.created_at as integer) >= ?1
                         group by s.project_id, p.name
                         order by (sum(u.input_tokens)+sum(u.output_tokens)+sum(u.cache_read_tokens)+sum(u.cache_create_tokens)) desc",
                    )?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(2)?;
                        let output: i64 = r.get(3)?;
                        let cread: i64 = r.get(4)?;
                        let ccreate: i64 = r.get(5)?;
                        let calls: i64 = r.get(6)?;
                        Ok(crate::usage::UsageProjectRow {
                            project_id: r.get(0)?,
                            name: r.get(1)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    })?;
                    for row in rows { by_project.push(row?); }
                }

                let mut by_agent = Vec::new();
                if has_sessions && has_table("agents")? {
                    let mut stmt = c.prepare(&format!(
                        "with {SESSION_OWNER_CTE}
                         select o.agent_id, coalesce(nullif(a.display_name,''), a.name, ''),
                                coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                                coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                                count(*)
                         from token_usage u
                         join session_owner o on o.session_id = u.session_id
                         left join agents a on a.id = o.agent_id
                         where cast(u.created_at as integer) >= ?1
                         group by o.agent_id, a.display_name, a.name
                         order by (sum(u.input_tokens)+sum(u.output_tokens)+sum(u.cache_read_tokens)+sum(u.cache_create_tokens)) desc"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(2)?;
                        let output: i64 = r.get(3)?;
                        let cread: i64 = r.get(4)?;
                        let ccreate: i64 = r.get(5)?;
                        let calls: i64 = r.get(6)?;
                        Ok(crate::usage::UsageAgentRow {
                            agent_id: r.get(0)?,
                            name: r.get(1)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                            calls: calls as u64,
                        })
                    })?;
                    for row in rows { by_agent.push(row?); }
                }

                let mut hour_map: std::collections::HashMap<u8, (u64, u64)> = std::collections::HashMap::new();
                {
                    let mut stmt = c.prepare(&format!(
                        "select cast(strftime('%H', cast(created_at as integer),'unixepoch','localtime') as integer) as h,
                                coalesce(sum(input_tokens)+sum(output_tokens)+sum(cache_read_tokens)+sum(cache_create_tokens),0),
                                count(*)
                         from token_usage where {where_cut} group by h"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let h: i64 = r.get(0)?;
                        let total: i64 = r.get(1)?;
                        let calls: i64 = r.get(2)?;
                        Ok((h as u8, total as u64, calls as u64))
                    })?;
                    for row in rows { let (h, t, c2) = row?; hour_map.insert(h, (t, c2)); }
                }
                let by_hour: Vec<crate::usage::UsageHourBucket> = (0u8..24)
                    .map(|h| {
                        let (total, calls) = hour_map.get(&h).copied().unwrap_or((0, 0));
                        crate::usage::UsageHourBucket { hour: h, total, calls }
                    })
                    .collect();

                let mut by_date_model = Vec::new();
                {
                    let mut stmt = c.prepare(&format!(
                        "select date(cast(created_at as integer),'unixepoch','localtime') as d, model,
                                coalesce(sum(input_tokens)+sum(output_tokens)+sum(cache_read_tokens)+sum(cache_create_tokens),0)
                         from token_usage where {where_cut} group by d, model order by d"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let total: i64 = r.get(2)?;
                        Ok(crate::usage::UsageDateModel { date: r.get(0)?, model: r.get(1)?, total: total as u64 })
                    })?;
                    for row in rows { by_date_model.push(row?); }
                }

                let read_calls = |only_cache: bool, conn: &rusqlite::Connection| -> Result<Vec<crate::usage::UsageCallRow>, rusqlite::Error> {
                    let filter = if only_cache {
                        " and (cache_read_tokens > 0 or cache_create_tokens > 0)"
                    } else {
                        ""
                    };
                    let mut stmt = conn.prepare(&format!(
                        "select created_at, provider, model, input_tokens, output_tokens, cache_read_tokens, cache_create_tokens
                         from token_usage where cast(created_at as integer) >= ?1{filter}
                         order by cast(created_at as integer) desc, id desc limit 20"
                    ))?;
                    let rows = stmt.query_map(params![cutoff], |r| {
                        let input: i64 = r.get(3)?;
                        let output: i64 = r.get(4)?;
                        let cread: i64 = r.get(5)?;
                        let ccreate: i64 = r.get(6)?;
                        Ok(crate::usage::UsageCallRow {
                            ts: r.get(0)?,
                            provider: r.get(1)?,
                            model: r.get(2)?,
                            input: input as u64,
                            output: output as u64,
                            cache_read: cread as u64,
                            cache_create: ccreate as u64,
                            total: (input + output + cread + ccreate) as u64,
                        })
                    })?;
                    let mut out = Vec::new();
                    for row in rows { out.push(row?); }
                    Ok(out)
                };
                let recent_calls = read_calls(false, c)?;
                let recent_cache_calls = read_calls(true, c)?;

                let sessions: i64 = c.query_row(
                    &format!("select count(distinct session_id) from token_usage where {where_cut}"),
                    params![cutoff],
                    |r| r.get(0),
                )?;
                let messages: i64 = c.query_row(
                    &format!("select count(distinct message_id) from token_usage where {where_cut} and message_id is not null"),
                    params![cutoff],
                    |r| r.get(0),
                )?;

                Ok(crate::usage::UsageAnalyticsView {
                    totals,
                    by_date,
                    by_model,
                    by_session,
                    by_project,
                    by_agent,
                    by_hour,
                    by_date_model,
                    recent_calls,
                    recent_cache_calls,
                    sessions: sessions as u64,
                    messages: messages as u64,
                    generated_at: now_epoch.to_string(),
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 写入一次调用的用量。非缓存输入 = max(0, prompt_tokens - cache_read)，
    /// 与展示侧 total = input + cache_read + cache_create + output 自洽。
    pub fn record(&self, id: &str, record: &UsageRecord) -> Result<(), String> {
        let prompt = record.usage.input_tokens.unwrap_or(0);
        let cache_read = record.usage.cache_read_tokens.unwrap_or(0);
        let cache_create = record.usage.cache_create_tokens.unwrap_or(0);
        let output = record.usage.output_tokens.unwrap_or(0);
        let input = prompt.saturating_sub(cache_read);
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into token_usage
                        (id, session_id, message_id, provider, model, usage_type,
                         input_tokens, output_tokens, cache_read_tokens, cache_create_tokens, created_at)
                     values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        id,
                        record.session_id,
                        record.message_id,
                        record.provider,
                        record.model,
                        record.usage_type,
                        input as i64,
                        output as i64,
                        cache_read as i64,
                        cache_create as i64,
                        record.created_at,
                    ],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 单会话累计用量（该会话所有调用求和）。无记录返回全 0。
    pub fn session_totals(&self, session_id: &str) -> Result<crate::usage::UsageTotals, String> {
        self.db
            .with_connection(|c| {
                let t = c.query_row(
                    "select
                        coalesce(sum(input_tokens),0),
                        coalesce(sum(output_tokens),0),
                        coalesce(sum(cache_read_tokens),0),
                        coalesce(sum(cache_create_tokens),0),
                        count(*)
                     from token_usage where session_id = ?1",
                    params![session_id],
                    Self::map_totals,
                )?;
                Ok(t)
            })
            .map_err(|e| e.to_string())
    }

    /// 项目用量：按 sessions.project_id 归属（子会话继承 project_id，天然覆盖）。
    pub fn project_usage(
        &self,
        project_id: &str,
        range: &str,
        now_epoch: i64,
    ) -> Result<crate::usage::ScopedUsageView, String> {
        let cutoff = crate::usage::resolve_cutoff(range, now_epoch);
        self.db
            .with_connection(|c| {
                let totals = c.query_row(
                    "select
                        coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                        coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                        count(*)
                     from token_usage u join sessions s on s.id = u.session_id
                     where s.project_id = ?1 and cast(u.created_at as integer) >= ?2",
                    params![project_id, cutoff],
                    Self::map_totals,
                )?;

                let mut by_session = Vec::new();
                let mut stmt = c.prepare(
                    "select u.session_id, coalesce(s.title,''),
                            coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                            coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                            count(*)
                     from token_usage u join sessions s on s.id = u.session_id
                     where s.project_id = ?1 and cast(u.created_at as integer) >= ?2
                     group by u.session_id, s.title
                     order by (sum(u.input_tokens)+sum(u.output_tokens)+sum(u.cache_read_tokens)+sum(u.cache_create_tokens)) desc",
                )?;
                let rows = stmt.query_map(params![project_id, cutoff], Self::map_session_row)?;
                for row in rows { by_session.push(row?); }

                Ok(crate::usage::ScopedUsageView { totals, by_session })
            })
            .map_err(|e| e.to_string())
    }

    /// 智能体用量：递归把会话上卷到「最近活动角色智能体」后，按该 agent 聚合。
    pub fn agent_usage(
        &self,
        agent_id: &str,
        range: &str,
        now_epoch: i64,
    ) -> Result<crate::usage::ScopedUsageView, String> {
        let cutoff = crate::usage::resolve_cutoff(range, now_epoch);
        self.db
            .with_connection(|c| {
                let totals = c.query_row(
                    &format!(
                        "with {SESSION_OWNER_CTE}
                         select
                            coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                            coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                            count(*)
                         from token_usage u
                         join session_owner o on o.session_id = u.session_id
                         where o.agent_id = ?1 and cast(u.created_at as integer) >= ?2"
                    ),
                    params![agent_id, cutoff],
                    Self::map_totals,
                )?;

                let mut by_session = Vec::new();
                let mut stmt = c.prepare(&format!(
                    "with {SESSION_OWNER_CTE}
                     select u.session_id, coalesce(s.title,''),
                            coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                            coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0),
                            count(*)
                     from token_usage u
                     join session_owner o on o.session_id = u.session_id
                     left join sessions s on s.id = u.session_id
                     where o.agent_id = ?1 and cast(u.created_at as integer) >= ?2
                     group by u.session_id, s.title
                     order by (sum(u.input_tokens)+sum(u.output_tokens)+sum(u.cache_read_tokens)+sum(u.cache_create_tokens)) desc"
                ))?;
                let rows = stmt.query_map(params![agent_id, cutoff], Self::map_session_row)?;
                for row in rows { by_session.push(row?); }

                Ok(crate::usage::ScopedUsageView { totals, by_session })
            })
            .map_err(|e| e.to_string())
    }

    /// 单会话的按消息用量（一次 assistant 回合一行），供会话→消息二层展开。
    /// 仅含带 message_id 的用量行；left join messages 取内容摘要与时间。按时间升序。
    pub fn session_message_usage(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::usage::UsageMessageRow>, String> {
        self.db
            .with_connection(|c| {
                // messages 表可能未初始化（如部分测试场景）；缺表时摘要/角色/时间留空。
                let has_messages = c
                    .query_row(
                        "select 1 from sqlite_master where type='table' and name='messages'",
                        [],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                let join = if has_messages {
                    "left join messages m on m.id = u.message_id"
                } else {
                    ""
                };
                let content = if has_messages { "coalesce(substr(m.content,1,140),'')" } else { "''" };
                let role = if has_messages { "coalesce(m.role,'')" } else { "''" };
                let ts = if has_messages {
                    "coalesce(m.created_at, min(u.created_at))"
                } else {
                    "min(u.created_at)"
                };
                let sql = format!(
                    "select u.message_id, {content}, {role}, {ts},
                            coalesce(sum(u.input_tokens),0), coalesce(sum(u.output_tokens),0),
                            coalesce(sum(u.cache_read_tokens),0), coalesce(sum(u.cache_create_tokens),0)
                     from token_usage u {join}
                     where u.session_id = ?1 and u.message_id is not null and u.message_id <> ''
                     group by u.message_id
                     order by cast({ts} as integer) asc, u.message_id"
                );
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt.query_map(params![session_id], Self::map_message_row)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 5 列汇总 (input,output,cache_read,cache_create,calls) → UsageTotals。
    fn map_totals(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::usage::UsageTotals> {
        let input: i64 = r.get(0)?;
        let output: i64 = r.get(1)?;
        let cread: i64 = r.get(2)?;
        let ccreate: i64 = r.get(3)?;
        let calls: i64 = r.get(4)?;
        Ok(crate::usage::UsageTotals {
            input: input as u64,
            output: output as u64,
            cache_read: cread as u64,
            cache_create: ccreate as u64,
            total: (input + output + cread + ccreate) as u64,
            calls: calls as u64,
        })
    }

    /// 7 列 (session_id,title,input,output,cache_read,cache_create,calls) → UsageSessionRow。
    fn map_session_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::usage::UsageSessionRow> {
        let input: i64 = r.get(2)?;
        let output: i64 = r.get(3)?;
        let cread: i64 = r.get(4)?;
        let ccreate: i64 = r.get(5)?;
        let calls: i64 = r.get(6)?;
        Ok(crate::usage::UsageSessionRow {
            session_id: r.get(0)?,
            title: r.get(1)?,
            input: input as u64,
            output: output as u64,
            cache_read: cread as u64,
            cache_create: ccreate as u64,
            total: (input + output + cread + ccreate) as u64,
            calls: calls as u64,
        })
    }

    /// 8 列 (message_id,snippet,role,ts,input,output,cache_read,cache_create) → UsageMessageRow。
    fn map_message_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<crate::usage::UsageMessageRow> {
        let input: i64 = r.get(4)?;
        let output: i64 = r.get(5)?;
        let cread: i64 = r.get(6)?;
        let ccreate: i64 = r.get(7)?;
        Ok(crate::usage::UsageMessageRow {
            message_id: r.get(0)?,
            snippet: r.get(1)?,
            role: r.get(2)?,
            ts: r.get(3)?,
            input: input as u64,
            output: output as u64,
            cache_read: cread as u64,
            cache_create: ccreate as u64,
            total: (input + output + cread + ccreate) as u64,
        })
    }

    /// 仅供单元测试：在内部连接上执行任意写入（构造 sessions/agents 等关联表）。
    #[cfg(test)]
    pub(crate) fn raw_for_test<F>(&self, f: F)
    where
        F: FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>,
    {
        self.db
            .with_connection(|c| {
                f(c)?;
                Ok(())
            })
            .unwrap();
    }

    /// 取该会话最近一次主体调用（usage_type='main_agent'）的用量，供 context meter 用。
    /// 返回 `(model, used_tokens)`，used = input + output + cache_read + cache_create
    /// （即下一轮预计要发送的上下文大小近似）。无记录返回 None。
    pub fn latest_session_usage(&self, session_id: &str) -> Result<Option<(String, u64)>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select model,
                                input_tokens + output_tokens + cache_read_tokens + cache_create_tokens
                         from token_usage
                         where session_id = ?1 and usage_type = 'main_agent'
                         order by cast(created_at as integer) desc, rowid desc
                         limit 1",
                        params![session_id],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                    )
                    .optional()?;
                Ok(row.map(|(m, u)| (m, u.max(0) as u64)))
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod latest_usage_tests {
    use super::UsageStore;
    use crate::provider::client::ModelUsage;
    use crate::storage::AppDatabase;
    use crate::usage::UsageRecord;
    use std::sync::Arc;

    fn temp_store() -> UsageStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sw-usage-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("u.sqlite3")).unwrap());
        UsageStore::open(db).unwrap()
    }

    fn rec(session: &str, model: &str, prompt: u64, output: u64, created_at: &str) -> UsageRecord {
        UsageRecord {
            session_id: session.to_string(),
            message_id: None,
            provider: "p".to_string(),
            model: model.to_string(),
            usage_type: "main_agent".to_string(),
            created_at: created_at.to_string(),
            usage: ModelUsage {
                input_tokens: Some(prompt),
                output_tokens: Some(output),
                cache_read_tokens: None,
                cache_create_tokens: None,
            },
        }
    }

    #[test]
    fn returns_newest_main_agent_row_per_session() {
        let store = temp_store();
        store
            .record("u1", &rec("s1", "claude", 1000, 200, "100"))
            .unwrap();
        store
            .record("u2", &rec("s1", "claude", 3000, 500, "200"))
            .unwrap();
        store
            .record("u3", &rec("s2", "qwen", 999, 1, "300"))
            .unwrap();

        let (model, used) = store.latest_session_usage("s1").unwrap().unwrap();
        assert_eq!(model, "claude");
        assert_eq!(used, 3500); // 最近一行 prompt 3000 + output 500

        assert!(store.latest_session_usage("missing").unwrap().is_none());
    }
}

#[cfg(test)]
mod session_totals_tests {
    use super::UsageStore;
    use crate::provider::client::ModelUsage;
    use crate::storage::AppDatabase;
    use crate::usage::UsageRecord;
    use std::sync::Arc;

    fn temp_store() -> UsageStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sw-usage-st-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("u.sqlite3")).unwrap());
        UsageStore::open(db).unwrap()
    }

    fn rec(session: &str, prompt: u64, output: u64, created_at: &str) -> UsageRecord {
        UsageRecord {
            session_id: session.to_string(),
            message_id: None,
            provider: "p".to_string(),
            model: "m".to_string(),
            usage_type: "main_agent".to_string(),
            created_at: created_at.to_string(),
            usage: ModelUsage {
                input_tokens: Some(prompt),
                output_tokens: Some(output),
                cache_read_tokens: None,
                cache_create_tokens: None,
            },
        }
    }

    #[test]
    fn sums_all_calls_for_one_session() {
        let store = temp_store();
        store.record("u1", &rec("s1", 1000, 200, "100")).unwrap();
        store.record("u2", &rec("s1", 3000, 500, "200")).unwrap();
        store.record("u3", &rec("s2", 999, 1, "300")).unwrap();

        let t = store.session_totals("s1").unwrap();
        assert_eq!(t.input, 4000); // 1000 + 3000（无 cache，input=prompt）
        assert_eq!(t.output, 700); // 200 + 500
        assert_eq!(t.total, 4700);
        assert_eq!(t.calls, 2);

        let empty = store.session_totals("missing").unwrap();
        assert_eq!(empty.total, 0);
        assert_eq!(empty.calls, 0);
    }
}

#[cfg(test)]
mod scoped_usage_tests {
    use super::UsageStore;
    use crate::provider::client::ModelUsage;
    use crate::storage::AppDatabase;
    use crate::usage::UsageRecord;
    use std::sync::Arc;

    fn store_with_session_tables() -> UsageStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sw-usage-sc-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("u.sqlite3")).unwrap());
        // 最小化建表：只含查询引用到的列。
        db.with_connection(|c| {
            c.execute_batch(
                "create table sessions (
                    id text primary key,
                    title text,
                    project_id text,
                    agent_id text,
                    parent_session_id text
                 );
                 create table agents (id text primary key, name text, display_name text);
                 create table projects (id text primary key, name text);",
            )?;
            Ok(())
        })
        .unwrap();
        UsageStore::open(db).unwrap()
    }

    fn mk_session(
        store: &UsageStore,
        id: &str,
        title: &str,
        project_id: Option<&str>,
        agent_id: Option<&str>,
        parent: Option<&str>,
    ) {
        store.raw_for_test(|c| {
            c.execute(
                "insert into sessions (id, title, project_id, agent_id, parent_session_id)
                 values (?1,?2,?3,?4,?5)",
                rusqlite::params![id, title, project_id, agent_id, parent],
            )?;
            Ok(())
        });
    }

    fn rec(session: &str, prompt: u64, output: u64, ts: &str) -> UsageRecord {
        UsageRecord {
            session_id: session.to_string(),
            message_id: None,
            provider: "p".to_string(),
            model: "m".to_string(),
            usage_type: "main_agent".to_string(),
            created_at: ts.to_string(),
            usage: ModelUsage {
                input_tokens: Some(prompt),
                output_tokens: Some(output),
                cache_read_tokens: None,
                cache_create_tokens: None,
            },
        }
    }

    #[test]
    fn project_and_agent_attribution() {
        let store = store_with_session_tables();
        store.raw_for_test(|c| {
            c.execute("insert into projects (id,name) values ('P1','项目一')", [])?;
            c.execute(
                "insert into agents (id,name,display_name) values ('A1','alice','Alice')",
                [],
            )?;
            Ok(())
        });

        // 主会话：属于 P1，所属智能体 = A1
        mk_session(&store, "root", "主会话", Some("P1"), Some("A1"), None);
        // 子会话：dispatch 给别的专家，父为 root，继承 project P1 / agent A1
        mk_session(&store, "child", "子会话", Some("P1"), None, Some("root"));
        // 无项目、无角色的散会话
        mk_session(&store, "loose", "散会话", None, None, None);

        store.record("u1", &rec("root", 1000, 200, "100")).unwrap();
        store.record("u2", &rec("child", 500, 100, "200")).unwrap();
        store.record("u3", &rec("loose", 9, 1, "300")).unwrap();

        // 项目用量：root + child = (1500 in, 300 out)
        let proj = store.project_usage("P1", "all", 1_000).unwrap();
        assert_eq!(proj.totals.input, 1500);
        assert_eq!(proj.totals.output, 300);
        assert_eq!(proj.totals.total, 1800);
        assert_eq!(proj.totals.calls, 2);
        assert_eq!(proj.by_session.len(), 2);

        // 智能体用量：root（直接）+ child（递归上卷到 A1）= 1800；loose 不计
        let agent = store.agent_usage("A1", "all", 1_000).unwrap();
        assert_eq!(agent.totals.total, 1800);
        assert_eq!(agent.totals.calls, 2);
        let ids: Vec<&str> = agent
            .by_session
            .iter()
            .map(|r| r.session_id.as_str())
            .collect();
        assert!(ids.contains(&"root") && ids.contains(&"child"));
        assert!(!ids.contains(&"loose"));
    }

    #[test]
    fn analytics_includes_project_and_agent_dimensions() {
        let store = store_with_session_tables();
        store.raw_for_test(|c| {
            c.execute("insert into projects (id,name) values ('P1','项目一')", [])?;
            c.execute(
                "insert into agents (id,name,display_name) values ('A1','alice','Alice')",
                [],
            )?;
            Ok(())
        });
        mk_session(&store, "root", "主会话", Some("P1"), Some("A1"), None);
        mk_session(&store, "child", "子会话", Some("P1"), None, Some("root"));
        store.record("u1", &rec("root", 1000, 200, "100")).unwrap();
        store.record("u2", &rec("child", 500, 100, "200")).unwrap();

        let view = store.analytics("all", 1_000).unwrap();
        assert_eq!(view.by_project.len(), 1);
        assert_eq!(view.by_project[0].project_id, "P1");
        assert_eq!(view.by_project[0].name, "项目一");
        assert_eq!(view.by_project[0].total, 1800);

        assert_eq!(view.by_agent.len(), 1);
        assert_eq!(view.by_agent[0].agent_id, "A1");
        assert_eq!(view.by_agent[0].name, "Alice");
        assert_eq!(view.by_agent[0].total, 1800);
    }
}

#[cfg(test)]
mod message_usage_tests {
    use super::UsageStore;
    use crate::provider::client::ModelUsage;
    use crate::storage::AppDatabase;
    use crate::usage::UsageRecord;
    use std::sync::Arc;

    fn store_with_messages() -> UsageStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sw-usage-msg-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Arc::new(AppDatabase::open(dir.join("u.sqlite3")).unwrap());
        db.with_connection(|c| {
            c.execute_batch(
                "create table messages (
                    id text primary key,
                    session_id text not null,
                    role text not null,
                    content text not null,
                    created_at text not null
                 );",
            )?;
            Ok(())
        })
        .unwrap();
        UsageStore::open(db).unwrap()
    }

    fn rec_msg(session: &str, message: &str, prompt: u64, output: u64, ts: &str) -> UsageRecord {
        UsageRecord {
            session_id: session.to_string(),
            message_id: Some(message.to_string()),
            provider: "p".to_string(),
            model: "m".to_string(),
            usage_type: "main_agent".to_string(),
            created_at: ts.to_string(),
            usage: ModelUsage {
                input_tokens: Some(prompt),
                output_tokens: Some(output),
                cache_read_tokens: None,
                cache_create_tokens: None,
            },
        }
    }

    #[test]
    fn groups_usage_by_message_with_snippet_and_order() {
        let store = store_with_messages();
        store.raw_for_test(|c| {
            c.execute(
                "insert into messages (id,session_id,role,content,created_at) values ('m1','s1','assistant','第一条回复内容','100')",
                [],
            )?;
            c.execute(
                "insert into messages (id,session_id,role,content,created_at) values ('m2','s1','assistant','第二条回复内容','200')",
                [],
            )?;
            Ok(())
        });
        // m1 两次调用（同一回合内可能多次），m2 一次
        store
            .record("u1", &rec_msg("s1", "m1", 1000, 200, "100"))
            .unwrap();
        store
            .record("u2", &rec_msg("s1", "m1", 500, 100, "110"))
            .unwrap();
        store
            .record("u3", &rec_msg("s1", "m2", 800, 50, "200"))
            .unwrap();

        let rows = store.session_message_usage("s1").unwrap();
        assert_eq!(rows.len(), 2);
        // 时间升序：m1 在前
        assert_eq!(rows[0].message_id, "m1");
        assert_eq!(rows[0].snippet, "第一条回复内容");
        assert_eq!(rows[0].role, "assistant");
        assert_eq!(rows[0].total, 1800); // (1000+500) in + (200+100) out
        assert_eq!(rows[1].message_id, "m2");
        assert_eq!(rows[1].total, 850);

        assert!(store.session_message_usage("missing").unwrap().is_empty());
    }
}
