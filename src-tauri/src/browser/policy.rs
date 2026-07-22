//! 浏览器动作安全门控：据结构化信号（动作类型 + 目标是否提交控件）判定。
//! 纯函数，便于单测；**不靠按钮文字猜**（守 AGENTS.md「不要用脆弱关键词规则」红线）。

/// 门控决策。P1 用 Allow/Confirm；Deny 为 P2 预留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Confirm,
    Deny,
}

/// 据动作与「目标是否提交控件」判定。
/// 高风险 = 点击解析为提交控件（触发表单提交，可能不可逆：下单/提交/删除）。
pub fn evaluate(action: &str, target_submits: bool) -> Decision {
    match action {
        "click" | "double_click" if target_submits => Decision::Confirm,
        _ => Decision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_submit_control_needs_confirm() {
        assert_eq!(evaluate("click", true), Decision::Confirm);
    }

    #[test]
    fn click_non_submit_is_allowed() {
        assert_eq!(evaluate("click", false), Decision::Allow);
    }

    #[test]
    fn read_and_fill_actions_always_allowed() {
        for a in ["navigate", "observe", "fill", "select", "scroll", "extract", "wait", "back"] {
            assert_eq!(evaluate(a, true), Decision::Allow, "{a} 不应被 submits 影响");
        }
    }
}
