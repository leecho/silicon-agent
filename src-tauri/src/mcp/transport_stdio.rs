//! stdio 传输：子进程 + 行分隔 JSON-RPC。
//! 读取走专用线程 + mpsc，使 request 能带超时等待。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::mcp::transport::McpTransport;

/// stderr 留存上限（字节）：超出后丢弃旧内容，仅保尾部。
const STDERR_CAP: usize = 8 * 1024;

pub struct StdioTransport {
    child: Child,
    stdin: std::process::ChildStdin,
    rx: Receiver<serde_json::Value>,
    /// 子进程 stderr 留存：server 退出/报错时（如缺 API key）把真因带进错误，不再只报「超时」。
    stderr_buf: Arc<Mutex<String>>,
}

impl StdioTransport {
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::BTreeMap<String, String>,
        cwd: Option<&str>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = cwd.filter(|d| !d.trim().is_empty()) {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn().map_err(|e| {
            format!("启动 MCP server 失败（{command}）：{e}。请确认本机已安装对应运行时（如 Node.js/Python）且命令在 PATH 中")
        })?;
        let stdin = child.stdin.take().ok_or("无法获取子进程 stdin")?;
        let stdout = child.stdout.take().ok_or("无法获取子进程 stdout")?;
        let stderr = child.stderr.take().ok_or("无法获取子进程 stderr")?;
        // 后台收集 stderr（保尾部 STDERR_CAP 字节），供失败时诊断。
        let stderr_buf = Arc::new(Mutex::new(String::new()));
        {
            let buf = stderr_buf.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let mut g = buf.lock().unwrap_or_else(|e| e.into_inner());
                    g.push_str(&line);
                    g.push('\n');
                    if g.len() > STDERR_CAP {
                        let cut = g.len() - STDERR_CAP;
                        *g = g[cut..].to_string();
                    }
                }
            });
        }
        let (tx, rx): (Sender<serde_json::Value>, Receiver<serde_json::Value>) =
            std::sync::mpsc::channel();
        // channel 无界：依赖每次 request 的超时窗口约束积压；每 server 单连接 + Mutex 串行化调用，洪泛风险有界。
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            stderr_buf,
        })
    }

    /// 当前收集到的子进程 stderr 尾部（trim 后）。供失败诊断。
    fn stderr_tail(&self) -> String {
        self.stderr_buf
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trim()
            .to_string()
    }
}

impl McpTransport for StdioTransport {
    fn request(
        &mut self,
        msg: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Option<serde_json::Value>, String> {
        let line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("写入 MCP server 失败：{e}"))?;
        let Some(want_id) = msg.get("id").cloned() else {
            return Ok(None); // 通知不等响应
        };
        let deadline = Instant::now() + timeout;
        loop {
            let remain = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| "等待 MCP server 响应超时".to_string())?;
            let v = self.rx.recv_timeout(remain).map_err(|_| {
                let tail = self.stderr_tail();
                if tail.is_empty() {
                    "等待 MCP server 响应超时或连接已断开".to_string()
                } else {
                    format!("MCP server 未响应（已退出或出错）。子进程输出：{tail}")
                }
            })?;
            let is_reply = v.get("id") == Some(&want_id)
                && (v.get("result").is_some() || v.get("error").is_some());
            if is_reply {
                return Ok(Some(v));
            }
            // 其余消息（server 通知/请求）一期忽略。
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_missing_command_gives_clear_error() {
        let err =
            StdioTransport::spawn("siliconworker-no-such-cmd", &[], &Default::default(), None)
                .err()
                .unwrap();
        assert!(err.contains("启动 MCP server 失败"));
        assert!(err.contains("运行时"));
    }

    #[cfg(unix)]
    #[test]
    fn stderr_is_surfaced_when_no_response() {
        // 子进程往 stderr 写一行后挂起、不回 stdout：request 超时，错误应带上该 stderr。
        let args = vec!["-c".to_string(), "echo boom-stderr 1>&2; sleep 5".to_string()];
        let mut t = StdioTransport::spawn("sh", &args, &Default::default(), None).unwrap();
        std::thread::sleep(Duration::from_millis(250)); // 等 stderr 收集线程读到
        let err = t
            .request(
                &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
                Duration::from_millis(500),
            )
            .unwrap_err();
        assert!(err.contains("boom-stderr"), "应带上子进程 stderr，实际：{err}");
    }

    #[cfg(unix)]
    #[test]
    fn notification_writes_without_waiting() {
        // cat 原样回显；通知不等响应，所以不会阻塞。
        let mut t = StdioTransport::spawn("cat", &[], &Default::default(), None).unwrap();
        let out = t
            .request(
                &serde_json::json!({"jsonrpc":"2.0","method":"x"}),
                Duration::from_secs(1),
            )
            .unwrap();
        assert!(out.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn request_times_out_when_no_reply() {
        // cat 回显的是「请求」（含 method 无 result），匹配逻辑应忽略它并最终超时。
        let mut t = StdioTransport::spawn("cat", &[], &Default::default(), None).unwrap();
        let err = t
            .request(
                &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"x"}),
                Duration::from_millis(300),
            )
            .unwrap_err();
        assert!(err.contains("超时"));
    }
}
