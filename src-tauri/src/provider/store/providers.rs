//! ProviderStore：厂商 CRUD（含密钥写入/清理、健康检查触发）。
use super::{new_id, provider_from_row, ProviderStore};
use crate::provider::model::{ProviderCheckResult, ProviderInput, ProviderView};
use rusqlite::{params, OptionalExtension};

impl ProviderStore {
    // ---- 厂商 CRUD ----
    pub fn list_providers(&self) -> Result<Vec<ProviderView>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, name, base_url, has_secret, secret_hint, enabled,
                            last_check_status, last_check_detail, last_check_at, sort_order, protocol
                     from providers order by sort_order asc, name asc, id asc",
                )?;
                let rows = stmt.query_map([], provider_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    pub(super) fn get_provider(&self, id: &str) -> Result<Option<ProviderView>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select id, name, base_url, has_secret, secret_hint, enabled,
                                last_check_status, last_check_detail, last_check_at, sort_order, protocol
                         from providers where id = ?1",
                        [id],
                        provider_from_row,
                    )
                    .optional()?;
                Ok(row)
            })
            .map_err(|e| e.to_string())
    }

    pub fn upsert_provider(&self, input: ProviderInput, now: &str) -> Result<ProviderView, String> {
        let name = input.name.trim();
        let base_url = input.base_url.trim();
        if name.is_empty() || base_url.is_empty() {
            return Err("厂商名与 Base URL 不能为空".into());
        }
        let id = input.id.clone().unwrap_or_else(|| new_id("prov"));
        // 密钥：None=保持，空串=清除，非空=设置。
        match input.api_key.as_deref() {
            Some("") => self.secrets.clear(&id)?,
            Some(secret) => self.secrets.set(&id, secret)?,
            None => {}
        }
        let has_secret = self.secrets.has_secret(&id);
        let secret_hint = self.secrets.hint(&id);
        let enabled = input.enabled as i64;
        let protocol = {
            let p = input.protocol.trim().to_ascii_lowercase();
            if p.is_empty() { "openai".to_string() } else { p }
        };
        self.db
            .with_connection(|c| {
                // 新建时 sort_order 取当前最大值 +1。
                let sort: i64 = c.query_row(
                    "select coalesce(max(sort_order), -1) + 1 from providers",
                    [],
                    |r| r.get(0),
                )?;
                c.execute(
                    "insert into providers (id, name, base_url, has_secret, secret_hint, enabled, protocol, sort_order, updated_at)
                     values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     on conflict(id) do update set
                        name = excluded.name,
                        base_url = excluded.base_url,
                        has_secret = excluded.has_secret,
                        secret_hint = excluded.secret_hint,
                        enabled = excluded.enabled,
                        protocol = excluded.protocol,
                        updated_at = excluded.updated_at",
                    params![id, name, base_url, has_secret, secret_hint, enabled, protocol, sort, now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.get_provider(&id)?
            .ok_or_else(|| "provider just upserted not found".into())
    }

    pub fn set_provider_enabled(&self, id: &str, enabled: bool, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update providers set enabled = ?1, updated_at = ?2 where id = ?3",
                    params![enabled as i64, now, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub fn delete_provider(&self, id: &str) -> Result<(), String> {
        // 所有 DB 变更在一个事务内原子完成：清 fallback 设置（若指向本厂商模型）→
        // 清 sessions.selected_model_id → 删 provider_models → 删 providers。
        // 密钥文件删除（文件 IO）放在事务提交后。
        self.db
            .with_transaction(|tx| {
                // 本厂商下所有模型 id。
                let model_ids: Vec<String> = {
                    let mut stmt =
                        tx.prepare("select id from provider_models where provider_id = ?1")?;
                    let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
                    let mut v = Vec::new();
                    for r in rows {
                        v.push(r?);
                    }
                    v
                };
                // fallback 设置若指向其中任一模型则清除。
                let fallback: Option<String> = tx
                    .query_row(
                        "select value from provider_settings where key = 'fallback_model_id'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .optional()?;
                if let Some(fb) = fallback {
                    if model_ids.iter().any(|m| m == &fb) {
                        tx.execute(
                            "delete from provider_settings where key = 'fallback_model_id'",
                            [],
                        )?;
                    }
                }
                // sessions 表由 SessionStore 建；缺失时（如独立单测库）跳过，不报错。
                let has_sessions: i64 = tx.query_row(
                    "select count(*) from sqlite_master where type='table' and name='sessions'",
                    [],
                    |r| r.get(0),
                )?;
                if has_sessions > 0 {
                    for mid in &model_ids {
                        tx.execute(
                            "update sessions set selected_model_id = null where selected_model_id = ?1",
                            [mid],
                        )?;
                    }
                }
                tx.execute("delete from provider_models where provider_id = ?1", [id])?;
                tx.execute("delete from providers where id = ?1", [id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.secrets.clear(id)?;
        Ok(())
    }

    pub fn check_provider(&self, id: &str, now: &str) -> Result<ProviderCheckResult, String> {
        let result = match self.fetch_models(id) {
            Ok(models) => ProviderCheckResult {
                status: "ready".into(),
                detail: format!("连通成功，{} 个模型可用", models.len()),
                checked_at: now.into(),
            },
            Err(e) => ProviderCheckResult {
                status: "error".into(),
                detail: e,
                checked_at: now.into(),
            },
        };
        self.db
            .with_connection(|c| {
                c.execute(
                    "update providers set last_check_status = ?1, last_check_detail = ?2, last_check_at = ?3 where id = ?4",
                    params![result.status, result.detail, result.checked_at, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(result)
    }
}
