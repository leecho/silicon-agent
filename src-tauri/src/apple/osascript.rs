//! osascript（AppleScript）子进程执行：备忘录无公开框架，只能走自动化（AppleEvents）。
//! 脚本经 stdin 传入，超时可终止；识别 TCC 未授权错误（-1743）→ PermissionDenied。
//! 结构改造自 `tools/command_tool.rs::run_command`。

use super::AppleError;

/// 运行一段 AppleScript，返回 stdout（已 trim）。失败映射为 `AppleError`。
pub fn run_osascript(script: &str, timeout_ms: u64) -> Result<String, AppleError> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new("osascript")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppleError::Backend(format!("启动 osascript 失败：{e}")))?;

    // 写入脚本到 stdin 后关闭，触发执行。
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .map_err(|e| AppleError::Backend(format!("写入脚本失败：{e}")))?;
        // drop(stdin) 关闭管道（离开作用域）。
    }

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(h) = stdout.as_mut() {
            let _ = h.read_to_string(&mut buf);
        }
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(h) = stderr.as_mut() {
            let _ = h.read_to_string(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_handle.join().unwrap_or_default();
                let err = err_handle.join().unwrap_or_default();
                if status.success() {
                    return Ok(out.trim_end().to_string());
                }
                return Err(classify_error(&err));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(AppleError::Backend("osascript 执行超时".into()));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(AppleError::Backend(format!("等待 osascript 失败：{e}"))),
        }
    }
}

/// 把 osascript stderr 归类为 AppleError。TCC 未授权错误码为 -1743。
fn classify_error(stderr: &str) -> AppleError {
    let s = stderr.trim();
    if s.contains("-1743") || s.contains("Not authorized") || s.contains("not allowed") {
        AppleError::PermissionDenied
    } else if s.is_empty() {
        AppleError::Backend("osascript 未返回错误信息".into())
    } else {
        AppleError::Backend(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_unauthorized() {
        assert_eq!(
            classify_error("execution error: Not authorized to send Apple events to Notes. (-1743)"),
            AppleError::PermissionDenied
        );
    }

    #[test]
    fn classify_other_backend_error() {
        match classify_error("execution error: 出错了 (-2700)") {
            AppleError::Backend(m) => assert!(m.contains("-2700")),
            other => panic!("应归为 Backend，实际：{other:?}"),
        }
    }

    /// 真实跑一段无副作用脚本，验证子进程链路通（不碰备忘录、不触发 TCC）。
    #[test]
    fn runs_trivial_script() {
        let out = run_osascript("return \"ok-\" & (1 + 1)", 5000).expect("应成功");
        assert_eq!(out, "ok-2");
    }
}
