//! app_settings owner 模块：拥有 `app_settings` 这张 app 级 key/value 配置表，
//! 以及其上的具象配置读写（全局权限模式、各开关、辅助模型 id 等）。
//!
//! 边界约定：
//! - 这里只承载「app 全局偏好」这一类设置；会话级字段（如会话级权限覆盖）仍归 `session` 模块。
//! - 不承载跨表/跨模块的组合逻辑（例如「生效权限模式 = 会话覆盖 ?? 全局默认」），
//!   该组合属于 `session::permission` 这类聚合点，避免本模块反向依赖会话语义。
//! - SQL 收敛在本文件：所有具象访问器都走私有 KV 原语，杜绝重复的 upsert/select 样板。

use std::sync::Arc;

use crate::storage::AppDatabase;

const DEFAULT_MAX_ITERATIONS: u32 = 24;
const MAX_ITERATIONS_MIN: u32 = 1;
const MAX_ITERATIONS_MAX: u32 = 100;
pub(crate) const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 30;
const TOOL_TIMEOUT_MIN: u64 = 1; // 0 由工具级 timeout_secs() 表达「不超时」，不经全局
const TOOL_TIMEOUT_MAX: u64 = 1800; // 30 分钟硬上限
pub(crate) const DEFAULT_TOOL_PARALLELISM: u64 = 8;
const TOOL_PARALLELISM_MIN: u64 = 1; // 1 = 退化为串行（逃生舱）
const TOOL_PARALLELISM_MAX: u64 = 32;
const DEFAULT_AUTO_COMPACT_THRESHOLD_PCT: u32 = 90;
const AUTO_COMPACT_THRESHOLD_MIN: u32 = 50;
const AUTO_COMPACT_THRESHOLD_MAX: u32 = 95;
const SUBAGENT_EXECUTION_MODE_PARALLEL: &str = "parallel";
const SUBAGENT_EXECUTION_MODE_SERIAL: &str = "serial";

/// app 全局配置存储。持有 db 句柄，构造时幂等建表。
pub struct AppSettingsStore {
    db: Arc<AppDatabase>,
}

impl AppSettingsStore {
    /// 打开存储并确保 `app_settings` 表存在（幂等）。
    pub fn open(db: Arc<AppDatabase>) -> Result<Self, String> {
        let store = Self { db };
        store.ensure_schema()?;
        Ok(store)
    }

    /// 幂等建表：单纯的 key/value，value 非空。
    fn ensure_schema(&self) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute_batch(
                    "create table if not exists app_settings (
                        key text primary key,
                        value text not null
                    );",
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ── KV 原语 ─────────────────────────────────────────────────────────────────
    // 全部具象访问器都建立在这三个原语之上，把 select/upsert/delete 的 SQL 收敛到一处。

    /// 读原始字符串值；缺失返回 None。
    fn get_raw(&self, key: &str) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.db
            .with_connection(|c| {
                let v: Option<String> = c
                    .query_row(
                        "select value from app_settings where key = ?1",
                        [key],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(v)
            })
            .map_err(|e| e.to_string())
    }

    /// 写原始字符串值（upsert）。
    fn set_raw(&self, key: &str, value: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute(
                    "insert into app_settings (key, value) values (?1, ?2)
                     on conflict(key) do update set value = excluded.value",
                    rusqlite::params![key, value],
                )?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    /// 删除某 key（用于「清除回退默认」语义）。
    fn delete_raw(&self, key: &str) -> Result<(), String> {
        self.db
            .with_connection(|c| {
                c.execute("delete from app_settings where key = ?1", [key])?;
                Ok(())
            })
            .map_err(|e| e.to_string())
    }

    // ── 类型化便捷层 ──────────────────────────────────────────────────────────────

    /// 读布尔开关：约定 "0" 为 false，其余非空值为 true；缺失用 `default`。
    fn get_bool(&self, key: &str, default: bool) -> Result<bool, String> {
        Ok(self.get_raw(key)?.map(|s| s != "0").unwrap_or(default))
    }

    /// 写布尔开关（存 "1" / "0"）。
    fn set_bool(&self, key: &str, value: bool) -> Result<(), String> {
        self.set_raw(key, if value { "1" } else { "0" })
    }

    /// 读字符串配置；缺失返回空串。
    fn get_str(&self, key: &str) -> Result<String, String> {
        Ok(self.get_raw(key)?.unwrap_or_default())
    }

    /// 写字符串配置。
    fn set_str(&self, key: &str, value: &str) -> Result<(), String> {
        self.set_raw(key, value)
    }

    // ── 具象访问器 ────────────────────────────────────────────────────────────────

    /// 读全局默认权限模式；未设置时返回 "manual"。
    pub fn get_global_permission_mode(&self) -> Result<String, String> {
        Ok(self
            .get_raw("permission_mode")?
            .unwrap_or_else(|| "manual".to_string()))
    }

    /// 写全局默认权限模式（upsert）。
    pub fn set_global_permission_mode(&self, mode: &str) -> Result<(), String> {
        self.set_raw("permission_mode", mode)
    }

    /// 是否在每轮结束后生成快捷建议（默认开）。
    pub fn get_suggestions_enabled(&self) -> Result<bool, String> {
        self.get_bool("suggestions_enabled", true)
    }

    /// 写「是否生成快捷建议」开关。
    pub fn set_suggestions_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("suggestions_enabled", enabled)
    }

