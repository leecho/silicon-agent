//! experts 索引表的 SQLite 读写。owner = agent 模块（SQL 收敛本文件，镜像 skill/store.rs）。
//!
//! 表只缓存元数据与启用状态；专家角色正文不入库。`tools` 以逗号连接文本列存储。
//! owner 三态：`plugin_id`（plugin 提供，全局）/ `team_id`（team 私有）/ 都空（散装，全局），至多一非空。
//! 唯一键 `(plugin_id, team_id, name)`。

use rusqlite::params;

use crate::expert::model::{ExpertRecord, ExpertSource};
use crate::storage::AppDatabase;

/// 新建表/重建用的列定义（`name` 不带 unique；唯一性由 `(plugin_id, team_id, name)` 复合索引保证）。
const NEW_TABLE_SQL: &str = "create table experts (
    id            text primary key,
    source        text not null,
    name          text not null,
    description   text not null,
    tools         text not null default '',
    model_tier    text not null default 'aux',
    max_turns     integer,
    role          text not null default 'member',
    plugin_id     text not null default '',
    team_id       text not null default '',
    display_name  text,
    profession    text,
    avatar        text,
    color         text,
    file_name     text not null,
    enabled       integer not null default 1,
    installed_at  text not null,
    updated_at    text not null,
    catalog_id    text,
    group_id      text
);";

