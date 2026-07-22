//! plugins 索引表的 SQLite 读写。owner = plugin 模块。
//!
//! 表只缓存插件元数据与启用状态；其下 skill 在 skills 表中以 `plugin_id` 关联（owner = skill 模块）。

use rusqlite::params;

use crate::plugin::model::{PluginRecord, PluginSource};
use crate::storage::AppDatabase;

/// 确保 plugins 索引表存在。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(
            "create table if not exists plugins (
                id              text primary key,
                source          text not null,
                name            text not null unique,
                display_name    text not null,
                version         text not null,
                description     text not null,
                description_zh  text,
                category        text,
                customized_from text,
                dir_name        text not null,
                enabled         integer not null default 1,
                installed_at    text not null,
                updated_at      text not null
            );",
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// upsert（按 name 冲突）：新增插入；已存在则更新元数据/updated_at，**保留 enabled/id/installed_at**。
pub fn upsert(db: &AppDatabase, r: &PluginRecord) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "insert into plugins
               (id, source, name, display_name, version, description, description_zh,
                category, customized_from, dir_name, enabled, installed_at, updated_at)
             values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             on conflict(name) do update set
               source = excluded.source,
               display_name = excluded.display_name,
               version = excluded.version,
               description = excluded.description,
               description_zh = excluded.description_zh,
               category = excluded.category,
               customized_from = excluded.customized_from,
               dir_name = excluded.dir_name,
               updated_at = excluded.updated_at",
            params![
                r.id,
                r.source.as_str(),
                r.name,
                r.display_name,
                r.version,
                r.description,
                r.description_zh,
                r.category,
                r.customized_from,
                r.dir_name,
                if r.enabled { 1 } else { 0 },
                r.installed_at,
                r.updated_at,
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 切换插件启用状态。
pub fn set_enabled(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update plugins set enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 删除插件索引行。
pub fn delete(db: &AppDatabase, id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from plugins where id = ?1", params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

const COLS: &str = "id, source, name, display_name, version, description, description_zh, category, customized_from, dir_name, enabled, installed_at, updated_at";

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<PluginRecord> {
    let source: String = row.get(1)?;
    let enabled: i64 = row.get(10)?;
    Ok(PluginRecord {
        id: row.get(0)?,
        source: PluginSource::from_str(&source),
        name: row.get(2)?,
        display_name: row.get(3)?,
        version: row.get(4)?,
        description: row.get(5)?,
        description_zh: row.get(6)?,
        category: row.get(7)?,
        customized_from: row.get(8)?,
        dir_name: row.get(9)?,
        enabled: enabled != 0,
        installed_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

/// 列出全部插件（按 name 升序）。
pub fn list(db: &AppDatabase) -> Result<Vec<PluginRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from plugins order by name");
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 列出被禁用插件的 id 集合（供引擎级联隐藏其 skill）。
pub fn disabled_ids(db: &AppDatabase) -> Result<Vec<String>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare("select id from plugins where enabled = 0")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

pub fn get_by_id(db: &AppDatabase, id: &str) -> Result<Option<PluginRecord>, String> {
    get_one(db, "id", id)
}

pub fn get_by_name(db: &AppDatabase, name: &str) -> Result<Option<PluginRecord>, String> {
    get_one(db, "name", name)
}

fn get_one(db: &AppDatabase, col: &str, val: &str) -> Result<Option<PluginRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from plugins where {col} = ?1");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([val], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}
