//! teams 索引表的 SQLite 读写。owner = team 模块（SQL 收敛本文件）。
//!
//! 编排定义（lead/members/quick_prompts）以 JSON 列存储；私有组件不在此表
//! （在 skills/agents 表以 `team_id` 关联，删除时由 TeamService 级联）。

use rusqlite::params;

use crate::storage::AppDatabase;
use crate::team::model::{TeamMember, TeamRecord, TeamSource};

const COLS: &str = "id, source, name, display_name, description, lead_json, members_json, avatar, category, quick_prompts_json, enabled, installed_at, updated_at, catalog_id, group_id";

fn column_exists(c: &rusqlite::Connection, col: &str) -> rusqlite::Result<bool> {
    let mut stmt = c.prepare("pragma table_info(teams)")?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for n in names {
        if n? == col {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 确保 teams 表存在。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(
            "create table if not exists teams (
                id                 text primary key,
                source             text not null,
                name               text not null unique,
                display_name       text not null,
                description        text not null default '',
                lead_json          text,
                members_json       text not null default '[]',
                avatar             text,
                category           text,
                quick_prompts_json text not null default '[]',
                enabled            integer not null default 1,
                installed_at       text not null,
                updated_at         text not null,
                catalog_id         text,
                group_id           text
            );",
        )?;
        // 旧库补列（广场来源标记）。
        if !column_exists(c, "catalog_id")? {
            c.execute_batch("alter table teams add column catalog_id text;")?;
        }
        // 旧库补列（「我的」分组）。
        if !column_exists(c, "group_id")? {
            c.execute_batch("alter table teams add column group_id text;")?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| format!("序列化失败：{e}"))
}

/// upsert（按 name 冲突）：新增插入；已存在则更新编排定义，保留 enabled/id/installed_at。
pub fn upsert(db: &AppDatabase, r: &TeamRecord) -> Result<(), String> {
    let lead_json = match &r.lead {
        Some(m) => Some(to_json(m)?),
        None => None,
    };
    let members_json = to_json(&r.members)?;
    let quick_prompts_json = to_json(&r.quick_prompts)?;
    db.with_connection(|c| {
        c.execute(
            "insert into teams
               (id, source, name, display_name, description, lead_json, members_json,
                avatar, category, quick_prompts_json, enabled, installed_at, updated_at, catalog_id, group_id)
             values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             on conflict(name) do update set
               source = excluded.source,
               display_name = excluded.display_name,
               description = excluded.description,
               lead_json = excluded.lead_json,
               members_json = excluded.members_json,
               avatar = excluded.avatar,
               category = excluded.category,
               quick_prompts_json = excluded.quick_prompts_json,
               updated_at = excluded.updated_at,
               catalog_id = excluded.catalog_id",
            params![
                r.id,
                r.source.as_str(),
                r.name,
                r.display_name,
                r.description,
                lead_json,
                members_json,
                r.avatar,
                r.category,
                quick_prompts_json,
                if r.enabled { 1 } else { 0 },
                r.installed_at,
                r.updated_at,
                r.catalog_id,
                r.group_id,
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn set_enabled(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update teams set enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn delete(db: &AppDatabase, id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from teams where id = ?1", params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TeamRecord> {
    let source: String = row.get(1)?;
    let lead_json: Option<String> = row.get(5)?;
    let members_json: String = row.get(6)?;
    let quick_prompts_json: String = row.get(9)?;
    let enabled: i64 = row.get(10)?;
    let lead: Option<TeamMember> = lead_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    let members: Vec<TeamMember> = serde_json::from_str(&members_json).unwrap_or_default();
    let quick_prompts: Vec<String> = serde_json::from_str(&quick_prompts_json).unwrap_or_default();
    Ok(TeamRecord {
        id: row.get(0)?,
        source: TeamSource::from_str(&source),
        name: row.get(2)?,
        display_name: row.get(3)?,
        description: row.get(4)?,
        lead,
        members,
        avatar: row.get(7)?,
        category: row.get(8)?,
        quick_prompts,
        enabled: enabled != 0,
        installed_at: row.get(11)?,
        updated_at: row.get(12)?,
        catalog_id: row.get(13)?,
        group_id: row.get(14)?,
    })
}

/// 设置团队分组（None=移出分组）。
pub fn set_group(db: &AppDatabase, id: &str, group_id: Option<&str>) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update teams set group_id = ?1 where id = ?2",
            params![group_id, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 把某分组下所有团队归零（删除分组时调用）。
pub fn clear_group(db: &AppDatabase, group_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update teams set group_id = null where group_id = ?1",
            params![group_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

pub fn list(db: &AppDatabase) -> Result<Vec<TeamRecord>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare(&format!("select {COLS} from teams order by name"))?;
        let rows = stmt.query_map([], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 列出所有已加入广场目录的 catalog_id。供广场标注「已加入」。
pub fn list_catalog_ids(db: &AppDatabase) -> Result<Vec<String>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare("select catalog_id from teams where catalog_id is not null")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

pub fn get_by_id(db: &AppDatabase, id: &str) -> Result<Option<TeamRecord>, String> {
    get_one(db, "id = ?1", id)
}

pub fn get_by_name(db: &AppDatabase, name: &str) -> Result<Option<TeamRecord>, String> {
    get_one(db, "name = ?1", name)
}

fn get_one(db: &AppDatabase, where_clause: &str, val: &str) -> Result<Option<TeamRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from teams where {where_clause}");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([val], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> AppDatabase {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let p = std::env::temp_dir().join(format!(
            "siw-team-store-{}-{}.db",
            std::process::id(),
            nanos
        ));
        let _ = std::fs::remove_file(&p);
        AppDatabase::open(&p).expect("open db")
    }

    fn member(name: &str, role: &str) -> TeamMember {
        TeamMember {
            plugin_id: String::new(),
            team_id: String::new(),
            name: name.into(),
            role: role.into(),
            display_name: None,
            profession: None,
            avatar: None,
        }
    }

    #[test]
    fn upsert_roundtrip_get_toggle_delete() {
        let db = db();
        ensure_schema(&db).expect("schema");
        let rec = TeamRecord {
            id: "team-1".into(),
            source: TeamSource::User,
            name: "trade-desk".into(),
            display_name: "交易台".into(),
            description: "投研团队".into(),
            lead: Some(member("coordinator", "lead")),
            members: vec![member("researcher", "member"), member("writer", "member")],
            avatar: None,
            category: Some("finance".into()),
            quick_prompts: vec!["分析这只股票".into(), "写一份周报".into()],
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            catalog_id: Some("cat-trade".into()),
            group_id: None,
        };
        upsert(&db, &rec).expect("upsert");

        let got = get_by_id(&db, "team-1").expect("g").expect("some");
        assert_eq!(got.lead.as_ref().unwrap().name, "coordinator");
        assert_eq!(got.members.len(), 2);
        assert_eq!(got.members[1].name, "writer");
        assert_eq!(got.quick_prompts, vec!["分析这只股票", "写一份周报"]);
        assert_eq!(got.category.as_deref(), Some("finance"));
        assert_eq!(got.catalog_id.as_deref(), Some("cat-trade"));

        assert_eq!(list(&db).expect("list").len(), 1);
        set_enabled(&db, "team-1", false, "2").expect("toggle");
        assert!(!get_by_name(&db, "trade-desk").expect("g").unwrap().enabled);
        delete(&db, "team-1").expect("del");
        assert!(get_by_id(&db, "team-1").expect("g").is_none());
    }
}
