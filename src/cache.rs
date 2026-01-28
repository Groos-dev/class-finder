use anyhow::{Context, Result};
use redb::{Database, ReadableTable, TableDefinition};
use std::path::{Path, PathBuf};

pub const CLASSES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("classes");
pub const JARS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("jars");

#[derive(Debug)]
pub struct PersistentCache {
    db: Database,
    db_path: PathBuf,
}

impl PersistentCache {
    pub fn open(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("无法创建缓存目录: {}", parent.display()))?;
        }

        let db = Database::create(&db_path).with_context(|| format!("无法创建/打开 redb: {}", db_path.display()))?;
        {
            let txn = db.begin_write()?;
            let _ = txn.open_table(CLASSES_TABLE)?;
            let _ = txn.open_table(JARS_TABLE)?;
            txn.commit()?;
        }
        Ok(Self { db, db_path })
    }

    pub fn path(&self) -> &Path {
        &self.db_path
    }

    pub fn get_class_source(&self, key: &str) -> Result<Option<String>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CLASSES_TABLE)?;
        Ok(table.get(key)?.map(|v| v.value().to_string()))
    }

    pub fn put_class_sources(&self, entries: &[(String, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(CLASSES_TABLE)?;
            for (k, v) in entries {
                table.insert(k.as_str(), v.as_str())?;
            }
        }
        txn.commit()?;
        Ok(entries.len())
    }

    pub fn is_jar_loaded(&self, jar_key: &str) -> Result<bool> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(JARS_TABLE)?;
        Ok(table.get(jar_key)?.is_some())
    }

    pub fn mark_jar_loaded(&self, jar_key: &str) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(JARS_TABLE)?;
            table.insert(jar_key, "1")?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let txn = self.db.begin_read()?;
        let classes = txn.open_table(CLASSES_TABLE)?;
        let jars = txn.open_table(JARS_TABLE)?;
        let classes_count = classes.iter()?.count() as u64;
        let jars_count = jars.iter()?.count() as u64;
        Ok(CacheStats {
            db_path: self.db_path.to_string_lossy().to_string(),
            classes: classes_count,
            jars: jars_count,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct CacheStats {
    pub db_path: String,
    pub classes: u64,
    pub jars: u64,
}
