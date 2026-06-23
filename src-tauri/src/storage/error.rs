//! SQLite 基础设施错误类型。

use std::fmt;

/// Storage 模块对外暴露的稳定错误。
#[derive(Debug)]
pub enum StorageError {
    /// SQLite 原始错误，进入 UI 前必须由 command 层转换成用户可读文案。
    Sqlite(rusqlite::Error),
    /// 事务闭包主动返回的失败。
    TransactionFailed(String),
    /// migration runner 无法完成基础 schema 初始化。
    MigrationFailed(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Sqlite(err) => write!(f, "sqlite error: {err}"),
            StorageError::TransactionFailed(message) => write!(f, "transaction failed: {message}"),
            StorageError::MigrationFailed(message) => write!(f, "migration failed: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        StorageError::Sqlite(value)
    }
}
