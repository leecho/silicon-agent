//! cancel 感知的 SSE 逐行读（L1 立即停止）。
//!
//! 现状流式读是 `for line in reader.lines()`——阻塞在 socket 读上，只有下一行到达才回到循环、
//! 才查得到 cancel；模型静默期（首 token 前/长 thinking）可挂到 socket idle 超时（默认 60s）。
//!
//! 本 helper 把 socket 读超时降为**短轮询节拍**，令阻塞读周期性醒来（`WouldBlock`/`TimedOut`）：
//! 醒来即查 cancel，置位则立即返回 `"model stream cancelled"`；否则累加 idle，超真实预算才判超时。
//! 半行跨读超时**不丢**——用持久 `buf` + `read_until`，下一拍继续追加同一行。

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::provider::client::ProviderCallError;

/// cancel 感知地逐行读 SSE。
/// - `on_line`：每读满一行（去掉行尾 CR/LF）回调一次；返回 `false` = on_event 请求中止 → 返回 cancelled。
/// - `idle_budget`：连续无数据的真实上限；超过判传输层 idle 超时（可重试）。
/// - `cancel`：run 级取消标记；idle 醒来时查，置位即立即返回 cancelled。
///
/// 依赖底层 reader 设了**短读超时**（socket idle 触发 `WouldBlock`/`TimedOut`），
/// 否则退化为普通阻塞逐行读（cancel 仍逐行生效，只是静默期不提前醒）。
pub(super) fn read_sse_lines<R: BufRead + ?Sized>(
    reader: &mut R,
    cancel: &AtomicBool,
    idle_budget: Duration,
    mut on_line: impl FnMut(&str) -> Result<bool, ProviderCallError>,
) -> Result<(), ProviderCallError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut idle_start: Option<Instant> = None;
    loop {
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return Ok(()), // EOF
            Ok(_) => {
                idle_start = None;
                if buf.last() == Some(&b'\n') {
                    let line = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    buf.clear();
                    if !on_line(&line)? {
                        return Err(ProviderCallError::new("model stream cancelled"));
                    }
                }
                // 罕见：无换行的 Ok(>0)（连接半关）——下一轮 read_until 返回 Ok(0) 收尾。
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // idle 醒来：优先查 cancel。
                if cancel.load(Ordering::Relaxed) {
                    return Err(ProviderCallError::new("model stream cancelled"));
                }
                let started = *idle_start.get_or_insert_with(Instant::now);
                if started.elapsed() >= idle_budget {
                    return Err(ProviderCallError::transient("provider stream idle timeout"));
                }
                // buf 保留半行，继续下一拍
            }
            Err(e) => {
                return Err(ProviderCallError::transient(format!(
                    "provider stream read failed: {e}"
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::{BufReader, Read};

    /// 脚本化假源：每次 read 弹出一段「数据片段」或一个 io 错误（如 WouldBlock）。
    struct ScriptedRead(VecDeque<std::io::Result<Vec<u8>>>);
    impl Read for ScriptedRead {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            match self.0.pop_front() {
                Some(Ok(data)) => {
                    let n = data.len().min(out.len());
                    out[..n].copy_from_slice(&data[..n]);
                    Ok(n)
                }
                Some(Err(e)) => Err(e),
                None => Ok(0), // EOF
            }
        }
    }
    fn wouldblock() -> std::io::Result<Vec<u8>> {
        Err(std::io::Error::from(std::io::ErrorKind::WouldBlock))
    }

    #[test]
    fn accumulates_line_across_idle_timeout() {
        // "hel" → idle 超时 → "lo\n"：半行跨超时不丢，最终成一行 "hello"。
        let mut reader = BufReader::new(ScriptedRead(VecDeque::from(vec![
            Ok(b"hel".to_vec()),
            wouldblock(),
            Ok(b"lo\n".to_vec()),
        ])));
        let cancel = AtomicBool::new(false);
        let mut lines = Vec::new();
        let res = read_sse_lines(&mut reader, &cancel, Duration::from_secs(60), |line| {
            lines.push(line.to_string());
            Ok(true)
        });
        assert!(res.is_ok(), "res={res:?}");
        assert_eq!(lines, vec!["hello".to_string()]);
    }

    #[test]
    fn returns_cancelled_when_flag_set_during_idle() {
        let mut reader = BufReader::new(ScriptedRead(VecDeque::from(vec![
            wouldblock(),
            wouldblock(),
        ])));
        let cancel = AtomicBool::new(true); // 静默期就已取消
        let res = read_sse_lines(&mut reader, &cancel, Duration::from_secs(60), |_| Ok(true));
        let err = res.expect_err("应因取消返回错误");
        assert!(
            format!("{err:?}").contains("cancel"),
            "err={err:?}"
        );
    }

    #[test]
    fn idle_timeout_when_budget_exceeded() {
        // 只吐 WouldBlock、cancel 不置位、预算为 0 → 立即判 idle 超时。
        let mut reader = BufReader::new(ScriptedRead(VecDeque::from(vec![wouldblock()])));
        let cancel = AtomicBool::new(false);
        let res = read_sse_lines(&mut reader, &cancel, Duration::from_secs(0), |_| Ok(true));
        let err = res.expect_err("应判 idle 超时");
        assert!(format!("{err:?}").contains("idle"), "err={err:?}");
    }

    #[test]
    fn on_line_false_aborts_as_cancelled() {
        let mut reader = BufReader::new(ScriptedRead(VecDeque::from(vec![Ok(b"a\nb\n".to_vec())])));
        let cancel = AtomicBool::new(false);
        let res = read_sse_lines(&mut reader, &cancel, Duration::from_secs(60), |_| Ok(false));
        assert!(res.is_err());
    }

}