    /// 自动压缩开关（缺省 = 开）。上一轮真实用量超阈值时，下一轮开跑前自动压缩较早历史。
    pub fn get_auto_compact_enabled(&self) -> Result<bool, String> {
        self.get_bool("auto_compact_enabled", true)
    }

    pub fn set_auto_compact_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("auto_compact_enabled", enabled)
    }

    /// 自动压缩阈值百分比（缺省 90，clamp 50..=95）。
    pub fn get_auto_compact_threshold_pct(&self) -> Result<u32, String> {
        let n = self
            .get_raw("auto_compact_threshold_pct")?
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_AUTO_COMPACT_THRESHOLD_PCT);
        Ok(n.clamp(AUTO_COMPACT_THRESHOLD_MIN, AUTO_COMPACT_THRESHOLD_MAX))
    }

    pub fn set_auto_compact_threshold_pct(&self, n: u32) -> Result<(), String> {
        let n = n.clamp(AUTO_COMPACT_THRESHOLD_MIN, AUTO_COMPACT_THRESHOLD_MAX);
        self.set_raw("auto_compact_threshold_pct", &n.to_string())
    }

    /// 是否显示已完成轮次的思考与执行过程（默认开）。
    pub fn get_show_completed_process(&self) -> Result<bool, String> {
        self.get_bool("show_completed_process", true)
    }

    pub fn set_show_completed_process(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("show_completed_process", enabled)
    }

    /// 模型调用日志开关（默认关）。开启后记录每次调用的完整请求/响应（含敏感内容）。
    pub fn get_model_call_log_enabled(&self) -> Result<bool, String> {
        self.get_bool("model_call_log_enabled", false)
    }

    pub fn set_model_call_log_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("model_call_log_enabled", enabled)
    }

    /// 桌面操作（computer use）总开关（默认关）。关闭时前端不暴露桌面操作入口，
    /// computer 工具不会被会话启用。开启需用户显式授权 + 系统辅助功能权限。
    pub fn get_computer_use_enabled(&self) -> Result<bool, String> {
        self.get_bool("computer_use_enabled", false)
    }

    pub fn set_computer_use_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("computer_use_enabled", enabled)
    }

    /// 浏览器操作总开关（默认关）。关闭时前端不暴露浏览器操作入口，browser 工具不会被会话启用。
    pub fn get_browser_use_enabled(&self) -> Result<bool, String> {
        self.get_bool("browser_use_enabled", false)
    }

    pub fn set_browser_use_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("browser_use_enabled", enabled)
    }

    /// 浏览器无头模式（默认关=有窗）。开启则不弹可见窗口，建议仅用于抓取/检索。
    pub fn get_browser_headless(&self) -> Result<bool, String> {
        self.get_bool("browser_headless", false)
    }

    pub fn set_browser_headless(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("browser_headless", enabled)
    }

    /// 浏览器空闲多久（分钟）自动关闭常驻窗口；0 = 不自动关。默认 10。
    pub fn get_browser_idle_close_min(&self) -> Result<u64, String> {
        Ok(self
            .get_raw("browser_idle_close_min")?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10))
    }

    pub fn set_browser_idle_close_min(&self, min: u64) -> Result<(), String> {
        self.set_raw("browser_idle_close_min", &min.to_string())
    }

    /// SessionPage 是否默认显示任务面板（默认开，保持既有界面行为）。
    pub fn get_session_task_panel_default_visible(&self) -> Result<bool, String> {
        self.get_bool("session_task_panel_default_visible", true)
    }

    pub fn set_session_task_panel_default_visible(&self, visible: bool) -> Result<(), String> {
        self.set_bool("session_task_panel_default_visible", visible)
    }

    /// 失败自动重试次数（缺省 3，clamp 0..=5；0 表示关闭自动重试）。
    pub fn get_auto_retry_max(&self) -> Result<u32, String> {
        let n = self
            .get_raw("auto_retry_max")?
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(3);
        Ok(n.min(5))
    }

    pub fn set_auto_retry_max(&self, n: u32) -> Result<(), String> {
        let n = n.min(5);
        self.set_raw("auto_retry_max", &n.to_string())
    }

    /// 单次任务最大模型迭代次数（缺省 24，clamp 1..=100）。
    pub fn get_max_iterations(&self) -> Result<u32, String> {
        let n = self
            .get_raw("max_iterations")?
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_ITERATIONS);
        Ok(n.clamp(MAX_ITERATIONS_MIN, MAX_ITERATIONS_MAX))
    }

    pub fn set_max_iterations(&self, n: u32) -> Result<(), String> {
        let n = n.clamp(MAX_ITERATIONS_MIN, MAX_ITERATIONS_MAX);
        self.set_raw("max_iterations", &n.to_string())
    }

    /// 单工具执行超时全局默认（秒，缺省 30，clamp 1..=1800）。
    /// 工具级 `Tool::timeout_secs()` 覆盖优先于此值。
    pub fn get_tool_timeout_secs(&self) -> Result<u64, String> {
        let n = self
            .get_raw("tool_timeout_secs")?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_SECS);
        Ok(n.clamp(TOOL_TIMEOUT_MIN, TOOL_TIMEOUT_MAX))
    }

    pub fn set_tool_timeout_secs(&self, n: u64) -> Result<(), String> {
        let n = n.clamp(TOOL_TIMEOUT_MIN, TOOL_TIMEOUT_MAX);
        self.set_raw("tool_timeout_secs", &n.to_string())
    }

    /// 工具并行执行上限（连续 concurrency_safe 段一次最多并发数，缺省 8，clamp 1..=32）。
    /// 1 = 退化为串行。
    pub fn get_tool_parallelism(&self) -> Result<u64, String> {
        let n = self
            .get_raw("tool_parallelism")?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_PARALLELISM);
        Ok(n.clamp(TOOL_PARALLELISM_MIN, TOOL_PARALLELISM_MAX))
    }

    pub fn set_tool_parallelism(&self, n: u64) -> Result<(), String> {
        let n = n.clamp(TOOL_PARALLELISM_MIN, TOOL_PARALLELISM_MAX);
        self.set_raw("tool_parallelism", &n.to_string())
    }

    /// 读辅助模型 id（用于标题/建议生成）。None = 跟随会话模型。
    pub fn get_aux_model_id(&self) -> Result<Option<String>, String> {
        Ok(self
            .get_raw("aux_model_id")?
            .filter(|s| !s.trim().is_empty()))
    }

    /// 写辅助模型 id；None/空 表示清除（回退会话模型）。
    pub fn set_aux_model_id(&self, model_id: Option<&str>) -> Result<(), String> {
        match model_id.filter(|s| !s.trim().is_empty()) {
            Some(id) => self.set_raw("aux_model_id", id),
            None => self.delete_raw("aux_model_id"),
        }
    }

    /// 知识库向量检索全局开关（默认关）。
    pub fn get_knowledge_vector_enabled(&self) -> Result<bool, String> {
        self.get_bool("knowledge_vector_enabled", false)
    }

    pub fn set_knowledge_vector_enabled(&self, enabled: bool) -> Result<(), String> {
        self.set_bool("knowledge_vector_enabled", enabled)
    }

    /// 知识库向量检索所用 embedding 模型 id（空 = 未选）。
    pub fn get_knowledge_embedding_model(&self) -> Result<String, String> {
        self.get_str("knowledge_embedding_model")
    }

    pub fn set_knowledge_embedding_model(&self, model_id: &str) -> Result<(), String> {
        self.set_str("knowledge_embedding_model", model_id)
    }

    /// 子代理执行方式：parallel（同轮并行启动）| serial（同父会话按创建顺序逐个启动）。
    /// 缺省 parallel，保持既有行为。
    pub fn get_subagent_execution_mode(&self) -> Result<String, String> {
        match self.get_raw("subagent_execution_mode")?.as_deref() {
            Some(SUBAGENT_EXECUTION_MODE_SERIAL) => Ok(SUBAGENT_EXECUTION_MODE_SERIAL.to_string()),
            Some(SUBAGENT_EXECUTION_MODE_PARALLEL) | None => {
                Ok(SUBAGENT_EXECUTION_MODE_PARALLEL.to_string())
            }
            Some(_) => Ok(SUBAGENT_EXECUTION_MODE_PARALLEL.to_string()),
        }
    }

    pub fn set_subagent_execution_mode(&self, mode: &str) -> Result<(), String> {
        match mode {
            SUBAGENT_EXECUTION_MODE_PARALLEL | SUBAGENT_EXECUTION_MODE_SERIAL => {
                self.set_raw("subagent_execution_mode", mode)
            }
            _ => Err("subagent_execution_mode 只能是 parallel 或 serial".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> AppSettingsStore {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-settings-test-{nanos}.sqlite3"));
        let db = std::sync::Arc::new(crate::storage::AppDatabase::open(path).unwrap());
        AppSettingsStore::open(db).unwrap()
    }

    #[test]
    fn model_call_log_toggle_default_off() {
        let s = temp_store();
        assert!(!s.get_model_call_log_enabled().unwrap());
        s.set_model_call_log_enabled(true).unwrap();
        assert!(s.get_model_call_log_enabled().unwrap());
    }

    #[test]
    fn auto_retry_max_defaults_3_and_clamps() {
        let s = temp_store();
        assert_eq!(s.get_auto_retry_max().unwrap(), 3);
        s.set_auto_retry_max(0).unwrap();
        assert_eq!(s.get_auto_retry_max().unwrap(), 0);
        s.set_auto_retry_max(9).unwrap();
        assert_eq!(s.get_auto_retry_max().unwrap(), 5);
    }

    #[test]
    fn max_iterations_defaults_24_and_clamps() {
        let s = temp_store();
        assert_eq!(s.get_max_iterations().unwrap(), 24);
        s.set_max_iterations(0).unwrap();
        assert_eq!(s.get_max_iterations().unwrap(), 1);
        s.set_max_iterations(48).unwrap();
        assert_eq!(s.get_max_iterations().unwrap(), 48);
        s.set_max_iterations(999).unwrap();
        assert_eq!(s.get_max_iterations().unwrap(), 100);
    }

    #[test]
    fn auto_compact_flag_defaults_on_then_round_trips() {
        let s = temp_store();
        assert!(s.get_auto_compact_enabled().unwrap(), "缺省应为开");
        s.set_auto_compact_enabled(false).unwrap();
        assert!(!s.get_auto_compact_enabled().unwrap());
        s.set_auto_compact_enabled(true).unwrap();
        assert!(s.get_auto_compact_enabled().unwrap());
    }

    #[test]
    fn auto_compact_threshold_defaults_90_and_clamps() {
        let s = temp_store();
        assert_eq!(s.get_auto_compact_threshold_pct().unwrap(), 90);
        s.set_auto_compact_threshold_pct(10).unwrap();
        assert_eq!(s.get_auto_compact_threshold_pct().unwrap(), 50);
        s.set_auto_compact_threshold_pct(80).unwrap();
        assert_eq!(s.get_auto_compact_threshold_pct().unwrap(), 80);
        s.set_auto_compact_threshold_pct(100).unwrap();
        assert_eq!(s.get_auto_compact_threshold_pct().unwrap(), 95);
    }

    #[test]
    fn show_completed_process_defaults_on_then_round_trips() {
        let s = temp_store();
        assert!(s.get_show_completed_process().unwrap(), "缺省应为开");
        s.set_show_completed_process(false).unwrap();
        assert!(!s.get_show_completed_process().unwrap());
        s.set_show_completed_process(true).unwrap();
        assert!(s.get_show_completed_process().unwrap());
    }

    #[test]
    fn computer_use_enabled_defaults_off_then_round_trips() {
        let s = temp_store();
        assert!(!s.get_computer_use_enabled().unwrap(), "缺省应为关");
        s.set_computer_use_enabled(true).unwrap();
        assert!(s.get_computer_use_enabled().unwrap());
        s.set_computer_use_enabled(false).unwrap();
        assert!(!s.get_computer_use_enabled().unwrap());
    }

    #[test]
    fn session_task_panel_default_visible_defaults_on_then_round_trips() {
        let s = temp_store();
        assert!(
            s.get_session_task_panel_default_visible().unwrap(),
            "缺省应为开，保持现有 SessionPage 行为"
        );
        s.set_session_task_panel_default_visible(false).unwrap();
        assert!(!s.get_session_task_panel_default_visible().unwrap());
        s.set_session_task_panel_default_visible(true).unwrap();
        assert!(s.get_session_task_panel_default_visible().unwrap());
    }

    #[test]
    fn global_permission_mode_defaults_manual_then_persists() {
        let s = temp_store();
        assert_eq!(s.get_global_permission_mode().unwrap(), "manual");
        s.set_global_permission_mode("auto").unwrap();
        assert_eq!(s.get_global_permission_mode().unwrap(), "auto");
        s.set_global_permission_mode("full").unwrap();
        assert_eq!(s.get_global_permission_mode().unwrap(), "full");
    }

    #[test]
    fn suggestions_enabled_defaults_on_then_round_trips() {
        let s = temp_store();
        assert!(s.get_suggestions_enabled().unwrap(), "缺省应为开");
        s.set_suggestions_enabled(false).unwrap();
        assert!(!s.get_suggestions_enabled().unwrap());
    }

    #[test]
    fn aux_model_id_round_trip_and_clear() {
        let s = temp_store();
        assert_eq!(s.get_aux_model_id().unwrap(), None);
        s.set_aux_model_id(Some("mdl_aux")).unwrap();
        assert_eq!(s.get_aux_model_id().unwrap(), Some("mdl_aux".to_string()));
        // 空白等同清除。
        s.set_aux_model_id(Some("   ")).unwrap();
        assert_eq!(s.get_aux_model_id().unwrap(), None);
        s.set_aux_model_id(Some("mdl_aux")).unwrap();
        s.set_aux_model_id(None).unwrap();
        assert_eq!(s.get_aux_model_id().unwrap(), None);
    }

    #[test]
    fn browser_idle_close_min_defaults_10_then_round_trips() {
        let s = temp_store();
        assert_eq!(s.get_browser_idle_close_min().unwrap(), 10, "缺省应为 10 分钟");
        s.set_browser_idle_close_min(0).unwrap();
        assert_eq!(s.get_browser_idle_close_min().unwrap(), 0, "0 = 不自动关");
        s.set_browser_idle_close_min(30).unwrap();
        assert_eq!(s.get_browser_idle_close_min().unwrap(), 30);
    }

    #[test]
    fn subagent_execution_mode_defaults_parallel_and_rejects_invalid_values() {
        let s = temp_store();
        assert_eq!(s.get_subagent_execution_mode().unwrap(), "parallel");
        s.set_subagent_execution_mode("serial").unwrap();
        assert_eq!(s.get_subagent_execution_mode().unwrap(), "serial");
        s.set_subagent_execution_mode("parallel").unwrap();
        assert_eq!(s.get_subagent_execution_mode().unwrap(), "parallel");
        assert!(s.set_subagent_execution_mode("batched").is_err());
    }

    #[test]
    fn tool_timeout_secs_default_and_clamp() {
        let s = temp_store();
        assert_eq!(s.get_tool_timeout_secs().unwrap(), DEFAULT_TOOL_TIMEOUT_SECS);
        s.set_tool_timeout_secs(5).unwrap();
        assert_eq!(s.get_tool_timeout_secs().unwrap(), 5);
        s.set_tool_timeout_secs(0).unwrap(); // 低于 min → clamp 到 TOOL_TIMEOUT_MIN
        assert_eq!(s.get_tool_timeout_secs().unwrap(), TOOL_TIMEOUT_MIN);
        s.set_tool_timeout_secs(99999).unwrap(); // 高于 max → clamp 到 TOOL_TIMEOUT_MAX
        assert_eq!(s.get_tool_timeout_secs().unwrap(), TOOL_TIMEOUT_MAX);
    }

    #[test]
    fn tool_parallelism_default_and_clamp() {
        let s = temp_store();
        assert_eq!(s.get_tool_parallelism().unwrap(), DEFAULT_TOOL_PARALLELISM);
        s.set_tool_parallelism(4).unwrap();
        assert_eq!(s.get_tool_parallelism().unwrap(), 4);
        s.set_tool_parallelism(0).unwrap(); // < min → clamp 到 1
        assert_eq!(s.get_tool_parallelism().unwrap(), TOOL_PARALLELISM_MIN);
        s.set_tool_parallelism(9999).unwrap(); // > max → clamp 到 32
        assert_eq!(s.get_tool_parallelism().unwrap(), TOOL_PARALLELISM_MAX);
    }
}
