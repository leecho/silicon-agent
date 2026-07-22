//! 「我的」用户自定义分组：把已添加/自建的专家或团队归类。
//!
//! 分组按 `kind`（agent|team）各自独立；成员归属存于 agents/teams 表的 `group_id` 列（单分组）。
//! 本模块只管分组本身（建/列/改名/删）；归属变更由各自 store 的 `set_group` 负责。

use serde::Serialize;
use std::sync::Arc;

use crate::session::new_id;
use crate::storage::AppDatabase;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    /// "agent" | "team"。
    pub kind: String,
    pub name: String,
    pub sort: i64,
    pub created_at: String,
}

fn now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
        .to_string()
}

pub fn ensure_schema(db: &AppDatabase) -> Result<(), String> {
    db.with_connection(|c| {
        c.execute_batch(
            "create table if not exists groups (
                id          text primary key,
                kind        text not null,
                name        text not null,
                sort        integer not null default 0,
                created_at  text not null
            );
            create index if not exists idx_groups_kind on groups(kind, sort);",
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Group> {
    Ok(Group {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        sort: r.get(3)?,
        created_at: r.get(4)?,
    })
}

/// 分组服务：薄封装，供命令调用。
pub struct GroupService {
    db: Arc<AppDatabase>,
}

impl GroupService {
    pub fn new(db: Arc<AppDatabase>) -> Self {
        let _ = ensure_schema(&db);
        Self { db }
    }

    /// 列出某类型（agent|team）的全部分组，按 sort、created_at 升序。
    pub fn list(&self, kind: &str) -> Result<Vec<Group>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, kind, name, sort, created_at from groups where kind = ?1 order by sort, created_at",
                )?;
                let rows = stmt.query_map([kind], row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 新建分组（name 不能为空；同 kind 下同名拒绝）。
    pub fn create(&self, kind: &str, name: &str) -> Result<Group, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("分组名不能为空".into());
        }
        if kind != "agent" && kind != "team" && kind != "skill" {
            return Err("分组类型非法".into());
        }
        let exists = self.list(kind)?.into_iter().any(|g| g.name == name);
        if exists {
            return Err("同名分组已存在".into());
        }
        let g = Group {
            id: new_id("group"),
            kind: kind.to_string(),
            name: name.to_string(),
            sort: self.list(kind)?.len() as i64,
            created_at: now(),
        };
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into groups (id, kind, name, sort, created_at) values (?1,?2,?3,?4,?5)",
                    rusqlite::params![g.id, g.kind, g.name, g.sort, g.created_at],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(g)
    }

    /// 重命名分组。
    pub fn rename(&self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("分组名不能为空".into());
        }
        self.db
            .with_connection(|c| {
                c.execute(
                    "update groups set name = ?1 where id = ?2",
                    rusqlite::params![name, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除分组（成员归零由调用方在 agents/teams 表 clear_group 处理）。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from groups where id = ?1", rusqlite::params![id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Arc<AppDatabase> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let p = std::env::temp_dir().join(format!("siw-group-{}-{}.db", std::process::id(), nanos));
        let _ = std::fs::remove_file(&p);
        Arc::new(AppDatabase::open(&p).unwrap())
    }

    #[test]
    fn create_list_rename_delete() {
        let svc = GroupService::new(db());
        let g = svc.create("agent", "投研常用").expect("create");
        assert_eq!(g.kind, "agent");
        // 团队分组独立，不串。
        svc.create("team", "投研常用").expect("team same name ok");
        assert_eq!(svc.list("agent").unwrap().len(), 1);
        assert_eq!(svc.list("team").unwrap().len(), 1);
        // 同 kind 同名拒绝。
        assert!(svc.create("agent", "投研常用").is_err());
        svc.rename(&g.id, "投研").expect("rename");
        assert_eq!(svc.list("agent").unwrap()[0].name, "投研");
        svc.delete(&g.id).expect("delete");
        assert!(svc.list("agent").unwrap().is_empty());
    }
}
