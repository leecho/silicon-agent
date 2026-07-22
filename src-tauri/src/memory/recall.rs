//! 检索式召回：Tier1（画像/置顶，始终注入）+ Tier2（fact，FTS5 trigram top-K）。
//!
//! 替换「全量注入」：按当前轮 query 的相关性挑选记忆，规模友好、相关优先。
//! 见 spec §3.5。FTS 查询构造 `build_fts_match` 为纯函数，单测覆盖。

use std::collections::HashSet;

use crate::memory::types::{Memory, MemoryScope};
use crate::memory::MemoryStore;

fn memory_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        content: r.get(1)?,
        created_at: r.get(2)?,
    })
}

/// CJK 字符（含基本区 + 扩展 A）。trigram 分词对中文按字符滑窗匹配，故按字符提取三元组。
fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch) || ('\u{3400}'..='\u{4dbf}').contains(&ch)
}

fn flush_latin(buf: &mut String, terms: &mut Vec<String>) {
    if buf.chars().count() >= 3 {
        terms.push(buf.to_lowercase());
    }
    buf.clear();
}

fn flush_cjk(buf: &mut String, terms: &mut Vec<String>) {
    let chars: Vec<char> = buf.chars().collect();
    if chars.len() >= 3 {
        for w in chars.windows(3) {
            terms.push(w.iter().collect());
        }
    }
    buf.clear();
}

/// 把自然语言 query 构造成安全的 FTS5（trigram）MATCH 表达式：
/// - 拉丁词：长度 ≥3 的连续字母数字串，小写。
/// - 中文：CJK 连续串的 3 字符滑窗（trigram 至少 3 字符）。
/// 每个 term 用双引号包裹（FTS5 字符串字面量，内部双引号转义为两个），以 OR 连接，最多 24 个。
/// 无可用 term（query 过短）则返回 None，调用方回退到「最近 fact」。
pub fn build_fts_match(query: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    let mut latin = String::new();
    let mut cjk = String::new();
    for ch in query.chars() {
        if is_cjk(ch) {
            flush_latin(&mut latin, &mut terms);
            cjk.push(ch);
        } else if ch.is_alphanumeric() && ch.is_ascii() {
            flush_cjk(&mut cjk, &mut terms);
            latin.push(ch);
        } else {
            flush_latin(&mut latin, &mut terms);
            flush_cjk(&mut cjk, &mut terms);
        }
    }
    flush_latin(&mut latin, &mut terms);
    flush_cjk(&mut cjk, &mut terms);

    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        return None;
    }
    terms.truncate(24);
    let expr = terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");
    Some(expr)
}

impl MemoryStore {
    /// 分层召回：Tier1（画像/置顶/tier=1，始终注入）+ Tier2（fact 的 FTS5 top-K，按 bm25）。
    /// query 为空或过短时 Tier2 回退到最近 `limit` 条 fact。结果按 Tier1 优先去重保序。
    /// `scope`：召回「全局 ∪ 当前作用域」——Global 仅全局；Project(p)/Agent(a) 并入对应层。
    pub fn recall(
        &self,
        query: &str,
        limit: usize,
        scope: MemoryScope<'_>,
    ) -> Result<Vec<Memory>, String> {
        let tier1 = self.list_tier1(scope)?;
        let tier2 = match build_fts_match(query) {
            Some(expr) => self.fts_recall_facts(&expr, limit, scope)?,
            None => self.recent_facts(limit, scope)?,
        };
        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<Memory> = Vec::new();
        for m in tier1.into_iter().chain(tier2.into_iter()) {
            if seen.insert(m.id.clone()) {
                out.push(m);
            }
        }
        Ok(out)
    }

    /// Tier1（事实侧）：置顶（pinned）+ 显式 tier=1 的 fact，始终注入。
    /// 画像（kind='profile'）走独立通道（`get_profile` + memory::prompt），不在此返回。
    fn list_tier1(&self, scope: MemoryScope<'_>) -> Result<Vec<Memory>, String> {
        let (frag, scope_val) = scope.predicate();
        let sql = format!(
            "select id, content, created_at from memories
             where kind != 'profile' and (pinned = 1 or tier = 1) and {frag}
             order by pinned desc, created_at, id"
        );
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&sql)?;
                let mut out = Vec::new();
                match &scope_val {
                    Some(v) => {
                        for r in
                            stmt.query_map(rusqlite::named_params! {":scope": v}, memory_from_row)?
                        {
                            out.push(r?);
                        }
                    }
                    None => {
                        for r in stmt.query_map([], memory_from_row)? {
                            out.push(r?);
                        }
                    }
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// Tier2：FTS5 trigram 召回 fact，按 bm25 相关性升序（bm25 越小越相关）取 top-K。
    fn fts_recall_facts(
        &self,
        match_expr: &str,
        limit: usize,
        scope: MemoryScope<'_>,
    ) -> Result<Vec<Memory>, String> {
        let (frag, scope_val) = scope.predicate();
        let sql = format!(
            "select memories.id, memories.content, memories.created_at
             from memories_fts join memories on memories.id = memories_fts.id
             where memories_fts match :expr and memories.kind = 'fact' and {frag}
             order by bm25(memories_fts) limit :limit"
        );
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&sql)?;
                let mut out = Vec::new();
                let lim = limit as i64;
                match &scope_val {
                    Some(v) => {
                        for r in stmt.query_map(
                            rusqlite::named_params! {":expr": match_expr, ":limit": lim, ":scope": v},
                            memory_from_row,
                        )? {
                            out.push(r?);
                        }
                    }
                    None => {
                        for r in stmt.query_map(
                            rusqlite::named_params! {":expr": match_expr, ":limit": lim},
                            memory_from_row,
                        )? {
                            out.push(r?);
                        }
                    }
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    /// 无可用 query 时回退：最近 `limit` 条 fact，按时间升序返回（阅读顺序）。
    fn recent_facts(&self, limit: usize, scope: MemoryScope<'_>) -> Result<Vec<Memory>, String> {
        let (frag, scope_val) = scope.predicate();
        let sql = format!(
            "select id, content, created_at from (
                select id, content, created_at from memories
                where kind = 'fact' and {frag}
                order by created_at desc, id desc limit :limit
             ) order by created_at, id"
        );
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(&sql)?;
                let mut out = Vec::new();
                let lim = limit as i64;
                match &scope_val {
                    Some(v) => {
                        for r in stmt.query_map(
                            rusqlite::named_params! {":limit": lim, ":scope": v},
                            memory_from_row,
                        )? {
                            out.push(r?);
                        }
                    }
                    None => {
                        for r in stmt
                            .query_map(rusqlite::named_params! {":limit": lim}, memory_from_row)?
                        {
                            out.push(r?);
                        }
                    }
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::build_fts_match;

    #[test]
    fn latin_tokens_min_len_three() {
        let m = build_fts_match("use Rust on CI").expect("some");
        assert!(m.contains("\"rust\""));
        // "CI"/"on" 长度<3 被忽略。
        assert!(!m.contains("\"ci\""));
    }

    #[test]
    fn cjk_trigrams() {
        let m = build_fts_match("用户喜欢简洁").expect("some");
        assert!(m.contains("\"用户喜\""));
        assert!(m.contains("\"户喜欢\""));
    }

    #[test]
    fn short_query_returns_none() {
        assert!(build_fts_match("a 的").is_none());
        assert!(build_fts_match("   ").is_none());
    }
}
