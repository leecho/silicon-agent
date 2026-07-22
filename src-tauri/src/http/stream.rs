use std::io::{self, Read};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::http::HttpError;

/// async 任务经 channel 送回的项：字节块 或 传输错误。
pub(crate) enum Chunk {
    Data(Vec<u8>),
    Err(HttpError),
}

/// 通道供给的可取消读：
/// - `read` 在带超时的 `recv` 上阻塞：超时→`WouldBlock`（供上层 read_sse_lines 查 cancel）；
///   通道断开→EOF(0)；收到 `Chunk::Err`→`io::Error`（错误存 `err` 供上层取回映射）。
/// - `Drop`：abort async 任务 → reqwest 连接 drop → socket 关闭（即时停止的落点）。
pub(crate) struct CancellableReader {
    rx: Receiver<Chunk>,
    pending: Vec<u8>,
    /// `Some(d)`：recv 超时 d 后返回 `WouldBlock`（供 read_sse_lines 查 cancel）；
    /// `None`：阻塞 recv（无 WouldBlock，供 std `reader.lines()` 这类不容忍 WouldBlock 的读者，如 MCP SSE）。
    poll: Option<Duration>,
    handle: Option<JoinHandle<()>>,
    err: Option<HttpError>,
}

impl CancellableReader {
    pub(crate) fn new(
        rx: Receiver<Chunk>,
        poll: Option<Duration>,
        handle: JoinHandle<()>,
    ) -> Self {
        Self {
            rx,
            pending: Vec::new(),
            poll,
            handle: Some(handle),
            err: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(rx: Receiver<Chunk>, poll: Option<Duration>) -> Self {
        Self {
            rx,
            pending: Vec::new(),
            poll,
            handle: None,
            err: None,
        }
    }

    /// 取回读过程中缓存的传输错误（上层 io::Error 后调用，映射到域错误）。
    #[allow(dead_code)]
    pub(crate) fn take_err(&mut self) -> Option<HttpError> {
        self.err.take()
    }
}

impl Read for CancellableReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            let chunk = match self.poll {
                Some(d) => match self.rx.recv_timeout(d) {
                    Ok(c) => c,
                    Err(RecvTimeoutError::Timeout) => {
                        return Err(io::Error::new(io::ErrorKind::WouldBlock, "stream idle"));
                    }
                    Err(RecvTimeoutError::Disconnected) => return Ok(0),
                },
                None => match self.rx.recv() {
                    Ok(c) => c,
                    Err(_) => return Ok(0), // 断开 → EOF
                },
            };
            match chunk {
                Chunk::Data(bytes) => self.pending = bytes,
                Chunk::Err(e) => {
                    self.err = Some(e);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "provider stream error",
                    ));
                }
            }
        }
        let n = out.len().min(self.pending.len());
        out[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Drop for CancellableReader {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort(); // 取消 async 任务 → 连接 drop → 即时停止
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CancellableReader, Chunk};
    use crate::http::HttpError;
    use std::io::Read;
    use std::sync::mpsc::sync_channel;
    use std::time::Duration;

    #[test]
    fn yields_data_then_eof_on_disconnect() {
        let (tx, rx) = sync_channel(4);
        tx.send(Chunk::Data(b"hello\n".to_vec())).unwrap();
        drop(tx); // 断开 → EOF
        let mut r = CancellableReader::for_test(rx, Some(Duration::from_millis(50)));
        let mut buf = [0u8; 8];
        let n = r.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello\n");
        assert_eq!(r.read(&mut buf).unwrap(), 0); // EOF
    }

    #[test]
    fn timeout_maps_to_wouldblock() {
        let (_tx, rx) = sync_channel::<Chunk>(1);
        let mut r = CancellableReader::for_test(rx, Some(Duration::from_millis(20)));
        let mut buf = [0u8; 8];
        let err = r.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
    }

    #[test]
    fn blocking_mode_no_wouldblock_eof_on_disconnect() {
        let (tx, rx) = sync_channel(1);
        tx.send(Chunk::Data(b"x".to_vec())).unwrap();
        drop(tx);
        let mut r = CancellableReader::for_test(rx, None); // 阻塞模式
        let mut buf = [0u8; 8];
        assert_eq!(r.read(&mut buf).unwrap(), 1);
        assert_eq!(r.read(&mut buf).unwrap(), 0); // EOF，无 WouldBlock
    }

    #[test]
    fn err_chunk_surfaces_io_error() {
        let (tx, rx) = sync_channel(1);
        tx.send(Chunk::Err(HttpError::Transport("reset".into()))).unwrap();
        let mut r = CancellableReader::for_test(rx, Some(Duration::from_millis(50)));
        let mut buf = [0u8; 8];
        assert!(r.read(&mut buf).is_err());
        assert!(r.take_err().is_some());
    }
}
