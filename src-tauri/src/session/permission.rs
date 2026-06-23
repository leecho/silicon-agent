use crate::app_settings::AppSettingsStore;
use crate::session::SessionStore;
use crate::tools::RiskLevel;

/// 权限强度模式。与工作模式（normal/plan）正交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// 有副作用工具首次确认，授权后会话内放行。
    Manual,
    /// 低风险写工具自动放行，高风险（命令执行）仍需确认。
    Auto,
    /// 所有工具放行，不确认。
    Full,
}

impl PermissionMode {
    /// 解析字符串，未知/脏数据回退 `Manual`。
    pub fn parse(s: &str) -> PermissionMode {
        match s {
            "auto" => PermissionMode::Auto,
            "full" => PermissionMode::Full,
            _ => PermissionMode::Manual,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::Auto => "auto",
            PermissionMode::Full => "full",
        }
    }
}

/// 解析会话生效权限模式：会话级覆盖优先，否则全局默认，再否则 "manual"。
///
/// 这是一处跨表/跨模块的组合：会话级覆盖存在 `sessions`（owner = [`SessionStore`]），
/// 全局默认存在 `app_settings`（owner = [`AppSettingsStore`]）。组合归在 permission
/// 模块，避免任一 store 反向依赖另一方的语义。`app_settings` 缺省（如测试未注入）时按
/// "manual" 兜底。
pub fn resolve_effective_mode(
    session: &SessionStore,
    app_settings: Option<&AppSettingsStore>,
    session_id: &str,
) -> Result<String, String> {
    if let Some(mode) = session.get_session_permission_mode(session_id)? {
        return Ok(mode);
    }
    match app_settings {
        Some(s) => s.get_global_permission_mode(),
        None => Ok("manual".to_string()),
    }
}

/// 给定工具风险、生效模式、是否已授权，判定本次调用是否需要弹卡确认。
pub fn needs_confirmation(risk: RiskLevel, mode: PermissionMode, granted: bool) -> bool {
    match mode {
        PermissionMode::Full => false,
        PermissionMode::Auto => risk == RiskLevel::High && !granted,
        PermissionMode::Manual => risk != RiskLevel::Safe && !granted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_falls_back_to_manual() {
        assert_eq!(PermissionMode::parse("auto"), PermissionMode::Auto);
        assert_eq!(PermissionMode::parse("full"), PermissionMode::Full);
        assert_eq!(PermissionMode::parse("manual"), PermissionMode::Manual);
        assert_eq!(PermissionMode::parse("garbage"), PermissionMode::Manual);
        assert_eq!(PermissionMode::parse(""), PermissionMode::Manual);
    }

    #[test]
    fn manual_confirms_low_and_high_until_granted() {
        let m = PermissionMode::Manual;
        assert!(needs_confirmation(RiskLevel::Low, m, false));
        assert!(needs_confirmation(RiskLevel::High, m, false));
        assert!(!needs_confirmation(RiskLevel::Safe, m, false));
        assert!(!needs_confirmation(RiskLevel::Low, m, true));
    }

    #[test]
    fn auto_runs_low_confirms_high() {
        let a = PermissionMode::Auto;
        assert!(!needs_confirmation(RiskLevel::Low, a, false));
        assert!(needs_confirmation(RiskLevel::High, a, false));
        assert!(!needs_confirmation(RiskLevel::High, a, true));
        assert!(!needs_confirmation(RiskLevel::Safe, a, false));
    }

    #[test]
    fn full_confirms_nothing() {
        let f = PermissionMode::Full;
        assert!(!needs_confirmation(RiskLevel::Low, f, false));
        assert!(!needs_confirmation(RiskLevel::High, f, false));
        assert!(!needs_confirmation(RiskLevel::Safe, f, false));
    }

    fn temp_stores() -> (SessionStore, AppSettingsStore) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sw-perm-test-{nanos}.sqlite3"));
        let db = std::sync::Arc::new(crate::storage::AppDatabase::open(path).unwrap());
        (
            SessionStore::open(db.clone()).unwrap(),
            AppSettingsStore::open(db).unwrap(),
        )
    }

    #[test]
    fn effective_mode_session_overrides_global() {
        let (session, app_settings) = temp_stores();
        session
            .create_session("session-em-1", "t", "1", false)
            .unwrap();
        // 默认：会话无覆盖 + 全局缺省 → manual
        assert_eq!(
            resolve_effective_mode(&session, Some(&app_settings), "session-em-1").unwrap(),
            "manual"
        );
        // 全局设 auto，会话仍无覆盖 → auto
        app_settings.set_global_permission_mode("auto").unwrap();
        assert_eq!(
            resolve_effective_mode(&session, Some(&app_settings), "session-em-1").unwrap(),
            "auto"
        );
        // 会话覆盖 full → full（优先于全局）
        session
            .set_session_permission_mode("session-em-1", Some("full"), "2")
            .unwrap();
        assert_eq!(
            resolve_effective_mode(&session, Some(&app_settings), "session-em-1").unwrap(),
            "full"
        );
    }

    #[test]
    fn effective_mode_without_app_settings_falls_back_to_manual() {
        let (session, _app_settings) = temp_stores();
        session
            .create_session("session-em-2", "t", "1", false)
            .unwrap();
        // 未注入 app_settings 且会话无覆盖 → manual 兜底
        assert_eq!(
            resolve_effective_mode(&session, None, "session-em-2").unwrap(),
            "manual"
        );
        // 会话覆盖仍生效（不依赖全局）
        session
            .set_session_permission_mode("session-em-2", Some("auto"), "2")
            .unwrap();
        assert_eq!(
            resolve_effective_mode(&session, None, "session-em-2").unwrap(),
            "auto"
        );
    }
}
