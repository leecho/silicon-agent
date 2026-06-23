//! Provider secret 文件存储（按 provider_id 分键）。
//!
//! 全部厂商密钥存于单个 JSON 文件 `{ provider_id: secret }`；SQLite 只保存非敏感元数据
//! 与掩码提示。文件权限 0600。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 按 provider_id 分键的 secret 文件存储。
#[derive(Debug, Clone)]
pub struct FileSecretStore {
    path: PathBuf,
}

impl FileSecretStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// 读取整张密钥表；文件缺失或解析失败返回空表（不报错，容错优先）。
    fn load(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str::<BTreeMap<String, String>>(&raw).ok())
            .unwrap_or_default()
    }

    /// 写回整张密钥表：先写同目录临时文件（unix 上以 0600 创建，避免世界可读窗口），
    /// 再 rename 覆盖目标（同盘原子），保证崩溃不会损坏整张 keystore。
    fn save(&self, map: &BTreeMap<String, String>) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create secret directory: {err}"))?;
        }
        let raw = serde_json::to_string(map).map_err(|err| format!("serialize secrets: {err}"))?;
        let tmp = {
            let mut p = self.path.clone().into_os_string();
            p.push(".tmp");
            std::path::PathBuf::from(p)
        };
        {
            use std::io::Write;
            #[cfg(unix)]
            let mut file = {
                use std::os::unix::fs::OpenOptionsExt;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&tmp)
                    .map_err(|err| format!("open provider secret temp: {err}"))?
            };
            #[cfg(not(unix))]
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|err| format!("open provider secret temp: {err}"))?;
            file.write_all(raw.as_bytes())
                .map_err(|err| format!("write provider secret: {err}"))?;
            file.flush()
                .map_err(|err| format!("flush provider secret: {err}"))?;
        }
        std::fs::rename(&tmp, &self.path).map_err(|err| {
            let _ = std::fs::remove_file(&tmp);
            format!("commit provider secret: {err}")
        })?;
        Ok(())
    }

    pub fn set(&self, provider_id: &str, value: &str) -> Result<(), String> {
        let mut map = self.load();
        map.insert(provider_id.to_string(), value.to_string());
        self.save(&map)
    }

    pub fn clear(&self, provider_id: &str) -> Result<(), String> {
        let mut map = self.load();
        if map.remove(provider_id).is_some() {
            self.save(&map)?;
        }
        Ok(())
    }

    pub fn has_secret(&self, provider_id: &str) -> bool {
        self.load()
            .get(provider_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    pub fn hint(&self, provider_id: &str) -> Option<String> {
        let map = self.load();
        let value = map.get(provider_id)?;
        if value.is_empty() {
            return None;
        }
        let suffix: String = value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Some(format!("****{suffix}"))
    }

    pub(crate) fn read(&self, provider_id: &str) -> Result<String, String> {
        self.load()
            .get(provider_id)
            .cloned()
            .ok_or_else(|| format!("provider secret not found: {provider_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::FileSecretStore;

    fn temp_path() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sw-secrets-{nanos}.json"))
    }

    #[test]
    fn keyed_set_read_clear_per_provider() {
        let store = FileSecretStore::new(temp_path());
        assert!(!store.has_secret("p1"));
        store.set("p1", "sk-aaaa1111").unwrap();
        store.set("p2", "sk-bbbb2222").unwrap();
        assert!(store.has_secret("p1"));
        assert_eq!(store.read("p1").unwrap(), "sk-aaaa1111");
        assert_eq!(store.read("p2").unwrap(), "sk-bbbb2222");
        assert_eq!(store.hint("p1"), Some("****1111".to_string()));
        // 清一个不影响另一个。
        store.clear("p1").unwrap();
        assert!(!store.has_secret("p1"));
        assert!(store.has_secret("p2"));
    }
}
