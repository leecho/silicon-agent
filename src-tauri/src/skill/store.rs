//! skills 索引表的 SQLite 读写。owner = skill 模块（按后端规范，模块 SQL 收敛到本文件）。
//!
//! 表只缓存元数据与启用状态；技能正文不入库。`ensure_schema` 含一次性旧 schema 迁移
//! （旧表缺 `plugin_id` 列或仍有 `content` 列 → drop 重建，符合「空库重来」决策；
//! sync 会从磁盘重建索引，drop 安全）。
//!
//! 唯一键 `(plugin_id, name)`：散装 skill 的 `plugin_id` 为哨兵空串 `''`，插件内 skill 带其插件 id，
//! 故不同插件可有同名 skill。

use rusqlite::params;

use crate::skill::model::{SkillRecord, SkillSource};
use crate::storage::AppDatabase;

/// 散装 skill 的 plugin_id 哨兵值（持久化层不存 NULL，便于唯一约束）。
const STANDALONE: &str = "";

/// 确保 skills 索引表为新 schema。检测到旧 schema（含 `content` 列，或缺 `plugin_id` 列）
/// 则一次性 drop 重建（sync 随后从磁盘重建）。
pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        let mut has_content = false;
        let mut has_plugin_id = false;
        let mut has_team_id = false;
        let mut has_expert_id = false;
        let mut has_group_id = false;
        {
            let mut stmt = c.prepare("pragma table_info(skills)")?;
            let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for col in cols {
                match col?.as_str() {
                    "content" => has_content = true,
                    "plugin_id" => has_plugin_id = true,
                    "team_id" => has_team_id = true,
                    "expert_id" => has_expert_id = true,
                    "group_id" => has_group_id = true,
                    _ => {}
                }
            }
        }
        // 表存在但 schema 过旧（有 content / 缺 plugin_id / 缺 team_id / 缺 expert_id）→ drop 重建
        // （sync 随后从磁盘重建散装/plugin/team skill；agent 私有 skill 由导入重建。drop 安全）。
        let table_exists: bool = c
            .query_row(
                "select 1 from sqlite_master where type='table' and name='skills'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if table_exists && (has_content || !has_plugin_id || !has_team_id || !has_expert_id) {
            c.execute("drop table if exists skills", [])?;
        }
        c.execute_batch(
            "create table if not exists skills (
                id             text primary key,
                source         text not null,
                name           text not null,
                description    text not null,
                dir_name       text not null,
                enabled        integer not null default 1,
                installed_at   text not null,
                updated_at     text not null,
                plugin_id      text not null default '',
                team_id        text not null default '',
                expert_id       text not null default '',
                user_invocable integer not null default 1,
                argument_hint  text,
                group_id       text,
                unique(plugin_id, team_id, expert_id, name)
            );
            create index if not exists idx_skills_enabled_name on skills(enabled, name);",
        )?;
        // 「我的」分组列（不在磁盘）：现存当前 schema 的表缺该列时补上（drop 重建路径已含）。
        if table_exists && !has_content && has_plugin_id && has_team_id && !has_group_id {
            c.execute_batch("alter table skills add column group_id text;")?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// upsert（按 `(plugin_id, name)` 冲突）：新增插入；已存在则更新
/// source/description/dir_name/updated_at/user_invocable/argument_hint，
/// **保留 enabled 与 id 与 installed_at**（sync 不覆盖用户启停与首次安装时间）。
pub fn upsert(db: &AppDatabase, r: &SkillRecord) -> Result<(), String> {
    let plugin_id = r.plugin_id.as_deref().unwrap_or(STANDALONE);
    let team_id = r.team_id.as_deref().unwrap_or(STANDALONE);
    let expert_id = r.expert_id.as_deref().unwrap_or(STANDALONE);
    db.with_connection(|c| {
        c.execute(
            "insert into skills
               (id, source, name, description, dir_name, enabled, installed_at, updated_at,
                plugin_id, team_id, expert_id, user_invocable, argument_hint, group_id)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             on conflict(plugin_id, team_id, expert_id, name) do update set
               source = excluded.source,
               description = excluded.description,
               dir_name = excluded.dir_name,
               updated_at = excluded.updated_at,
               user_invocable = excluded.user_invocable,
               argument_hint = excluded.argument_hint",
            // 注意：group_id 不在 on-conflict 更新内——sync 从磁盘重建时保留用户已设分组。
            params![
                r.id,
                r.source.as_str(),
                r.name,
                r.description,
                r.dir_name,
                if r.enabled { 1 } else { 0 },
                r.installed_at,
                r.updated_at,
                plugin_id,
                team_id,
                expert_id,
                if r.user_invocable { 1 } else { 0 },
                r.argument_hint,
                r.group_id,
            ],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 切换某技能启用状态。
pub fn set_enabled(db: &AppDatabase, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update skills set enabled = ?1, updated_at = ?2 where id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 删除某技能索引行。
pub fn delete(db: &AppDatabase, id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from skills where id = ?1", params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 删除某插件下的全部 skill 索引行（卸载插件时级联）。
pub fn delete_by_plugin(db: &AppDatabase, plugin_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "delete from skills where plugin_id = ?1",
            params![plugin_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

const COLS: &str = "id, source, name, description, dir_name, enabled, installed_at, updated_at, plugin_id, team_id, expert_id, user_invocable, argument_hint, group_id";

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillRecord> {
    let source: String = row.get(1)?;
    let enabled: i64 = row.get(5)?;
    let plugin_id: String = row.get(8)?;
    let team_id: String = row.get(9)?;
    let expert_id: String = row.get(10)?;
    let user_invocable: i64 = row.get(11)?;
    Ok(SkillRecord {
        id: row.get(0)?,
        source: SkillSource::from_str(&source),
        name: row.get(2)?,
        description: row.get(3)?,
        dir_name: row.get(4)?,
        enabled: enabled != 0,
        installed_at: row.get(6)?,
        updated_at: row.get(7)?,
        plugin_id: if plugin_id.is_empty() {
            None
        } else {
            Some(plugin_id)
        },
        team_id: if team_id.is_empty() {
            None
        } else {
            Some(team_id)
        },
        expert_id: if expert_id.is_empty() {
            None
        } else {
            Some(expert_id)
        },
        user_invocable: user_invocable != 0,
        argument_hint: row.get(12)?,
        group_id: row.get(13)?,
    })
}

/// 设置技能的「我的」分组（None=移出）。
pub fn set_group(db: &AppDatabase, id: &str, group_id: Option<&str>) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update skills set group_id = ?1 where id = ?2",
            params![group_id, id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 把某分组下所有技能归零（删除分组时调用）。
pub fn clear_group(db: &AppDatabase, group_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "update skills set group_id = null where group_id = ?1",
            params![group_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 列出全部技能（按 name 升序）。
pub fn list(db: &AppDatabase) -> Result<Vec<SkillRecord>, String> {
    query_many(db, &format!("select {COLS} from skills order by name"), [])
}

/// 列出**全局**启用且可见的技能（散装 + plugin 提供，即 team_id=''；按 name 升序）。
/// team 私有 skill（team_id 非空）不进默认池——仅其 team 被选中时由引擎按 `list_enabled_by_team` 追加。
pub fn list_enabled(db: &AppDatabase) -> Result<Vec<SkillRecord>, String> {
    query_many(
        db,
        &format!(
            "select {COLS} from skills where team_id = '' and expert_id = '' and enabled = 1 and user_invocable = 1 order by name"
        ),
        [],
    )
}

/// 列出某 agent 的私有可见技能（expert_id 非空 + 启用 + 可见）；供引擎在激活/派发该 agent 时追加入池。
pub fn list_enabled_by_expert(
    db: &AppDatabase,
    expert_id: &str,
) -> Result<Vec<SkillRecord>, String> {
    query_many(
        db,
        &format!(
            "select {COLS} from skills where expert_id = ?1 and enabled = 1 and user_invocable = 1 order by name"
        ),
        params![expert_id],
    )
}

/// 列出某 agent 的全部私有 skill（含隐藏，供详情）。
pub fn list_by_expert(db: &AppDatabase, expert_id: &str) -> Result<Vec<SkillRecord>, String> {
    query_many(
        db,
        &format!("select {COLS} from skills where expert_id = ?1 order by name"),
        params![expert_id],
    )
}

/// 删除某 agent 的全部私有 skill 索引行（agent 删除时级联）。
pub fn delete_by_agent(db: &AppDatabase, expert_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute(
            "delete from skills where expert_id = ?1",
            params![expert_id],
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 列出某 team 的私有可见技能（team_id 非空 + 启用 + 可见）；供引擎在选中该 team 时追加入池。
pub fn list_enabled_by_team(db: &AppDatabase, team_id: &str) -> Result<Vec<SkillRecord>, String> {
    query_many(
        db,
        &format!(
            "select {COLS} from skills where team_id = ?1 and enabled = 1 and user_invocable = 1 order by name"
        ),
        params![team_id],
    )
}

/// 列出某 team 的全部私有 skill（含隐藏，供详情）。
pub fn list_by_team(db: &AppDatabase, team_id: &str) -> Result<Vec<SkillRecord>, String> {
    query_many(
        db,
        &format!("select {COLS} from skills where team_id = ?1 order by name"),
        params![team_id],
    )
}

/// 删除某 team 的全部私有 skill 索引行（team 删除时级联）。
pub fn delete_by_team(db: &AppDatabase, team_id: &str) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute("delete from skills where team_id = ?1", params![team_id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 列出某插件下的全部 skill。
pub fn list_by_plugin(db: &AppDatabase, plugin_id: &str) -> Result<Vec<SkillRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from skills where plugin_id = ?1 order by name");
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(params![plugin_id], row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

fn query_many<P: rusqlite::Params>(
    db: &AppDatabase,
    sql: &str,
    params: P,
) -> Result<Vec<SkillRecord>, String> {
    db.with_connection(|c| {
        let mut stmt = c.prepare(sql)?;
        let rows = stmt.query_map(params, row_to_record)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .map_err(|e| e.to_string())
}

/// 按 id 取技能行。
pub fn get_by_id(db: &AppDatabase, id: &str) -> Result<Option<SkillRecord>, String> {
    get_one(db, "id = ?1", id)
}

/// 按 name 取**散装** skill 行（plugin_id='' 且 team_id=''）。插件/团队内 skill 各用其 owner 查询。
pub fn get_by_name(db: &AppDatabase, name: &str) -> Result<Option<SkillRecord>, String> {
    get_one(db, "name = ?1 and plugin_id = '' and team_id = ''", name)
}

/// 按 (plugin_id, name) 取插件内 skill 行。
pub fn get_by_plugin_and_name(
    db: &AppDatabase,
    plugin_id: &str,
    name: &str,
) -> Result<Option<SkillRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from skills where plugin_id = ?1 and name = ?2");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map(params![plugin_id, name], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

/// 按 name 取任意启用的技能（散装优先，其次任意启用的插件内同名）。供 load_skill 解析。
pub fn get_enabled_by_name_any(
    db: &AppDatabase,
    name: &str,
) -> Result<Option<SkillRecord>, String> {
    db.with_connection(|c| {
        // plugin_id='' 排前（散装优先），再按 name。
        let sql = format!(
            "select {COLS} from skills where name = ?1 and enabled = 1 \
             order by case when plugin_id = '' then 0 else 1 end limit 1"
        );
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map(params![name], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}

fn get_one(db: &AppDatabase, where_clause: &str, val: &str) -> Result<Option<SkillRecord>, String> {
    db.with_connection(|c| {
        let sql = format!("select {COLS} from skills where {where_clause}");
        let mut stmt = c.prepare(&sql)?;
        let mut rows = stmt.query_map([val], row_to_record)?;
        Ok(match rows.next() {
            Some(r) => Some(r?),
            None => None,
        })
    })
    .map_err(|e| e.to_string())
}
