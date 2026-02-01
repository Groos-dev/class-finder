use anyhow::Result;
use redb::ReadableTable;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cache::JAR_MTIME_TABLE;
use crate::catalog;
use crate::registry::ClassRegistry;
use crate::scan::scan_jars;

#[derive(Debug, Clone, Copy)]
pub struct IncrementalConfig {
    pub interval: Duration,
}

impl Default for IncrementalConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct IncrementalIndexResult {
    pub root: String,
    pub scanned_jars: usize,
    pub changed_jars: usize,
    pub indexed_classes: usize,
    pub failed_jars: usize,
}

#[derive(Clone)]
pub struct IncrementalIndexer {
    db: Arc<redb::Database>,
    root: PathBuf,
}

impl IncrementalIndexer {
    pub fn new(db: Arc<redb::Database>, root: PathBuf) -> Self {
        Self { db, root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn scan_changes(&self) -> Result<(usize, Vec<PathBuf>)> {
        let jars = scan_jars(&self.root)?;
        let txn = self.db.begin_write()?;
        let mut changed = Vec::new();
        {
            let mut table = txn.open_table(JAR_MTIME_TABLE)?;
            for jar_path in jars.iter() {
                let jar_key = jar_path.to_string_lossy().to_string();
                let mtime = jar_path
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let nanos = mtime
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let nanos_u64 = u64::try_from(nanos).unwrap_or(u64::MAX);

                let old = table
                    .get(jar_key.as_str())?
                    .and_then(|v| v.value().parse::<u64>().ok())
                    .unwrap_or(0);
                if old < nanos_u64 {
                    changed.push(jar_path.clone());
                }

                let value = nanos_u64.to_string();
                table.insert(jar_key.as_str(), value.as_str())?;
            }
        }
        txn.commit()?;
        Ok((jars.len(), changed))
    }

    pub fn run_once(&self, registry: &ClassRegistry) -> Result<IncrementalIndexResult> {
        let (scanned_jars, changed) = self.scan_changes()?;
        let mut indexed_classes = 0usize;
        let mut failed_jars = 0usize;

        for jar_path in changed.iter() {
            let jar_key = jar_path.to_string_lossy().to_string();
            match catalog::catalog(jar_path) {
                Ok(classes) => {
                    indexed_classes += classes.len();
                    let _ = registry.update_registry_and_mark_cataloged(&jar_key, &classes);
                }
                Err(_) => {
                    failed_jars += 1;
                }
            }
        }

        Ok(IncrementalIndexResult {
            root: self.root.to_string_lossy().to_string(),
            scanned_jars,
            changed_jars: changed.len(),
            indexed_classes,
            failed_jars,
        })
    }

    pub fn spawn(self, registry: ClassRegistry, config: IncrementalConfig) -> JoinHandle<()> {
        std::thread::spawn(move || {
            loop {
                let _ = self.run_once(&registry);
                std::thread::sleep(config.interval);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PersistentCache;

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
    fn scan_changes_detects_new_and_modified_jars() -> Result<()> {
        let base = std::env::temp_dir().join(format!(
            "class-finder-incremental-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let m2 = base.join("repository");
        std::fs::create_dir_all(&m2)?;

        let jar = m2.join("a.jar");
        std::fs::write(&jar, b"x")?;

        let cache = PersistentCache::open(temp_db_path("incremental_changes"))?;
        let indexer = IncrementalIndexer::new(cache.db(), m2.clone());

        let (_, changed1) = indexer.scan_changes()?;
        assert_eq!(changed1.len(), 1);

        let (_, changed2) = indexer.scan_changes()?;
        assert_eq!(changed2.len(), 0);

        std::thread::sleep(Duration::from_millis(2));
        std::fs::write(&jar, b"y")?;
        let (_, changed3) = indexer.scan_changes()?;
        assert_eq!(changed3.len(), 1);

        let _ = std::fs::remove_dir_all(base);
        Ok(())
    }
}
