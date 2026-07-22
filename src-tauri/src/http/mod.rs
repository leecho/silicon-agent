//! 统一 HTTP 请求框架（reqwest + tokio 同步门面）。async 封装在本模块内部，
//! 对外全同步；引擎/调用方零 async 侵入。取消靠 abort async 任务（连接 drop）实现，
//! 与读超时解耦。T105 P1–P5 已把 provider/MCP/tools/remote/market 全部收敛到此，ureq 已删。

mod client;
mod error;
mod stream;

pub(crate) use client::{HttpClient, HttpRequest};
pub(crate) use error::HttpError;

use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// 专用 tokio runtime（多线程、1 worker）：隔离 HTTP I/O、不占用 Tauri 主 runtime。
/// **必须多线程**：`stream_body` 用 `block_on` 读响应头后 `spawn` 正文流任务，该任务需在
/// block_on 返回后仍被独立 worker 线程驱动；`new_current_thread` 只在 block_on 活跃时推进
/// 任务，spawn 出的正文任务将永不运行（channel 无数据、reader 一直 WouldBlock）。
pub(crate) fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build http tokio runtime")
    })
}
