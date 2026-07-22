//! 伴随体 SOUL 版本表 `agent_soul_versions` 的 SQLite 读写（T73）。
//!
//! 版本三态：`active`（当前生效，每伴随体至多一条）/ `pending`（待批准提案）/ `archived`（历史/回滚）。
//! 活跃版本的 `soul` 与 `agents.instructions`（T74 的 SOUL 别名）保持一致——同步由 service 经
//! `store::set_instructions` 完成（见 agent/service.rs）。SQL 风格镜像 `agent/store.rs`。

use rusqlite::params;

use crate::agent::model::SoulVersion;
use crate::session::new_id;
use crate::storage::AppDatabase;

const TABLE_SQL: &str = "create table if not exists agent_soul_versions (
    id          text primary key,
    agent_id    text not null,
    soul        text not null default '',
    status      text not null default 'archived',
    summary     text not null default '',
    source      text not null default 'reflection',
    created_at  text not null
);";

const COLS: &str = "id, agent_id, soul, status, summary, source, created_at";

/// 建表 + 索引。幂等。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(TABLE_SQL)?;
        c.execute_batch(
            "create index if not exists idx_soul_versions_agent on agent_soul_versions(agent_id, status);",
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<SoulVersion> {
    Ok(SoulVersion {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        soul: row.get(2)?,
        status: row.get(3)?,
        summary: row.get(4)?,
        source: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// 插入一条版本（id 由调用方给定）。
pub fn insert(db: &AppDatabase, v: &SoulVersion) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "insert into agent_soul_versions (id, agent_id, soul, status, summary, source, created_at)
             values (?1,?2,?3,?4,?5,?6,?7)",
            params![v.id, v.agent_id, v.soul, v.status, v.summary, v.source, v.created_at],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 列出某伴随体的全部版本（created_at 倒序，新在前）。
pub fn list_by_agent(db: &AppDatabase, agent_id: &str) -> Result<Vec<SoulVersion>, String> {
    db.with_connection(|c| {
        let sql = format!(
            "select {COLS} from agent_soul_versions where agent_id = ?1 order by created_at desc, id desc"
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([agent_id], row_to_version)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 取某伴随体当前活跃版本（至多一条）。
pub fn active_for(db: &AppDatabase, agent_id: &str) -> Result<Option<SoulVersion>, String> {
    db.with_connection(|c| {
        let sql = format!(
            "select {COLS} from agent_soul_versions where agent_id = ?1 and status = 'active' limit 1"
        );
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([agent_id], row_to_version)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

/// 按 id 取单条版本。
pub fn get(db: &AppDatabase, version_id: &str) -> Result<Option<SoulVersion>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from agent_soul_versions where id = ?1");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([version_id], row_to_version)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

/// 把某版本设为活跃：单事务内「现 active → archived，目标 → active」。
/// 批准（pending→active）与回滚（archived→active）共用此原子切换。
pub fn set_active(db: &AppDatabase, agent_id: &str, version_id: &str) -> Result<(), String> {
    db.with_transaction(|tx| {
        tx.execute(
            "update agent_soul_versions set status='archived' where agent_id=?1 and status='active'",
            params![agent_id],
        )?;
        tx.execute(
            "update agent_soul_versions set status='active' where id=?1 and agent_id=?2",
            params![version_id, agent_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 拒绝一条提案：pending → archived。
pub fn reject(db: &AppDatabase, version_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update agent_soul_versions set status='archived' where id=?1 and status='pending'",
            params![version_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 若该伴随体尚无任何版本，则以其当前 SOUL 种一条 `active/seed`（初始化/补种用）。
/// 返回是否新种。
pub fn seed_if_empty(db: &AppDatabase, agent_id: &str, soul: &str, now: &str) -> Result<bool, String> {
    if !list_by_agent(db, agent_id)?.is_empty() {
        return Ok(false);
    }
    insert(
        db,
        &SoulVersion {
            id: new_id("soul"),
            agent_id: agent_id.to_string(),
            soul: soul.to_string(),
            status: "active".to_string(),
            summary: "初始人格".to_string(),
            source: "seed".to_string(),
            created_at: now.to_string(),
        },
    )?;
    Ok(true)
}

/// 删除某伴随体全部版本（随伴随体删除一并清理）。
pub fn delete_by_agent(db: &AppDatabase, agent_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "delete from agent_soul_versions where agent_id = ?1",
            params![agent_id],
        )?;
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
        let p = std::env::temp_dir().join(format!("siw-soul-store-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&p);
        let db = AppDatabase::open(&p).expect("open");
        ensure_schema(&db).expect("schema");
        db
    }

    fn ver(id: &str, agent: &str, soul: &str, status: &str) -> SoulVersion {
        SoulVersion {
            id: id.into(),
            agent_id: agent.into(),
            soul: soul.into(),
            status: status.into(),
            summary: String::new(),
            source: "reflection".into(),
            created_at: "1".into(),
        }
    }

    #[test]
    fn seed_then_propose_then_approve_then_rollback() {
        let db = db();
        // 种子 active。
        assert!(seed_if_empty(&db, "A", "灵魂v0", "0").unwrap());
        assert!(!seed_if_empty(&db, "A", "灵魂v0", "0").unwrap()); // 已有版本不再种
        let active = active_for(&db, "A").unwrap().expect("active");
        assert_eq!(active.soul, "灵魂v0");
        assert_eq!(active.source, "seed");
        let seed_id = active.id.clone();

        // 提案 pending。
        insert(&db, &ver("p1", "A", "灵魂v1", "pending")).unwrap();
        assert_eq!(list_by_agent(&db, "A").unwrap().len(), 2);

        // 批准：p1 → active，旧 seed → archived。
        set_active(&db, "A", "p1").unwrap();
        assert_eq!(active_for(&db, "A").unwrap().unwrap().soul, "灵魂v1");
        assert_eq!(get(&db, &seed_id).unwrap().unwrap().status, "archived");

        // 回滚到 seed：seed → active，p1 → archived。
        set_active(&db, "A", &seed_id).unwrap();
        assert_eq!(active_for(&db, "A").unwrap().unwrap().soul, "灵魂v0");
        assert_eq!(get(&db, "p1").unwrap().unwrap().status, "archived");

        // 拒绝一条新 pending。
        insert(&db, &ver("p2", "A", "灵魂v2", "pending")).unwrap();
        reject(&db, "p2").unwrap();
        assert_eq!(get(&db, "p2").unwrap().unwrap().status, "archived");
        // 活跃版本不受拒绝影响。
        assert_eq!(active_for(&db, "A").unwrap().unwrap().soul, "灵魂v0");

        // 删除清理。
        delete_by_agent(&db, "A").unwrap();
        assert!(list_by_agent(&db, "A").unwrap().is_empty());
    }
}
