//! ProviderStore：模型 CRUD、默认/选择解析、fallback 设置、远端模型拉取。
use super::{model_from_row, new_id, ProviderStore};
use crate::provider::adapter::authorization_header_value;
use crate::provider::client::{ModelCallRequest, ProviderCallError};
use crate::provider::model::{EnabledProviderModels, ModelInput, ModelView, ResolvedModel};
use crate::provider::protocol::{CallTarget, Protocol};
use crate::storage::StorageError;
use rusqlite::{params, OptionalExtension};

impl ProviderStore {
    // ---- 模型 CRUD ----
    pub fn list_models(&self, provider_id: &str) -> Result<Vec<ModelView>, String> {
        self.db
            .with_connection(|c| {
                let mut stmt = c.prepare(
                    "select id, provider_id, model, display_name, enabled, is_default, sort_order, context_limit, supports_vision
                     from provider_models where provider_id = ?1
                     order by sort_order asc, model asc, id asc",
                )?;
                let rows = stmt.query_map([provider_id], model_from_row)?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r?);
                }
                Ok(out)
            })
            .map_err(|e| e.to_string())
    }

    fn get_model(&self, id: &str) -> Result<Option<ModelView>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select id, provider_id, model, display_name, enabled, is_default, sort_order, context_limit, supports_vision
                         from provider_models where id = ?1",
                        [id],
                        model_from_row,
                    )
                    .optional()?;
                Ok(row)
            })
            .map_err(|e| e.to_string())
    }

    pub fn upsert_model(&self, input: ModelInput, now: &str) -> Result<ModelView, String> {
        let model = input.model.trim();
        if model.is_empty() {
            return Err("模型名不能为空".into());
        }
        if self.get_provider(&input.provider_id)?.is_none() {
            return Err("所属厂商不存在".into());
        }
        let id = input.id.clone().unwrap_or_else(|| new_id("mdl"));
        let display = input
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        self.db
            .with_connection(|c| {
                let sort: i64 = c.query_row(
                    "select coalesce(max(sort_order), -1) + 1 from provider_models where provider_id = ?1",
                    [&input.provider_id],
                    |r| r.get(0),
                )?;
                c.execute(
                    "insert into provider_models (id, provider_id, model, display_name, enabled, is_default, sort_order, context_limit, supports_vision, updated_at)
                     values (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9)
                     on conflict(id) do update set
                        model = excluded.model,
                        display_name = excluded.display_name,
                        enabled = excluded.enabled,
                        context_limit = excluded.context_limit,
                        supports_vision = excluded.supports_vision,
                        updated_at = excluded.updated_at",
                    params![id, input.provider_id, model, display, input.enabled as i64, sort, input.context_limit, input.supports_vision.map(|b| b as i64), now],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.get_model(&id)?
            .ok_or_else(|| "model just upserted not found".into())
    }

    pub fn set_model_enabled(&self, id: &str, enabled: bool, now: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "update provider_models set enabled = ?1, updated_at = ?2 where id = ?3",
                    params![enabled as i64, now, id],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 设全局默认模型：先校验模型存在（否则报错回滚，不动现有默认），再清空所有 is_default 并置目标为 1。
    pub fn set_default_model(&self, id: &str, now: &str) -> Result<(), String> {
        self.db
            .with_transaction(|tx| {
                let exists: i64 = tx.query_row(
                    "select count(*) from provider_models where id = ?1",
                    [id],
                    |r| r.get(0),
                )?;
                if exists == 0 {
                    return Err(StorageError::TransactionFailed("模型不存在".into()));
                }
                tx.execute("update provider_models set is_default = 0", [])?;
                tx.execute(
                    "update provider_models set is_default = 1, updated_at = ?1 where id = ?2",
                    params![now, id],
                )?;
                Ok(())
            })
            .map_err(|e| match e {
                StorageError::TransactionFailed(msg) if msg == "模型不存在" => msg,
                other => other.to_string(),
            })
    }

    pub fn delete_model(&self, id: &str) -> Result<(), String> {
        self.clear_model_references(id)?;
        self.db
            .with_connection(|c| {
                c.execute("delete from provider_models where id = ?1", [id])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 清除某模型的全局引用：fallback 设置 + 会话选择（sessions 表同库直接操作）。
    fn clear_model_references(&self, model_id: &str) -> Result<(), String> {
        if self.get_setting("fallback_model_id")?.as_deref() == Some(model_id) {
            self.clear_setting("fallback_model_id")?;
        }
        self.db
            .with_connection(|c| {
                // sessions 表由 SessionStore 建；缺失时（如独立单测库）跳过，不报错。
                let has_sessions: i64 = c.query_row(
                    "select count(*) from sqlite_master where type='table' and name='sessions'",
                    [],
                    |r| r.get(0),
                )?;
                if has_sessions > 0 {
                    c.execute(
                        "update sessions set selected_model_id = null where selected_model_id = ?1",
                        [model_id],
                    )?;
                }
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ---- Composer 分组视图 ----
    pub fn list_enabled_models(&self) -> Result<Vec<EnabledProviderModels>, String> {
        let mut out = Vec::new();
        for p in self.list_providers()?.into_iter().filter(|p| p.enabled) {
            let models: Vec<ModelView> = self
                .list_models(&p.id)?
                .into_iter()
                .filter(|m| m.enabled)
                .collect();
            if !models.is_empty() {
                out.push(EnabledProviderModels {
                    provider_id: p.id,
                    provider_name: p.name,
                    models,
                });
            }
        }
        Ok(out)
    }

    /// 模型上下文窗口上限（token）：优先用该模型名配置的 `context_limit`（取首个非空覆盖），
    /// 否则回退内置查表 `model_context_limit`。供 context meter / 自动压缩阈值用。
    pub fn context_limit_for(&self, model_name: &str) -> i64 {
        let configured: Option<i64> = self
            .db
            .with_connection(|c| {
                let v = c
                    .query_row(
                        "select context_limit from provider_models
                         where model = ?1 and context_limit is not null
                         order by sort_order asc, id asc limit 1",
                        [model_name],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional()?;
                Ok(v)
            })
            .ok()
            .flatten();
        configured
            .filter(|v| *v > 0)
            .unwrap_or_else(|| crate::provider::model::model_context_limit(model_name))
    }

    /// 模型 vision 能力：优先用该模型名配置的 `supports_vision` 覆盖（取首个非空），
    /// 否则回退内置查表 `model_supports_vision`。供引擎装配消息时判定降级。
    pub fn supports_vision_for(&self, model_name: &str) -> bool {
        let configured: Option<i64> = self
            .db
            .with_connection(|c| {
                let v = c
                    .query_row(
                        "select supports_vision from provider_models
                         where model = ?1 and supports_vision is not null
                         order by sort_order asc, id asc limit 1",
                        [model_name],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional()?;
                Ok(v)
            })
            .ok()
            .flatten();
        crate::provider::model::resolved_supports_vision(model_name, configured.map(|v| v != 0))
    }

    // ---- fallback 设置 ----
    pub fn set_fallback_model(&self, model_id: Option<&str>) -> Result<(), String> {
        match model_id {
            Some(id) => self.set_setting("fallback_model_id", id),
            None => self.clear_setting("fallback_model_id"),
        }
    }

    pub fn get_fallback_model_id(&self) -> Result<Option<String>, String> {
        self.get_setting("fallback_model_id")
    }

    // ---- 解析 ----
    /// 解析全局默认（is_default=1 且自身启用、厂商启用）。
    fn default_resolved(&self) -> Result<Option<ResolvedModel>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select pm.id, pm.provider_id, p.name, pm.model
                         from provider_models pm join providers p on p.id = pm.provider_id
                         where pm.is_default = 1 and pm.enabled = 1 and p.enabled = 1
                         limit 1",
                        [],
                        |r| {
                            Ok(ResolvedModel {
                                model_id: r.get(0)?,
                                provider_id: r.get(1)?,
                                provider_name: r.get(2)?,
                                model: r.get(3)?,
                            })
                        },
                    )
                    .optional()?;
                Ok(row)
            })
            .map_err(|e| e.to_string())
    }

    fn resolved_by_id(&self, model_id: &str) -> Result<Option<ResolvedModel>, String> {
        self.db
            .with_connection(|c| {
                let row = c
                    .query_row(
                        "select pm.id, pm.provider_id, p.name, pm.model
                         from provider_models pm join providers p on p.id = pm.provider_id
                         where pm.id = ?1 and pm.enabled = 1 and p.enabled = 1",
                        [model_id],
                        |r| {
                            Ok(ResolvedModel {
                                model_id: r.get(0)?,
                                provider_id: r.get(1)?,
                                provider_name: r.get(2)?,
                                model: r.get(3)?,
                            })
                        },
                    )
                    .optional()?;
                Ok(row)
            })
            .map_err(|e| e.to_string())
    }

    /// 解析模型选择：给定 model_id 且可用则用之；否则（含失效）回退全局默认；都没有则 Err。
    pub fn resolve_selection(&self, model_id: Option<&str>) -> Result<ResolvedModel, String> {
        if let Some(id) = model_id {
            if let Some(r) = self.resolved_by_id(id)? {
                return Ok(r);
            }
        }
        self.default_resolved()?
            .ok_or_else(|| "未配置可用模型，请在设置中添加并启用模型，并设置默认模型。".into())
    }

    /// 据 request.model_selection（或默认）得到端点凭证 + 协议。
    pub(in crate::provider) fn call_target(
        &self,
        request: &ModelCallRequest,
    ) -> Result<CallTarget, ProviderCallError> {
        let selection = match &request.model_selection {
            Some(s) => s.clone(),
            None => self
                .resolve_selection(None)
                .map_err(ProviderCallError::new)?
                .selection(),
        };
        let provider = self
            .get_provider(&selection.provider_id)
            .map_err(ProviderCallError::new)?
            .ok_or_else(|| ProviderCallError::new("所选模型的厂商不存在"))?;
        let api_key = self
            .secrets
            .read(&selection.provider_id)
            .map_err(|_| ProviderCallError::new("该厂商未配置 API Key"))?;
        Ok(CallTarget {
            base_url: provider.base_url,
            api_key,
            model: selection.model,
            protocol: Protocol::from_str(&provider.protocol),
        })
    }

    // ---- 自动拉取模型列表 ----
    /// 拉取厂商可用模型名列表：OpenAI 走 {base}/models + Bearer；Anthropic 走 {base}/v1/models + x-api-key。
    pub fn fetch_models(&self, provider_id: &str) -> Result<Vec<String>, String> {
        let provider = self
            .get_provider(provider_id)?
            .ok_or_else(|| "厂商不存在".to_string())?;
        let api_key = self
            .secrets
            .read(provider_id)
            .map_err(|_| "该厂商未配置 API Key".to_string())?;
        let api_key = api_key.trim();
        let base = provider.base_url.trim().trim_end_matches('/');
        let (url, hdrs) = match Protocol::from_str(&provider.protocol) {
            Protocol::Anthropic => (
                format!("{base}/v1/models"),
                vec![
                    ("x-api-key".to_string(), api_key.to_string()),
                    ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ],
            ),
            Protocol::OpenAi => (
                format!("{base}/models"),
                vec![(
                    "Authorization".to_string(),
                    authorization_header_value(api_key),
                )],
            ),
        };
        let resp = crate::http::HttpClient::new()
            .send(
                crate::http::HttpRequest::get(url)
                    .headers(hdrs)
                    .timeout(std::time::Duration::from_secs(30)),
            )
            .map_err(|e| format!("拉取模型失败：{e:?}"))?;
        if !resp.is_success() {
            return Err(format!("拉取模型失败：HTTP {}", resp.status));
        }
        let response: serde_json::Value = resp
            .json()
            .map_err(|e| format!("模型列表不是合法 JSON：{e:?}"))?;
        let ids = response
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(ids)
    }
}
