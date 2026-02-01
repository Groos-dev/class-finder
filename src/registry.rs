use anyhow::{Context, Result};
use redb::{Database, ReadableTable};
use std::sync::Arc;

use crate::cache::{ARTIFACT_MANIFEST_TABLE, CLASS_REGISTRY_TABLE};

#[derive(Clone)]
pub struct ClassRegistry {
    db: Arc<Database>,
}

impl ClassRegistry {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn get_artifacts(&self, fqn: &str) -> Result<Vec<String>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CLASS_REGISTRY_TABLE)?;
        let Some(value) = table.get(fqn)? else {
            return Ok(Vec::new());
        };
        let raw = value.value();
        serde_json::from_str(raw)
            .with_context(|| format!("Failed to parse artifact list for class: {}", fqn))
    }

    pub fn is_cataloged(&self, jar_key: &str) -> Result<bool> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
        Ok(table.get(jar_key)?.is_some())
    }

    pub fn update_registry_and_mark_cataloged(
        &self,
        jar_key: &str,
        classes: &[String],
    ) -> Result<usize> {
        let txn = self.db.begin_write()?;
        let updated = {
            let mut registry = txn.open_table(CLASS_REGISTRY_TABLE)?;
            let mut updated = 0usize;

            for class in classes {
                let mut paths: Vec<String> = registry
                    .get(class.as_str())?
                    .and_then(|v| serde_json::from_str::<Vec<String>>(v.value()).ok())
                    .unwrap_or_default();

                if !paths.iter().any(|p| p == jar_key) {
                    paths.push(jar_key.to_string());
                    let json = serde_json::to_string(&paths)?;
                    registry.insert(class.as_str(), json.as_str())?;
                    updated += 1;
                }
            }

            let mut manifest = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
            manifest.insert(jar_key, "1")?;

            updated
        };
        txn.commit()?;
        Ok(updated)
    }

    pub fn indexed_classes(&self) -> Result<u64> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CLASS_REGISTRY_TABLE)?;
        Ok(table.iter()?.count() as u64)
    }

    pub fn cataloged_jars(&self) -> Result<u64> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
        Ok(table.iter()?.count() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PersistentCache;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "class_finder_test_{}_{}_{}.redb",
            std::process::id(),
            nanos,
            name
        ))
    }

    #[test]
    fn update_registry_appends_and_dedupes_paths() -> Result<()> {
        let db_path = temp_db_path("registry_append");
        let cache = PersistentCache::open(db_path.clone())?;
        let registry = ClassRegistry::new(cache.db());

        let classes = vec!["a.A".to_string(), "a.B".to_string()];
        registry.update_registry_and_mark_cataloged("jar1", &classes)?;
        assert!(registry.is_cataloged("jar1")?);
        assert_eq!(registry.get_artifacts("a.A")?, vec!["jar1".to_string()]);

        registry.update_registry_and_mark_cataloged("jar1", &classes)?;
        assert_eq!(registry.get_artifacts("a.A")?, vec!["jar1".to_string()]);

        registry.update_registry_and_mark_cataloged("jar2", &["a.A".to_string()])?;
        assert_eq!(
            registry.get_artifacts("a.A")?,
            vec!["jar1".to_string(), "jar2".to_string()]
        );

        drop(registry);
        drop(cache);
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }
}
