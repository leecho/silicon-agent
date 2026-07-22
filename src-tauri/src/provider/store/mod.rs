//! 多模型 Provider Gateway：厂商/模型持久化（CRUD、设置、解析）。
//! 模型调用行为（`impl ModelClient`）见 `provider::gateway`。

use std::sync::Arc;

use rusqlite::{params, OptionalExtension};

use super::model::{ModelView, ProviderCheckResult, ProviderView};
use super::secret::FileSecretStore;
use crate::storage::{AppDatabase, StorageError};

// 按职责拆分的 `impl ProviderStore`（同模块子文件，共享私有 db/secrets 字段）。
mod models;
mod providers;
#[cfg(test)]
mod tests;

/// Provider Gateway 是厂商/模型配置的唯一事实 owner。
pub struct ProviderStore {
    db: Arc<AppDatabase>,
    secrets: FileSecretStore,
}

impl ProviderStore {
    /// `data_dir`：应用数据目录。密钥表存 `data_dir/provider.secrets.json`；
    /// 迁移期读取旧单密钥文件 `data_dir/provider.secret`。
    pub fn open(
        db: Arc<AppDatabase>,
        data_dir: impl AsRef<std::path::Path>,
    ) -> Result<Self, String> {
        let dir = data_dir.as_ref().to_path_buf();
        let gateway = Self {
            db,
            secrets: FileSecretStore::new(dir.join("provider.secrets.json")),
        };
        gateway.ensure_schema().map_err(|err| err.to_string())?;
        gateway.migrate_legacy(&dir)?;
        Ok(gateway)
    }

    fn ensure_schema(&self) -> Result<(), StorageError> {
        self.db.with_connection(|connection| {
            connection.execute_batch(
                "
                create table if not exists providers (
                    id text primary key,
                    name text not null,
                    base_url text not null,
                    has_secret integer not null default 0,
                    secret_hint text,
                    enabled integer not null default 1,
                    last_check_status text,
                    last_check_detail text,
                    last_check_at text,
                    sort_order integer not null default 0,
                    updated_at text not null
                );
                create table if not exists provider_models (
                    id text primary key,
                    provider_id text not null,
                    model text not null,
                    display_name text,
                    enabled integer not null default 1,
                    is_default integer not null default 0,
                    sort_order integer not null default 0,
                    updated_at text not null
                );
                create index if not exists idx_provider_models_provider
                    on provider_models(provider_id, sort_order, id);
                create table if not exists provider_settings (
                    key text primary key,
                    value text not null
                );
                ",
            )?;
            Ok(())
        })?;
        // 既有库幂等补列：provider_models.context_limit（每模型上下文上限覆盖，NULL=用内置表）。
        self.db.with_connection(|c| {
            let has: i64 = c.query_row(
                "select count(*) from pragma_table_info('provider_models') where name = 'context_limit'",
                [],
                |r| r.get(0),
            )?;
            if has == 0 {
                c.execute(
                    "alter table provider_models add column context_limit integer",
                    [],
                )?;
            }
            Ok(())
        })?;
        // 既有库幂等补列：provider_models.supports_vision（每模型 vision 能力覆盖，NULL=用内置表）。
        self.db.with_connection(|c| {
            let has: i64 = c.query_row(
                "select count(*) from pragma_table_info('provider_models') where name = 'supports_vision'",
                [],
                |r| r.get(0),
            )?;
            if has == 0 {
                c.execute(
                    "alter table provider_models add column supports_vision integer",
                    [],
                )?;
            }
            Ok(())
        })?;
        // 既有库幂等补列：providers.protocol（调用协议，默认 openai）。
        self.db.with_connection(|c| {
            let has: i64 = c.query_row(
                "select count(*) from pragma_table_info('providers') where name = 'protocol'",
                [],
                |r| r.get(0),
            )?;
            if has == 0 {
                c.execute(
                    "alter table providers add column protocol text not null default 'openai'",
                    [],
                )?;
            }
            Ok(())
        })?;
        // 一次性迁移：旧版本把 provider 的 fallback_model_id 存在跨模块共享表 app_settings 里；
        // 现 fallback 设置归 provider 独占的 provider_settings。若旧表存在且本表尚无该键，则搬过来
        // （insert or ignore，幂等）。app_settings 现由 AppSettingsStore 独占，此处只读旧值、不再写它。
        self.db.with_connection(|c| {
            let has_legacy: i64 = c.query_row(
                "select count(*) from sqlite_master where type='table' and name='app_settings'",
                [],
                |r| r.get(0),
            )?;
            if has_legacy > 0 {
                c.execute(
                    "insert or ignore into provider_settings (key, value)
                     select key, value from app_settings where key = 'fallback_model_id'",
                    [],
                )?;
            }
            Ok(())
        })
    }

