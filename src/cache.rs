//! Persistent cache for decompiled Java sources and metadata.
//!
//! Uses redb for efficient key-value storage with ACID guarantees.
//! Stores decompiled class sources, JAR load status, class registry,
//! artifact manifests, hotspot tracking, and modification times.

use anyhow::{Context, Result};
use redb::{Database, ReadableTable, TableDefinition};
use std::path::PathBuf;
use std::sync::Arc;

pub const CLASSES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("classes");
pub const JARS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("jars");
pub const CLASS_REGISTRY_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("class_registry");
pub const ARTIFACT_MANIFEST_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("artifact_manifest");
pub const JAR_HOTSPOT_TABLE: TableDefinition<&str, &str> = TableDefinition::new("jar_hotspot");
pub const JAR_MTIME_TABLE: TableDefinition<&str, &str> = TableDefinition::new("jar_mtime");

#[derive(Debug)]
pub struct PersistentCache {
    db: Arc<Database>,
    db_path: PathBuf,
}

impl PersistentCache {
    pub fn open(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("无法创建缓存目录: {}", parent.display()))?;
        }

        let db = Database::create(&db_path)
            .with_context(|| format!("无法创建/打开 redb: {}", db_path.display()))?;
        let db = Arc::new(db);
        {
            let txn = db.begin_write()?;
            let _ = txn.open_table(CLASSES_TABLE)?;
            let _ = txn.open_table(JARS_TABLE)?;
            let _ = txn.open_table(CLASS_REGISTRY_TABLE)?;
            let _ = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
            let _ = txn.open_table(JAR_HOTSPOT_TABLE)?;
            let _ = txn.open_table(JAR_MTIME_TABLE)?;
            txn.commit()?;
        }
        Ok(Self { db, db_path })
    }

    pub fn db(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }

    pub fn pending_gauge_path(&self) -> PathBuf {
        let mut os = self.db_path.clone().into_os_string();
        os.push(".pending");
        PathBuf::from(os)
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
        let source_cache = txn.open_table(CLASSES_TABLE)?;
        let loaded_jars = txn.open_table(JARS_TABLE)?;
        let class_registry = txn.open_table(CLASS_REGISTRY_TABLE)?;
        let artifact_manifest = txn.open_table(ARTIFACT_MANIFEST_TABLE)?;
        let jar_hotspot = txn.open_table(JAR_HOTSPOT_TABLE)?;

        let source_entries = source_cache.iter()?.count() as u64;
        let loaded_jars = loaded_jars.iter()?.count() as u64;
        let indexed_classes = class_registry.iter()?.count() as u64;
        let cataloged_jars = artifact_manifest.iter()?.count() as u64;
        let hotspot_jars = jar_hotspot.iter()?.count() as u64;
        let mut warmed_jars = 0u64;
        let mut hotspot_top = Vec::new();
        for item in jar_hotspot.iter()? {
            let (k, v) = item?;
            let Ok(h) = serde_json::from_str::<JarHotspotRow>(v.value()) else {
                continue;
            };
            if h.warmed {
                warmed_jars += 1;
            }
            hotspot_top.push(HotspotTopEntry {
                jar_path: k.value().to_string(),
                access_count: h.access_count,
                last_access: h.last_access,
                warmed: h.warmed,
            });
        }
        hotspot_top.sort_by(|a, b| {
            b.access_count
                .cmp(&a.access_count)
                .then_with(|| b.last_access.cmp(&a.last_access))
                .then_with(|| a.jar_path.cmp(&b.jar_path))
        });
        hotspot_top.truncate(10);
        let write_buffer_pending = std::fs::read_to_string(self.pending_gauge_path())
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        Ok(CacheStats {
            db_path: self.db_path.to_string_lossy().to_string(),
            source_entries,
            indexed_classes,
            cataloged_jars,
            loaded_jars,
            write_buffer_pending,
            hotspot_jars,
            warmed_jars,
            warmup_threshold: 2,
            warmup_pending_tasks: 0,
            hotspot_top,
        })
    }
}

#[derive(Debug, serde::Deserialize)]
struct JarHotspotRow {
    access_count: u32,
    last_access: u64,
    warmed: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct HotspotTopEntry {
    pub jar_path: String,
    pub access_count: u32,
    pub last_access: u64,
    pub warmed: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct CacheStats {
    pub db_path: String,
    pub source_entries: u64,
    pub indexed_classes: u64,
    pub cataloged_jars: u64,
    pub loaded_jars: u64,
    pub write_buffer_pending: u64,
    pub hotspot_jars: u64,
    pub warmed_jars: u64,
    pub warmup_threshold: u32,
    pub warmup_pending_tasks: u64,
    pub hotspot_top: Vec<HotspotTopEntry>,
}
