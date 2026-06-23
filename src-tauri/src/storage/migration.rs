//! Storage 基础 migration ledger。

/// R1 storage 基础设施 migration 版本。
pub const STORAGE_MIGRATION_VERSION: i64 = 1;

/// R1 storage 基础设施 migration 校验摘要。
pub const STORAGE_MIGRATION_CHECKSUM: &str = "storage-v1-r1-ledger";

/// migration ledger 的去敏查询投影。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationRecord {
    pub module: String,
    pub version: i64,
    pub checksum: String,
    pub status: String,
    pub applied_at: Option<String>,
    pub error: Option<String>,
}