    /// 一次性迁移旧单行 `provider_config` 到新表。无旧表 / 无 default 行则跳过。
    fn migrate_legacy(&self, data_dir: &std::path::Path) -> Result<(), String> {
        let has_legacy = self
            .db
            .with_connection(|c| {
                let n: i64 = c.query_row(
                    "select count(*) from sqlite_master where type='table' and name='provider_config'",
                    [],
                    |r| r.get(0),
                )?;
                Ok(n > 0)
            })
            .map_err(|e| e.to_string())?;
        if !has_legacy {
            return Ok(());
        }
        // 已迁移过（新表已有厂商）则只清理旧表。
        let already: i64 = self
            .db
            .with_connection(|c| {
                Ok(c.query_row("select count(*) from providers", [], |r| r.get(0))?)
            })
            .map_err(|e| e.to_string())?;
        if already == 0 {
            let legacy = self
                .db
                .with_connection(|c| {
                    let row = c
                        .query_row(
                            "select provider, base_url, model, fallback_model from provider_config where id='default'",
                            [],
                            |r| {
                                Ok((
                                    r.get::<_, String>(0)?,
                                    r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?,
                                    r.get::<_, Option<String>>(3)?,
                                ))
                            },
                        )
                        .optional()?;
                    Ok(row)
                })
                .map_err(|e| e.to_string())?;
            if let Some((provider, base_url, model, fallback)) = legacy {
                let now = crate::app_state::now_string();
                // 旧单密钥文件 → 新分键存储。先读明文（文件 IO 必须在事务外）。
                let legacy_secret = data_dir.join("provider.secret");
                let api_key = std::fs::read_to_string(&legacy_secret)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                // 事务前生成所有 id。new_id 基于纳秒，连续调用可能撞号，故 fallback 用确定性后缀避开默认模型 id。
                let provider_id = new_id("prov");
                let default_model_id = new_id("mdl");
                let fallback = fallback.filter(|s| !s.trim().is_empty());
                let fallback_model_id = fallback.as_ref().map(|_| format!("{default_model_id}-fb"));
                // 整行导入（厂商 + 默认模型 + 可选 fallback 模型 + 默认/fallback 设置）放进一个事务，
                // 全部成功才提交；任一步失败回滚，旧表不删，下次仍可重试。
                self.db
                    .with_transaction(|tx| {
                        let has_secret = api_key.is_some() as i64;
                        let secret_hint = api_key.as_deref().map(secret_hint_for);
                        tx.execute(
                            "insert into providers (id, name, base_url, has_secret, secret_hint, enabled, sort_order, updated_at)
                             values (?1, ?2, ?3, ?4, ?5, 1, 0, ?6)",
                            params![provider_id, provider.trim(), base_url.trim(), has_secret, secret_hint, now],
                        )?;
                        tx.execute(
                            "insert into provider_models (id, provider_id, model, display_name, enabled, is_default, sort_order, updated_at)
                             values (?1, ?2, ?3, null, 1, 1, 0, ?4)",
                            params![default_model_id, provider_id, model.trim(), now],
                        )?;
                        if let (Some(fb), Some(fb_id)) = (fallback.as_ref(), fallback_model_id.as_ref()) {
                            tx.execute(
                                "insert into provider_models (id, provider_id, model, display_name, enabled, is_default, sort_order, updated_at)
                                 values (?1, ?2, ?3, null, 1, 0, 1, ?4)",
                                params![fb_id, provider_id, fb.trim(), now],
                            )?;
                            tx.execute(
                                "insert into provider_settings (key, value) values ('fallback_model_id', ?1)
                                 on conflict(key) do update set value = excluded.value",
                                params![fb_id],
                            )?;
                        }
                        Ok(())
                    })
                    .map_err(|e| e.to_string())?;
                // 事务提交成功后：写入密钥（文件 IO），删除旧密钥文件，再删旧表。
                if let Some(key) = api_key {
                    self.secrets.set(&provider_id, &key)?;
                }
                let _ = std::fs::remove_file(&legacy_secret);
            }
        }
        // 删除旧表（仅在成功导入 / 无可导入 / 已迁移后到达此处）。
        self.db
            .with_connection(|c| {
                c.execute("drop table if exists provider_config", [])?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- provider_settings（provider 独占的 app 级 KV：目前仅 fallback_model_id）----
    pub(in crate::provider) fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.db
            .with_connection(|c| {
                Ok(c.query_row(
                    "select value from provider_settings where key = ?1",
                    [key],
                    |r| r.get::<_, String>(0),
                )
                .optional()?)
            })
            .map_err(|e| e.to_string())
    }

    pub(super) fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into provider_settings (key, value) values (?1, ?2)
                     on conflict(key) do update set value = excluded.value",
                    params![key, value],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    pub(super) fn clear_setting(&self, key: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from provider_settings where key = ?1", [key])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }
}

/// 生成带前缀的随机 id（复用 session::new_id 风格）。
pub(super) fn new_id(prefix: &str) -> String {
    crate::session::new_id(prefix)
}

/// 密钥掩码提示（与 FileSecretStore::hint 一致）：`****{末4位}`。
pub(super) fn secret_hint_for(secret: &str) -> String {
    let suffix: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("****{suffix}")
}

pub(super) fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderView> {
    let enabled: i64 = row.get(5)?;
    let last_status: Option<String> = row.get(6)?;
    let last_detail: Option<String> = row.get(7)?;
    let last_at: Option<String> = row.get(8)?;
    let last_check = last_status.map(|status| ProviderCheckResult {
        status,
        detail: last_detail.unwrap_or_default(),
        checked_at: last_at.unwrap_or_default(),
    });
    Ok(ProviderView {
        id: row.get(0)?,
        name: row.get(1)?,
        base_url: row.get(2)?,
        has_secret: row.get::<_, i64>(3)? != 0,
        secret_hint: row.get(4)?,
        enabled: enabled != 0,
        last_check,
        sort_order: row.get(9)?,
        protocol: row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "openai".into()),
    })
}

pub(super) fn model_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelView> {
    let enabled: i64 = row.get(4)?;
    let is_default: i64 = row.get(5)?;
    let model: String = row.get(2)?;
    let supports_vision = row.get::<_, Option<i64>>(8)?.map(|v| v != 0);
    let vision_capable =
        crate::provider::model::resolved_supports_vision(&model, supports_vision);
    Ok(ModelView {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        model,
        display_name: row.get(3)?,
        enabled: enabled != 0,
        is_default: is_default != 0,
        sort_order: row.get(6)?,
        context_limit: row.get(7)?,
        supports_vision,
        vision_capable,
    })
}
