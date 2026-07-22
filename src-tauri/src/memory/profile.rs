//! 用户画像（对应 Hermes USER.md）：kind='profile' 的单例行，Tier1 常驻注入。
//!
//! 画像与普通事实分通道：画像是稳定的用户偏好/背景整段文本，由用户编辑或 W2 的主动整理
//! （curation）从近期记忆中抽取维护；普通事实走 FTS5 检索召回。

use crate::memory::MemoryStore;

/// 画像单例行的固定 id。整个库至多一条 kind='profile'。
const PROFILE_ID: &str = "mem-profile";

impl MemoryStore {
    /// 读取用户画像整段文本；不存在或为空则 None。
    pub fn get_profile(&self) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt =
                    c.prepare("select content from memories where kind = 'profile' limit 1")?;
                let mut rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                Ok(match rows.next() {
                    Some(r) => {
                        let s = r?;
                        if s.trim().is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                    None => None,
                })
            })
            .map_err(|e| e.to_string())
    }

    /// 写入/覆盖用户画像（单例 upsert）。空内容等同清空画像。
    pub fn set_profile(&self, content: &str, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                let exists: bool = {
                    let n: i64 = c.query_row(
                        "select count(*) from memories where kind = 'profile'",
                        [],
                        |r| r.get(0),
                    )?;
                    n > 0
                };
                if exists {
                    c.execute(
                        "update memories set content = ?1, updated_at = ?2 where kind = 'profile'",
                        rusqlite::params![content, now],
                    )?;
                } else {
                    c.execute(
                        "insert into memories (id, content, created_at, kind, tier, pinned, source, updated_at)
                         values (?1, ?2, ?3, 'profile', 1, 0, 'profile', ?3)",
                        rusqlite::params![PROFILE_ID, content, now],
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        // 画像不进 FTS（不参与检索召回，始终全量注入）。
        Ok(())
    }
}
