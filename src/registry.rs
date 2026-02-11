use anyhow::{Context, Result};
use heed::types::Str;
use heed::{Database, Env};
use std::sync::Arc;

use crate::cache::{ARTIFACT_MANIFEST_DB, CLASS_REGISTRY_DB};

type StrDb = Database<Str, Str>;

#[derive(Clone)]
pub struct ClassRegistry {
    db: Arc<Env>,
}

#[derive(Clone)]
pub struct ReadOnlyClassRegistry {
    db: Arc<Env>,
}

impl ClassRegistry {
    pub fn new(db: Arc<Env>) -> Self {
        Self { db }
    }

    pub fn get_artifacts(&self, fqn: &str) -> Result<Vec<String>> {
        let rtxn = self.db.read_txn()?;
        let table = open_named_db(&self.db, &rtxn, CLASS_REGISTRY_DB)?;
        let Some(value) = table.get(&rtxn, fqn)? else {
            return Ok(Vec::new());
        };
        serde_json::from_str(value)
            .with_context(|| format!("Failed to parse artifact list for class: {}", fqn))
    }

    pub fn is_cataloged(&self, jar_key: &str) -> Result<bool> {
        let rtxn = self.db.read_txn()?;
        let table = open_named_db(&self.db, &rtxn, ARTIFACT_MANIFEST_DB)?;
        Ok(table.get(&rtxn, jar_key)?.is_some())
    }

    pub fn update_registry_and_mark_cataloged(
        &self,
        jar_key: &str,
        classes: &[String],
    ) -> Result<usize> {
        let mut wtxn = self.db.write_txn()?;
        let updated = {
            let registry = self
                .db
                .create_database::<Str, Str>(&mut wtxn, Some(CLASS_REGISTRY_DB))?;
            let mut updated = 0usize;

            for class in classes {
                let mut paths: Vec<String> = registry
                    .get(&wtxn, class.as_str())?
                    .and_then(|v| serde_json::from_str::<Vec<String>>(v).ok())
                    .unwrap_or_default();

                if !paths.iter().any(|p| p == jar_key) {
                    paths.push(jar_key.to_string());
                    let json = serde_json::to_string(&paths)?;
                    registry.put(&mut wtxn, class.as_str(), json.as_str())?;
                    updated += 1;
                }
            }

            let manifest = self
                .db
                .create_database::<Str, Str>(&mut wtxn, Some(ARTIFACT_MANIFEST_DB))?;
            manifest.put(&mut wtxn, jar_key, "1")?;

            updated
        };
        wtxn.commit()?;
        Ok(updated)
    }

    pub fn indexed_classes(&self) -> Result<u64> {
        let rtxn = self.db.read_txn()?;
        let table = open_named_db(&self.db, &rtxn, CLASS_REGISTRY_DB)?;
        table_len(&table, &rtxn)
    }

    pub fn cataloged_jars(&self) -> Result<u64> {
        let rtxn = self.db.read_txn()?;
        let table = open_named_db(&self.db, &rtxn, ARTIFACT_MANIFEST_DB)?;
        table_len(&table, &rtxn)
    }
}

impl ReadOnlyClassRegistry {
    pub fn new(db: Arc<Env>) -> Self {
        Self { db }
    }

    pub fn get_artifacts(&self, fqn: &str) -> Result<Vec<String>> {
        let rtxn = self.db.read_txn()?;
        let table = open_named_db(&self.db, &rtxn, CLASS_REGISTRY_DB)?;
        let Some(value) = table.get(&rtxn, fqn)? else {
            return Ok(Vec::new());
        };
        serde_json::from_str(value)
            .with_context(|| format!("Failed to parse artifact list for class: {}", fqn))
    }
}

fn open_named_db(env: &Env, rtxn: &heed::RoTxn<'_>, name: &str) -> Result<StrDb> {
    env.open_database::<Str, Str>(rtxn, Some(name))?
        .with_context(|| format!("Database not found: {name}"))
}

fn table_len(db: &StrDb, rtxn: &heed::RoTxn<'_>) -> Result<u64> {
    let mut count = 0u64;
    for item in db.iter(rtxn)? {
        let _ = item?;
        count += 1;
    }
    Ok(count)
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
