use silicon_agent::commands::format_ask_answers;
use silicon_agent::session::AskQuestion;

fn q(header: &str, question: &str, multi: bool, options: &[&str]) -> AskQuestion {
    AskQuestion {
        header: header.into(),
        question: question.into(),
        multi_select: multi,
        options: options.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn formats_single_multi_and_skipped() {
    let questions = vec![
        q(
            "角色定位",
            "你扮演哪个角色?",
            false,
            &["IT项目经理", "产品经理"],
        ),
        q("重点功能", "关注哪些?", true, &["风险管理", "进度跟踪"]),
        q("", "补充说明?", false, &[]),
    ];
    let answers = vec![
        vec!["IT项目经理".to_string()],
        vec!["风险管理".to_string(), "进度跟踪".to_string()],
        vec![],
    ];
    let text = format_ask_answers(&questions, &answers);
    assert_eq!(
        text,
        "用户已回答：\n1. 角色定位：IT项目经理\n2. 重点功能：风险管理、进度跟踪\n3. 补充说明?：（未回答）"
    );
}

#[test]
fn answers_shorter_than_questions_treated_as_skipped() {
    let questions = vec![q("A", "q1", false, &["x"]), q("B", "q2", false, &["y"])];
    let answers = vec![vec!["x".to_string()]];
    let text = format_ask_answers(&questions, &answers);
    assert_eq!(text, "用户已回答：\n1. A：x\n2. B：（未回答）");
}
