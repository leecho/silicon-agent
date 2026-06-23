//! 运行时编排子系统：run 生命周期（提交/续跑/取消/各类 decision）+ 子代理编排
//! （派发/排队/collect/retry/回填/重启恢复）。从 `AppState` 抽出，拥有 run 运行时状态。

pub mod coordinator;
pub mod reconcile;
pub mod watchdog;

pub use coordinator::RunCoordinator;
