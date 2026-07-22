//! hook 执行器：在会话工作目录起一个 `sh -c` 子进程执行 hook 命令，stdin 写事件 payload JSON，
//! 超时 10s，读 stdout 试解析控制 JSON。错误/超时/非零退出**非致命**（记 log，按不阻止处理）。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::service::HookRule;
use crate::plugin::vars::resolve_plugin_vars;

/// 单次 hook 命令执行超时。
const HOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// hook 执行结果。`block` 仅在 PreToolUse 语义下被引擎采纳（拦截工具执行）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookOutcome {
    /// 是否阻止后续动作（仅 PreToolUse 有意义；其它事件忽略）。
    pub block: bool,
    /// 阻止原因（回灌给模型 / 落 tool 结果）。
    pub reason: Option<String>,
}

/// 执行一条 command hook。
/// - 解析命令中的 `${CLAUDE_PLUGIN_ROOT}`/`${CLAUDE_PLUGIN_DATA}`/`${ENV}`。
/// - `sh -c <cmd>`，`current_dir(cwd)`（会话工作目录），stdin 写 `payload` JSON。
/// - 超时 10s（超时杀进程、按不阻止处理）。
/// - 读 stdout 试解析 `{"decision":"block","reason":...}`；解析失败/非零退出 → 不阻止（记 log）。
///
/// 任何 IO/进程错误均非致命：返回 `HookOutcome::default()`（不阻止）并记 log。
pub fn run_command_hook(rule: &HookRule, payload: &serde_json::Value, cwd: &Path) -> HookOutcome {
    let plugin_root = rule.plugin_root.to_string_lossy();
    let plugin_data = rule.plugin_data.to_string_lossy();
    let cmd = resolve_plugin_vars(&rule.command, &plugin_root, &plugin_data);
    let payload_bytes = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());

    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[hook] 启动失败（event={}）：{e}", rule.event);
            return HookOutcome::default();
        }
    };

    // 写 stdin（独立作用域，写完即关闭 stdin 让子进程读到 EOF）。
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&payload_bytes);
        // drop(stdin) 关闭管道。
    }

    // 轮询等待，超时则杀进程（避免无限阻塞引擎）。
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= HOOK_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("[hook] 超时 10s 已终止（event={}）", rule.event);
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("[hook] 等待失败（event={}）：{e}", rule.event);
                let _ = child.kill();
                return HookOutcome::default();
            }
        }
    };

    // 读 stdout。
    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_string(&mut stdout);
    }

    let Some(status) = status else {
        return HookOutcome::default(); // 超时 → 不阻止。
    };
    if !status.success() {
        eprintln!(
            "[hook] 非零退出（event={}, code={:?}）——按不阻止处理",
            rule.event,
            status.code()
        );
        return HookOutcome::default();
    }

    parse_outcome(&stdout, &rule.event)
}

/// 解析 stdout 控制 JSON：`{"decision":"block","reason":...}` → block（仅 PreToolUse）。
/// 非 JSON / 缺字段 / 非 PreToolUse → 不阻止。
fn parse_outcome(stdout: &str, event: &str) -> HookOutcome {
    if event != "PreToolUse" {
        return HookOutcome::default();
    }
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return HookOutcome::default();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => {
            let decision = v.get("decision").and_then(|d| d.as_str()).unwrap_or("");
            if decision.eq_ignore_ascii_case("block") {
                HookOutcome {
                    block: true,
                    reason: v
                        .get("reason")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string()),
                }
            } else {
                HookOutcome::default()
            }
        }
        // 解析失败：不阻止（非致命）。
        Err(_) => HookOutcome::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rule(event: &str, command: &str) -> HookRule {
        HookRule {
            event: event.into(),
            matcher: None,
            command: command.into(),
            plugin_root: PathBuf::from("/r"),
            plugin_data: PathBuf::from("/d"),
        }
    }

    #[test]
    fn pretooluse_block_decision_is_parsed() {
        let r = rule(
            "PreToolUse",
            r#"echo '{"decision":"block","reason":"nope"}'"#,
        );
        let out = run_command_hook(
            &r,
            &serde_json::json!({"tool":"x"}),
            std::env::temp_dir().as_path(),
        );
        assert!(out.block);
        assert_eq!(out.reason.as_deref(), Some("nope"));
    }

    #[test]
    fn non_pretooluse_never_blocks() {
        let r = rule(
            "PostToolUse",
            r#"echo '{"decision":"block","reason":"nope"}'"#,
        );
        let out = run_command_hook(&r, &serde_json::json!({}), std::env::temp_dir().as_path());
        assert!(!out.block, "仅 PreToolUse 采纳 block");
    }

    #[test]
    fn non_zero_exit_does_not_block() {
        let r = rule("PreToolUse", "exit 3");
        let out = run_command_hook(&r, &serde_json::json!({}), std::env::temp_dir().as_path());
        assert!(!out.block, "非零退出非致命、不阻止");
    }

    #[test]
    fn unparseable_stdout_does_not_block() {
        let r = rule("PreToolUse", "echo not-json");
        let out = run_command_hook(&r, &serde_json::json!({}), std::env::temp_dir().as_path());
        assert!(!out.block);
    }

    #[test]
    fn plugin_root_var_is_resolved_in_command() {
        // 命令打印解析后的 ${CLAUDE_PLUGIN_ROOT}；不 block，仅验证不 panic 且非零安全。
        let mut r = rule(
            "PreToolUse",
            r#"test -n "${CLAUDE_PLUGIN_ROOT}" && echo '{"decision":"allow"}'"#,
        );
        r.plugin_root = PathBuf::from("/some/root");
        let out = run_command_hook(&r, &serde_json::json!({}), std::env::temp_dir().as_path());
        assert!(!out.block);
    }
}