fn column_exists(c: &rusqlite::Connection, col: &str) -> rusqlite::Result<bool> {
    let mut stmt = c.prepare("pragma table_info(experts)")?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for n in names {
        if n? == col {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 确保 experts 表存在并为最新 schema（plugin_id 命名空间 + 展示列）。
/// 老表（feat 早期 schema：`name unique`、无 plugin_id）一次性重建：去 name-unique + 加列，拷贝旧行。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        // T67：旧库表 `agents` → `experts` 一次性改名（索引随表 rename 自动迁移）。
        let has_old: i64 = c.query_row(
            "select count(*) from sqlite_master where type='table' and name='agents'",
            [],
            |r| r.get(0),
        )?;
        let has_new: i64 = c.query_row(
            "select count(*) from sqlite_master where type='table' and name='experts'",
            [],
            |r| r.get(0),
        )?;
        if has_old > 0 && has_new == 0 {
            c.execute_batch("alter table agents rename to experts;")?;
        }
        let table_exists: i64 = c.query_row(
            "select count(*) from sqlite_master where type='table' and name='experts'",
            [],
            |r| r.get(0),
        )?;
        if table_exists > 0 {
            if !column_exists(c, "plugin_id")? {
                // 很老的表重建：拷旧 12 列，plugin_id/team_id 默认 ''、展示列 NULL。
                c.execute_batch(&format!(
                    "alter table experts rename to experts_old;
                     {NEW_TABLE_SQL}
                     insert into experts
                       (id, source, name, description, tools, model_tier, max_turns, role, file_name, enabled, installed_at, updated_at)
                     select id, source, name, description, tools, model_tier, max_turns, role, file_name, enabled, installed_at, updated_at
                       from experts_old;
                     drop table experts_old;"
                ))?;
            } else if !column_exists(c, "team_id")? {
                // master schema（有 plugin_id 无 team_id）：补列即可。
                c.execute_batch("alter table experts add column team_id text not null default '';")?;
            }
            // 广场来源标记列（可空）：旧表补列。
            if !column_exists(c, "catalog_id")? {
                c.execute_batch("alter table experts add column catalog_id text;")?;
            }
            // 「我的」分组列（可空）：旧表补列。
            if !column_exists(c, "group_id")? {
                c.execute_batch("alter table experts add column group_id text;")?;
            }
        } else {
            c.execute_batch(NEW_TABLE_SQL)?;
        }
        // 旧唯一索引 (plugin_id, name) 会阻止两个 team 各自的同名私有 agent（都 plugin_id=''）；
        // 改用 (plugin_id, team_id, name)。
        c.execute_batch(
            "drop index if exists ux_agents_plugin_name;
             create unique index if not exists ux_agents_owner_name on experts(plugin_id, team_id, name);
             create index if not exists idx_agents_enabled_name on experts(enabled, name);",
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

/// upsert（按 `(plugin_id, team_id, name)` 冲突）：新增插入；已存在则更新元数据，**保留 enabled/id/installed_at**。
pub fn upsert(db: &AppDatabase, r: &ExpertRecord) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "insert into experts
               (id, source, name, description, tools, model_tier, max_turns, role,
                plugin_id, team_id, display_name, profession, avatar, color, file_name,
                enabled, installed_at, updated_at, catalog_id, group_id)
             values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
             on conflict(plugin_id, team_id, name) do update set
               source = excluded.source,
               description = excluded.description,
               tools = excluded.tools,
               model_tier = excluded.model_tier,
               max_turns = excluded.max_turns,
               role = excluded.role,
               display_name = excluded.display_name,
               profession = excluded.profession,
               avatar = excluded.avatar,
               color = excluded.color,
               file_name = excluded.file_name,
               updated_at = excluded.updated_at,
               catalog_id = excluded.catalog_id",
            // 注意：group_id 不在 on-conflict 更新内——重扫描/重索引时保留用户已设分组。
            params![
                r.id,
                r.source.as_str(),
                r.name,
                r.description,
                join_tools(&r.tools),
                r.model_tier,
                r.max_turns,
                r.role,
                r.plugin_id,
                r.team_id,
                r.display_name,
                r.profession,
                r.avatar,
                r.color,
                r.file_name,
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

/// 删除某 plugin 下、不在 `keep`（仍声明的 name）内的孤儿 agent 行（plugin 降级/卸载清理）。
pub fn delete_plugin_orphans(
    db: &AppDatabase,
    plugin_id: &str,
    keep: &[String],
) -> Result<(), String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare("select name from experts where plugin_id = ?1")?;
        let names: Vec<String> = stmt
            .query_map([plugin_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for n in names {
            if !keep.iter().any(|k| k == &n) {
                c.execute(
                    "delete from experts where plugin_id = ?1 and name = ?2",
                    params![plugin_id, n],
                )?;
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 切换某专家启用状态。
pub fn set_enabled(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update experts set enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 删除某专家索引行。
pub fn delete(db: &AppDatabase, id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from experts where id = ?1", params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

const COLS: &str = "id, source, name, description, tools, model_tier, max_turns, role, plugin_id, team_id, display_name, profession, avatar, color, file_name, enabled, installed_at, updated_at, catalog_id, group_id";

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExpertRecord> {
    let source: String = row.get(1)?;
    let tools: String = row.get(4)?;
    let enabled: i64 = row.get(15)?;
    Ok(ExpertRecord {
        id: row.get(0)?,
        source: ExpertSource::from_str(&source),
        name: row.get(2)?,
        description: row.get(3)?,
        tools: split_tools(&tools),
        model_tier: row.get(5)?,
        max_turns: row.get(6)?,
        role: row.get(7)?,
        plugin_id: row.get(8)?,
        team_id: row.get(9)?,
        display_name: row.get(10)?,
        profession: row.get(11)?,
        avatar: row.get(12)?,
        color: row.get(13)?,
        file_name: row.get(14)?,
        enabled: enabled != 0,
        installed_at: row.get(16)?,
        updated_at: row.get(17)?,
        catalog_id: row.get(18)?,
        group_id: row.get(19)?,
    })
}

/// 列出全部专家（按 name 升序）。
pub fn list(db: &AppDatabase) -> Result<Vec<ExpertRecord>, String> {
    query_many(db, &format!("select {COLS} from experts order by name"))
}

/// 设置某散装专家的分组（None=移出分组）。
pub fn set_group(db: &AppDatabase, id: &str, group_id: Option<&str>) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update experts set group_id = ?1 where id = ?2",
            params![group_id, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 把某分组下所有专家归零（删除分组时调用）。
pub fn clear_group(db: &AppDatabase, group_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update experts set group_id = null where group_id = ?1",
            params![group_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 列出所有已加入广场目录的 catalog_id（去重由调用方处理）。供广场标注「已加入」。
pub fn list_catalog_ids(db: &AppDatabase) -> Result<Vec<String>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare("select catalog_id from experts where catalog_id is not null")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 列出**公开**的启用专家（散装 + plugin 提供），按 name 升序。
///
/// **`team_id = ''` 这个条件是隔离的地基，别拿掉。** team 私有专家只在激活该团队时
/// 由 roster 载入，绝不能出现在任何全局面。
///
/// 此前这里没有 owner 过滤，而 UI 侧的 `list_manageable` 却在内存里过滤了 —— 两个口径
/// 不一致，导致 team 私有专家「UI 看不见、却会漏进全局池」。现对齐
/// `skill::store::list_enabled` 的做法：**在 SQL 里过滤**，而不是指望每个调用方自觉。
pub fn list_enabled(db: &AppDatabase) -> Result<Vec<ExpertRecord>, String> {
    query_many(
        db,
        &format!("select {COLS} from experts where enabled = 1 and team_id = '' order by name"),
    )
}

/// 按 name 找**公开**专家（`team_id = ''`），可能命中多行（不同 plugin 同名）。
///
/// 供 dispatch 的裸名解析用。命中多行时由上层报歧义错，**不得静默挑一个**。
pub fn list_public_by_name(db: &AppDatabase, name: &str) -> Result<Vec<ExpertRecord>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare(&format!(
            "select {COLS} from experts where team_id = '' and name = ?1 order by plugin_id"
        ))?;
        let rows = stmt
            .query_map([name], row_to_record)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
    .map_err(|e| e.to_string())
}

/// 删除某 plugin 提供的全部专家（卸载级联）。
///
/// 与 `delete_plugin_orphans` 区分：那个是**重新索引**时清掉「不再声明」的行（带 keep 白名单）；
/// 这个是**卸载**时全清。语义不同，不复用。
pub fn delete_by_plugin(db: &AppDatabase, plugin_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "delete from experts where plugin_id = ?1",
            params![plugin_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn query_many(db: &AppDatabase, sql: &str) -> Result<Vec<ExpertRecord>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare(sql)?;
        let rows = stmt.query_map([], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 按 id 取专家行。
pub fn get_by_id(db: &AppDatabase, id: &str) -> Result<Option<ExpertRecord>, String> {
    get_one(db, "id = ?1", id)
}

/// 按 name 取专家行（任意 plugin_id；散装查询用，命中最先匹配）。
pub fn get_by_name(db: &AppDatabase, name: &str) -> Result<Option<ExpertRecord>, String> {
    get_one(db, "name = ?1", name)
}

/// 按 `(plugin_id, name)` 精确取专家行（命名空间解析用）。
pub fn get_by_plugin_and_name(
    db: &AppDatabase,
    plugin_id: &str,
    name: &str,
) -> Result<Option<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from experts where plugin_id = ?1 and name = ?2");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map(params![plugin_id, name], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

/// 列出某 plugin 下的启用专家（按 name 升序）。
pub fn list_enabled_by_plugin(
    db: &AppDatabase,
    plugin_id: &str,
) -> Result<Vec<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!(
            "select {COLS} from experts where plugin_id = ?1 and enabled = 1 order by name"
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([plugin_id], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 列出某 plugin 下的全部专家（含未启用，按 name 升序）；供详情页展示。
pub fn list_by_plugin(db: &AppDatabase, plugin_id: &str) -> Result<Vec<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from experts where plugin_id = ?1 order by name");
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([plugin_id], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 按 owner 命名空间（plugin_id, team_id 至多一非空；都空=散装）精确取行。
pub fn get_by_owner_and_name(
    db: &AppDatabase,
    plugin_id: &str,
    team_id: &str,
    name: &str,
) -> Result<Option<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!(
            "select {COLS} from experts where plugin_id = ?1 and team_id = ?2 and name = ?3"
        );
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map(params![plugin_id, team_id, name], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

/// 列出某 team 的私有 agent（team_id 非空，含未启用，按 name 升序）。
pub fn list_by_team(db: &AppDatabase, team_id: &str) -> Result<Vec<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from experts where team_id = ?1 order by name");
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map([team_id], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 删除某 team 的全部私有 agent（team 删除时级联）。
pub fn delete_by_team(db: &AppDatabase, team_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from experts where team_id = ?1", params![team_id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn get_one(
    db: &AppDatabase,
    where_clause: &str,
    val: &str,
) -> Result<Option<ExpertRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from experts where {where_clause}");
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
    use crate::storage::AppDatabase;

    fn db() -> AppDatabase {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let p = std::env::temp_dir().join(format!(
            "siw-agent-store-{}-{}.db",
            std::process::id(),
            nanos
        ));
        let _ = std::fs::remove_file(&p);
        AppDatabase::open(&p).expect("open db")
    }

    fn rec(name: &str) -> ExpertRecord {
        rec_in("", name)
    }
    fn rec_in(plugin_id: &str, name: &str) -> ExpertRecord {
        rec_owner(plugin_id, "", name)
    }
    fn rec_owner(plugin_id: &str, team_id: &str, name: &str) -> ExpertRecord {
        let owner = if !plugin_id.is_empty() {
            plugin_id
        } else {
            team_id
        };
        ExpertRecord {
            id: if owner.is_empty() {
                format!("id-{name}")
            } else {
                format!("id-{owner}-{name}")
            },
            source: if plugin_id.is_empty() {
                ExpertSource::User
            } else {
                ExpertSource::Plugin
            },
            name: name.into(),
            description: "d".into(),
            tools: vec!["read_file".into(), "grep".into()],
            model_tier: "aux".into(),
            max_turns: Some(12),
            role: "member".into(),
            plugin_id: plugin_id.into(),
            team_id: team_id.into(),
            display_name: None,
            profession: None,
            avatar: None,
            color: None,
            file_name: format!("{name}.md"),
            enabled: true,
            installed_at: "1".into(),
            updated_at: "1".into(),
            catalog_id: None,
            group_id: None,
        }
    }

    #[test]
    fn upsert_get_list_toggle_delete() {
        let db = db();
        ensure_schema(&db).expect("schema");
        upsert(&db, &rec("explorer")).expect("upsert");

        let got = get_by_name(&db, "explorer").expect("get").expect("some");
        assert_eq!(got.tools, vec!["read_file", "grep"]);
        assert_eq!(got.max_turns, Some(12));

        assert_eq!(list_enabled(&db).expect("le").len(), 1);
        set_enabled(&db, "id-explorer", false, "2").expect("toggle");
        assert_eq!(list_enabled(&db).expect("le2").len(), 0);
        assert_eq!(list(&db).expect("list").len(), 1);

        delete(&db, "id-explorer").expect("del");
        assert!(get_by_name(&db, "explorer").expect("get2").is_none());
    }

    #[test]
    fn plugin_namespace_isolates_same_name() {
        let db = db();
        ensure_schema(&db).expect("schema");
        // 同名 "analyst" 分属散装('')与两个 plugin，三行共存。
        upsert(&db, &rec_in("", "analyst")).expect("u0");
        upsert(&db, &rec_in("plg-a", "analyst")).expect("ua");
        upsert(&db, &rec_in("plg-b", "analyst")).expect("ub");

        assert_eq!(list(&db).expect("list").len(), 3);
        assert_eq!(
            get_by_plugin_and_name(&db, "plg-a", "analyst")
                .expect("g")
                .expect("some")
                .source,
            ExpertSource::Plugin
        );
        assert_eq!(list_enabled_by_plugin(&db, "plg-b").expect("lb").len(), 1);

        // 孤儿清理：plg-a 现只保留空集 → 该 plugin 行清掉，其余不动。
        delete_plugin_orphans(&db, "plg-a", &[]).expect("orphan");
        assert!(get_by_plugin_and_name(&db, "plg-a", "analyst")
            .expect("g2")
            .is_none());
        assert_eq!(list(&db).expect("list2").len(), 2);
    }

    #[test]
    fn team_owner_is_private_and_isolated() {
        let db = db();
        ensure_schema(&db).expect("schema");
        // 同名 "lead" 分属两个 team（都 plugin_id=''），加一条散装同名 → 三行共存（旧 (plugin_id,name) 唯一会冲突）。
        upsert(&db, &rec_owner("", "t1", "lead")).expect("t1");
        upsert(&db, &rec_owner("", "t2", "lead")).expect("t2");
        upsert(&db, &rec_owner("", "", "lead")).expect("std");
        assert_eq!(list(&db).expect("list").len(), 3);

        // owner 精确解析。
        assert!(get_by_owner_and_name(&db, "", "t1", "lead")
            .expect("g")
            .is_some());
        assert!(get_by_owner_and_name(&db, "", "", "lead")
            .expect("g")
            .is_some());
        assert_eq!(list_by_team(&db, "t1").expect("lt").len(), 1);
        assert_eq!(list_by_team(&db, "t2").expect("lt").len(), 1);

        // team 级联删：t1 私有没了，t2 与散装不动。
        delete_by_team(&db, "t1").expect("del team");
        assert!(get_by_owner_and_name(&db, "", "t1", "lead")
            .expect("g")
            .is_none());
        assert!(get_by_owner_and_name(&db, "", "t2", "lead")
            .expect("g")
            .is_some());
        assert!(get_by_owner_and_name(&db, "", "", "lead")
            .expect("g")
            .is_some());
        assert_eq!(list(&db).expect("list2").len(), 2);
    }
}

#[cfg(test)]
mod migration_tests {
    use crate::storage::AppDatabase;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// T67：旧库表 `agents`（含数据）→ `experts` 改名后数据保留；skills 旧 `agent_id` 列
    /// 走既有 drop-重建路径（schema 迁移既定行为），重建后含 `expert_id` 列。
    #[test]
    fn migrates_legacy_agents_table_to_experts() {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("t67-mig-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        let db = AppDatabase::open(&path).unwrap();
        // 造旧库：当前 schema 的 agents 表（有 plugin_id/team_id）+ 一行数据；skills 旧 agent_id 列。
        db.with_connection(|c| {
            c.execute_batch(
                "create table agents (id text primary key, source text not null, name text not null,
                   description text not null, tools text not null default '', model_tier text not null default 'aux',
                   max_turns integer, role text not null default 'member', plugin_id text not null default '',
                   team_id text not null default '', display_name text, profession text, avatar text, color text,
                   file_name text not null, enabled integer not null default 1, installed_at text not null,
                   updated_at text not null, catalog_id text, group_id text);
                 insert into agents (id,source,name,description,file_name,installed_at,updated_at)
                   values ('e1','User','张三','desc','张三.md','0','0');
                 create table skills (id text primary key, source text not null default 'User', name text not null,
                   description text not null default '', dir_name text not null default '', enabled integer not null default 1,
                   installed_at text not null default '0', updated_at text not null default '0',
                   plugin_id text not null default '', team_id text not null default '', agent_id text not null default '',
                   user_invocable integer not null default 1, argument_hint text, group_id text);",
            )?;
            Ok(())
        })
        .unwrap();
        // 跑迁移。
        super::ensure_schema(&db).unwrap();
        crate::skill::store::ensure_schema(&db).unwrap();
        db.with_connection(|c| {
            // experts 表存在、数据保留（rename 不丢行）。
            let cnt: i64 = c.query_row("select count(*) from experts", [], |r| r.get(0))?;
            assert_eq!(cnt, 1, "experts 应保留 1 行");
            let name: String =
                c.query_row("select name from experts where id='e1'", [], |r| r.get(0))?;
            assert_eq!(name, "张三");
            // 旧 agents 表已不存在。
            let old: i64 = c.query_row(
                "select count(*) from sqlite_master where type='table' and name='agents'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(old, 0, "旧 agents 表应已改名");
            // skills 重建后含 expert_id 列。
            let mut has_expert_id = false;
            let mut stmt = c.prepare("pragma table_info(skills)")?;
            for col in stmt.query_map([], |r| r.get::<_, String>(1))? {
                if col? == "expert_id" {
                    has_expert_id = true;
                }
            }
            assert!(has_expert_id, "skills 重建后应含 expert_id 列");
            Ok(())
        })
        .unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
