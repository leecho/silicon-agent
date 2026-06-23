//! SQLite 基础设施。
//!
//! 本模块只负责连接、事务 helper 和 migration ledger，不拥有业务表、状态机或 projection。

mod error;
mod migration;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use error::StorageError;
pub use migration::{MigrationRecord, STORAGE_MIGRATION_CHECKSUM, STORAGE_MIGRATION_VERSION};
use rusqlite::{params, Connection};

/// SQLite application database entry point.
pub struct AppDatabase {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl AppDatabase {
    /// Open the SQLite file and ensure the storage migration ledger exists.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                StorageError::MigrationFailed(format!("create db directory: {err}"))
            })?;
        }

        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        initialize_storage_schema(&connection)?;

        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    /// Execute a short SQLite operation while holding the application database lock.
    ///
    /// Callers must not perform model calls, network requests, file IO, permission
    /// waits or long-running runtime loops inside this closure.
    pub fn with_connection<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = self.connection.lock().map_err(|_| {
            StorageError::TransactionFailed("database connection mutex poisoned".into())
        })?;
        f(&connection)
    }

    /// Execute a short SQLite transaction and roll back on closure error.
    ///
    /// The transaction closure must contain only database work. External side
    /// effects must happen before or after the transaction, never while the
    /// database lock is held.
    pub fn with_transaction<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = self.connection.lock().map_err(|_| {
            StorageError::TransactionFailed("database connection mutex poisoned".into())
        })?;
        let transaction = connection.transaction()?;
        match f(&transaction) {
            Ok(value) => {
                transaction.commit()?;
                Ok(value)
            }
            Err(err) => {
                let _ = transaction.rollback();
                Err(err)
            }
        }
    }

    /// 查询 migration ledger。
    pub fn migration_ledger(&self) -> Result<Vec<MigrationRecord>, StorageError> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "
                select module, version, checksum, status, applied_at, error
                from schema_migrations
                order by module, version
                ",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(MigrationRecord {
                    module: row.get(0)?,
                    version: row.get(1)?,
                    checksum: row.get(2)?,
                    status: row.get(3)?,
                    applied_at: row.get(4)?,
                    error: row.get(5)?,
                })
            })?;

            let mut records = Vec::new();
            for row in rows {
                records.push(row?);
            }
            Ok(records)
        })
    }

    /// 判断表是否存在；仅用于测试和诊断。
    pub fn table_exists(&self, table_name: &str) -> Result<bool, StorageError> {
        self.with_connection(|connection| {
            let exists: i64 = connection.query_row(
                "select count(*) from sqlite_master where type = 'table' and name = ?1",
                [table_name],
                |row| row.get(0),
            )?;
            Ok(exists > 0)
        })
    }

    /// 返回数据库路径；不得直接暴露到普通 UI projection。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn initialize_storage_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "
        create table if not exists schema_migrations (
            module text not null,
            version integer not null,
            checksum text not null,
            status text not null,
            applied_at text,
            error text,
            primary key (module, version)
        );
        ",
    )?;

    connection.execute(
        "
        insert into schema_migrations (module, version, checksum, status, applied_at, error)
        values ('storage', ?1, ?2, 'applied', datetime('now'), null)
        on conflict(module, version) do nothing
        ",
        params![STORAGE_MIGRATION_VERSION, STORAGE_MIGRATION_CHECKSUM],
    )?;

    Ok(())
}
