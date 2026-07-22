//! 伴随体 `agents` 索引表的 SQLite 读写（SQL 收敛本文件，镜像 expert/store.rs 风格）。
//!
//! ⚠️ 表名 `agents` 复用 T67 腾出的名字（旧 `agents`=expert 已迁移为 `experts`）。
//! **`ensure_schema` 必须在 `expert::store::ensure_schema` 之后调用**——先完成 agents→experts 改名，
//! 再建本伴随体新表，否则新表会被 T67 的旧迁移误改名。调用顺序由 builder 固定（见 agent/service.rs）。

use rusqlite::params;

use crate::agent::model::AgentRecord;
use crate::storage::AppDatabase;

const NEW_TABLE_SQL: &str = "create table if not exists agents (
    id               text primary key,
    name             text not null,
    instructions     text not null default '',
    tools            text not null default '',
    model_tier       text not null default 'main',
    source_expert_id text,
    display_name     text,
    profession       text,
    avatar           text,
    color            text,
    enabled          integer not null default 1,
    group_id         text,
    created_at       text not null,
    updated_at       text not null,
    working_dir      text,
    identity         text not null default '',
    evolution_enabled integer not null default 0,
    last_reflection_at integer
);";

fn column_exists(c: &rusqlite::Connection, col: &str) -> rusqlite::Result<bool> {
    let mut stmt = c.prepare("pragma table_info(agents)")?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for n in names {
        if n? == col {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 确保伴随体 `agents` 表与唯一索引存在。幂等。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(NEW_TABLE_SQL)?;
        // T69+：专属工作目录列；旧库（T69 首版无此列）幂等补列。
        if !column_exists(c, "working_dir")? {
            c.execute_batch("alter table agents add column working_dir text;")?;
        }
        // T74：IDENTITY 稳定锚列；旧库幂等补列，存量行 identity=''（注入时退化为仅 SOUL）。
        if !column_exists(c, "identity")? {
            c.execute_batch("alter table agents add column identity text not null default '';")?;
        }
        // T73：演化开关 + 上次反思时刻；旧库幂等补列。
        if !column_exists(c, "evolution_enabled")? {
            c.execute_batch("alter table agents add column evolution_enabled integer not null default 0;")?;
        }
        if !column_exists(c, "last_reflection_at")? {
            c.execute_batch("alter table agents add column last_reflection_at integer;")?;
        }
        c.execute_batch(
            "create unique index if not exists ux_agents_name on agents(name);
             create index if not exists idx_agents_enabled_name on agents(enabled, name);",
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn join_tools(tools: &[String]) -> String {
    tools.join(",")
}
fn split_tools(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

const COLS: &str = "id, name, instructions, tools, model_tier, source_expert_id, display_name, profession, avatar, color, enabled, group_id, created_at, updated_at, working_dir, identity, evolution_enabled, last_reflection_at";

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    let tools: String = row.get(3)?;
    let enabled: i64 = row.get(10)?;
    Ok(AgentRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        instructions: row.get(2)?,
        tools: split_tools(&tools),
        model_tier: row.get(4)?,
        source_expert_id: row.get(5)?,
        display_name: row.get(6)?,
        profession: row.get(7)?,
        avatar: row.get(8)?,
        color: row.get(9)?,
        enabled: enabled != 0,
        group_id: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        working_dir: row.get(14)?,
        identity: row.get(15)?,
        evolution_enabled: row.get::<_, i64>(16)? != 0,
        last_reflection_at: row.get(17)?,
    })
}

/// upsert（按主键 id；唯一名冲突由 `ux_agents_name` 兜底，调用方先查重名）。
pub fn upsert(db: &AppDatabase, r: &AgentRecord) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "insert into agents
               (id, name, instructions, tools, model_tier, source_expert_id,
                display_name, profession, avatar, color, enabled, group_id, created_at, updated_at, working_dir, identity, evolution_enabled, last_reflection_at)
             values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
             on conflict(id) do update set
               name = excluded.name,
               instructions = excluded.instructions,
               tools = excluded.tools,
               model_tier = excluded.model_tier,
               source_expert_id = excluded.source_expert_id,
               display_name = excluded.display_name,
               profession = excluded.profession,
               avatar = excluded.avatar,
               color = excluded.color,
               enabled = excluded.enabled,
               group_id = excluded.group_id,
               updated_at = excluded.updated_at,
               working_dir = excluded.working_dir,
               identity = excluded.identity,
               evolution_enabled = excluded.evolution_enabled,
               last_reflection_at = excluded.last_reflection_at",
            params![
                r.id,
                r.name,
                r.instructions,
                join_tools(&r.tools),
                r.model_tier,
                r.source_expert_id,
                r.display_name,
                r.profession,
                r.avatar,
                r.color,
                if r.enabled { 1 } else { 0 },
                r.group_id,
                r.created_at,
                r.updated_at,
                r.working_dir,
                r.identity,
                if r.evolution_enabled { 1 } else { 0 },
                r.last_reflection_at,
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn get_one(db: &AppDatabase, where_clause: &str, val: &str) -> Result<Option<AgentRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from agents where {where_clause}");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([val], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

pub fn get_by_id(db: &AppDatabase, id: &str) -> Result<Option<AgentRecord>, String> {
    get_one(db, "id = ?1", id)
}

pub fn get_by_name(db: &AppDatabase, name: &str) -> Result<Option<AgentRecord>, String> {
    get_one(db, "name = ?1", name)
}

pub fn list(db: &AppDatabase) -> Result<Vec<AgentRecord>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare(&format!("select {COLS} from agents order by name"))?;
        let rows = stmt.query_map([], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

pub fn set_enabled(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agents set enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn set_group(db: &AppDatabase, id: &str, group_id: Option<&str>) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agents set group_id = ?1 where id = ?2",
            params![group_id, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// T73：更新某伴随体的 SOUL（= `instructions` 列，T74 别名）并刷新 updated_at。
/// 批准/回滚 SOUL 版本时调用，使活跃版本与注入用的 `instructions` 保持一致。
pub fn set_instructions(db: &AppDatabase, id: &str, soul: &str, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agents set instructions = ?1, updated_at = ?2 where id = ?3",
            params![soul, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// T73：设置演化开关。
pub fn set_evolution(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agents set evolution_enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// T73：记录上次触发反思的时刻（epoch 秒）。
pub fn set_last_reflection_at(db: &AppDatabase, id: &str, at: i64, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agents set last_reflection_at = ?1, updated_at = ?2 where id = ?3",
            params![at, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn delete(db: &AppDatabase, id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from agents where id = ?1", params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn db() -> AppDatabase {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("siw-agent-store-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&p);
        let db = AppDatabase::open(&p).expect("open");
        ensure_schema(&db).expect("schema");
        db
    }

    fn rec(id: &str, name: &str) -> AgentRecord {
        AgentRecord {
            id: id.into(),
            name: name.into(),
            instructions: "你是助手".into(),
            identity: String::new(),
            evolution_enabled: false,
            last_reflection_at: None,
            tools: vec!["web_search".into()],
            model_tier: "main".into(),
            source_expert_id: Some("研究员".into()),
            working_dir: None,
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            enabled: true,
            group_id: None,
            created_at: "0".into(),
            updated_at: "0".into(),
        }
    }

    #[test]
    fn crud_roundtrip() {
        let db = db();
        upsert(&db, &rec("a1", "小研")).unwrap();
        let got = get_by_name(&db, "小研").unwrap().expect("found");
        assert_eq!(got.id, "a1");
        assert_eq!(got.tools, vec!["web_search".to_string()]);
        assert_eq!(got.source_expert_id.as_deref(), Some("研究员"));
        assert_eq!(list(&db).unwrap().len(), 1);

        // 同 id upsert：更新而非新增。
        let mut r2 = rec("a1", "小研究");
        r2.instructions = "改了".into();
        upsert(&db, &r2).unwrap();
        assert_eq!(list(&db).unwrap().len(), 1);
        assert_eq!(get_by_id(&db, "a1").unwrap().unwrap().instructions, "改了");

        set_enabled(&db, "a1", false, "1").unwrap();
        assert!(!get_by_id(&db, "a1").unwrap().unwrap().enabled);

        delete(&db, "a1").unwrap();
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn migration_adds_identity_to_old_db() {
        // 模拟 T74 前的旧库：无 identity 列的 agents 表 + 一行存量数据。
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("siw-agent-mig-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&p);
        let db = AppDatabase::open(&p).expect("open");
        db.with_connection(|c| {
            c.execute_batch(
                "create table agents (
                   id text primary key, name text not null,
                   instructions text not null default '', tools text not null default '',
                   model_tier text not null default 'main', source_expert_id text,
                   display_name text, profession text, avatar text, color text,
                   enabled integer not null default 1, group_id text,
                   created_at text not null, updated_at text not null, working_dir text
                 );
                 insert into agents (id,name,instructions,tools,model_tier,created_at,updated_at)
                 values ('old1','旧伴随','旧人设','', 'main','0','0');",
            )?;
            Ok(())
        })
        .unwrap();

        // 跑迁移。
        ensure_schema(&db).expect("schema");

        // 存量行读得出、identity 归空、SOUL(instructions) 不丢。
        let got = get_by_id(&db, "old1").unwrap().expect("found");
        assert_eq!(got.identity, "");
        assert_eq!(got.instructions, "旧人设");
        assert!(!got.evolution_enabled);
        assert_eq!(got.last_reflection_at, None);
        let _ = std::fs::remove_file(&p);
    }
}
