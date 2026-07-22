use std::path::PathBuf;

use crate::tools::sandbox::resolve_in_workspace;
use crate::tools::{RiskLevel, Tool};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MIN_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 120_000;

/// 命令执行结果(原始 outcome)。
enum CommandOutcome {
    Exited {
        code: i32,
        stdout: String,
        stderr: String,
    },
    TimedOut,
    /// run 级取消标记置位 → 已 kill 子进程并中止等待。
    Cancelled,
}

/// 非 shell 结构化执行 + 超时可终止。stdout/stderr 用读取线程避免管道满死锁;
/// 超时 kill 子进程。改造自 super-worker tools/executor.rs::run_command。
fn run_command(
    program: &str,
    args: &[String],
    cwd: &str,
    timeout_ms: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<CommandOutcome, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("启动命令失败 '{program}': {err}"))?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(handle) = stdout.as_mut() {
            let _ = handle.read_to_string(&mut buf);
        }
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(handle) = stderr.as_mut() {
            let _ = handle.read_to_string(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_handle.join().unwrap_or_default();
                let err = err_handle.join().unwrap_or_default();
                return Ok(CommandOutcome::Exited {
                    code: status.code().unwrap_or(-1),
                    stdout: out,
                    stderr: err,
                });
            }
            Ok(None) => {
                // run 级停止：立即 kill 子进程，不留后台僵尸（L2 立即停止）。
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Ok(CommandOutcome::Cancelled);
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Ok(CommandOutcome::TimedOut);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(format!("等待命令失败: {err}")),
        }
    }
}

/// 输出按字符预算截断。改造自 super-worker tools/executor.rs::truncate_output。
fn truncate_output(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(max_chars).collect();
    format!("{truncated}…(已截断)")
}

pub struct CommandExecute {
    pub workspace: PathBuf,
}

impl Tool for CommandExecute {
    fn name(&self) -> &str {
        "run_command"
    }

    fn label(&self) -> &str {
        "执行命令"
    }

    fn description(&self) -> &str {
        "在工作区内执行结构化命令(非 shell):program + args。有超时与输出截断。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "program": {"type": "string", "description": "可执行程序"},
                "args": {"type": "array", "items": {"type": "string"}, "description": "参数列表"},
                "cwd": {"type": "string", "description": "工作目录(工作区内,缺省为工作区根)"},
                "timeout_ms": {"type": "integer", "description": "超时毫秒(1000..120000,缺省 30000)"}
            },
            "required": ["program"]
        })
    }

    fn concurrency_safe(&self) -> bool {
        false
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    fn execute(&self, args: &serde_json::Value) -> Result<String, String> {
        // 无取消标记路径：永不取消，行为等同改造前。
        self.execute_cancellable(args, &std::sync::atomic::AtomicBool::new(false))
    }

    fn execute_cancellable(
        &self,
        args: &serde_json::Value,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<String, String> {
        let program = args
            .get("program")
            .and_then(|v| v.as_str())
            .ok_or("缺少 program")?;
        let cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let cwd = match args.get("cwd").and_then(|v| v.as_str()) {
            Some(p) => resolve_in_workspace(&self.workspace, p)?,
            None => self.workspace.clone(),
        };
        let cwd_str = cwd.to_string_lossy().to_string();

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);

        match run_command(program, &cmd_args, &cwd_str, timeout_ms, cancel)? {
            CommandOutcome::TimedOut => Err(format!("命令超时(>{timeout_ms}ms)")),
            CommandOutcome::Cancelled => Err("命令执行已被停止".into()),
            CommandOutcome::Exited {
                code,
                stdout,
                stderr,
            } => {
                let mut result = format!("退出码: {code}");
                let out = truncate_output(&stdout, 3000);
                if !out.is_empty() {
                    result.push_str(&format!("\nstdout:\n{out}"));
                }
                let err = truncate_output(&stderr, 1000);
                if !err.is_empty() {
                    result.push_str(&format!("\nstderr:\n{err}"));
                }
                Ok(result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    /// 取消已置位时，长命令必须被立即 kill 并快速返回（不等满 timeout_ms）。
    #[test]
    fn execute_cancellable_kills_running_command_promptly() {
        let tool = CommandExecute {
            workspace: std::env::temp_dir(),
        };
        let args = serde_json::json!({
            "program": "sleep",
            "args": ["30"],
            "timeout_ms": 120_000,
        });
        let cancel = AtomicBool::new(true); // 预置取消
        let start = Instant::now();
        let result = tool.execute_cancellable(&args, &cancel);
        assert!(
            start.elapsed().as_secs() < 2,
            "取消已置位应 ~20ms 内杀子进程，实际耗时 {:?}",
            start.elapsed()
        );
        assert_eq!(result, Err("命令执行已被停止".into()));
    }
}
