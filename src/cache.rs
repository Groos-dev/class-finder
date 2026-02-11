//! Persistent cache for decompiled Java sources and metadata.
//!
//! Uses LMDB (via heed) for efficient key-value storage with ACID guarantees.
//! Stores decompiled class sources, JAR load status, class registry,
//! artifact manifests, hotspot tracking, and modification times.

use anyhow::{Context, Result};
use heed::types::Str;
use heed::{Database, Env, EnvFlags, EnvOpenOptions, RoTxn};
use std::path::PathBuf;
use std::sync::Arc;

pub const CLASSES_DB: &str = "classes";
pub const JARS_DB: &str = "jars";
pub const CLASS_REGISTRY_DB: &str = "class_registry";
pub const ARTIFACT_MANIFEST_DB: &str = "artifact_manifest";
pub const JAR_HOTSPOT_DB: &str = "jar_hotspot";
pub const JAR_MTIME_DB: &str = "jar_mtime";

const DEFAULT_MAP_SIZE: usize = 1024 * 1024 * 1024;
const DEFAULT_MAX_DBS: u32 = 32;

type StrDb = Database<Str, Str>;

#[derive(Debug)]
pub struct PersistentCache {
    env: Arc<Env>,
    db_path: PathBuf,
    classes: StrDb,
    jars: StrDb,
    class_registry: StrDb,
    artifact_manifest: StrDb,
    jar_hotspot: StrDb,
}

#[derive(Debug)]
pub struct ReadOnlyCache {
    inner: PersistentCache,
}

impl PersistentCache {
    pub fn open(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create cache directory: {}", parent.display())
            })?;
        }

        let env = open_env(&db_path)?;
        let env = Arc::new(env);

        let mut wtxn = env.write_txn()?;
        let classes = env.create_database::<Str, Str>(&mut wtxn, Some(CLASSES_DB))?;
        let jars = env.create_database::<Str, Str>(&mut wtxn, Some(JARS_DB))?;
        let class_registry = env.create_database::<Str, Str>(&mut wtxn, Some(CLASS_REGISTRY_DB))?;
        let artifact_manifest =
            env.create_database::<Str, Str>(&mut wtxn, Some(ARTIFACT_MANIFEST_DB))?;
        let jar_hotspot = env.create_database::<Str, Str>(&mut wtxn, Some(JAR_HOTSPOT_DB))?;
        let _jar_mtime = env.create_database::<Str, Str>(&mut wtxn, Some(JAR_MTIME_DB))?;
        wtxn.commit()?;

        Ok(Self {
            env,
            db_path,
            classes,
            jars,
            class_registry,
            artifact_manifest,
            jar_hotspot,
        })
    }

    pub fn db(&self) -> Arc<Env> {
        Arc::clone(&self.env)
    }

    pub fn pending_gauge_path(&self) -> PathBuf {
        let mut os = self.db_path.clone().into_os_string();
        os.push(".pending");
        PathBuf::from(os)
    }

    pub fn get_class_source(&self, key: &str) -> Result<Option<String>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.classes.get(&rtxn, key)?.map(|v| v.to_string()))
    }

    pub fn put_class_sources(&self, entries: &[(String, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let mut wtxn = self.env.write_txn()?;
        for (k, v) in entries {
            self.classes.put(&mut wtxn, k.as_str(), v.as_str())?;
        }
        wtxn.commit()?;
        Ok(entries.len())
    }

    pub fn is_jar_loaded(&self, jar_key: &str) -> Result<bool> {
        let rtxn = self.env.read_txn()?;
        Ok(self.jars.get(&rtxn, jar_key)?.is_some())
    }

    pub fn mark_jar_loaded(&self, jar_key: &str) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.jars.put(&mut wtxn, jar_key, "1")?;
        wtxn.commit()?;
        Ok(())
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let rtxn = self.env.read_txn()?;

        let source_entries = table_len(&self.classes, &rtxn)?;
        let loaded_jars = table_len(&self.jars, &rtxn)?;
        let indexed_classes = table_len(&self.class_registry, &rtxn)?;
        let cataloged_jars = table_len(&self.artifact_manifest, &rtxn)?;
        let hotspot_jars = table_len(&self.jar_hotspot, &rtxn)?;
        let mut warmed_jars = 0u64;
        let mut hotspot_top = Vec::new();
        for item in self.jar_hotspot.iter(&rtxn)? {
            let (k, v) = item?;
            let Ok(h) = serde_json::from_str::<JarHotspotRow>(v) else {
                continue;
            };
            if h.warmed {
                warmed_jars += 1;
            }
            hotspot_top.push(HotspotTopEntry {
                jar_path: k.to_string(),
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

impl ReadOnlyCache {
    pub fn open(db_path: PathBuf) -> Result<Self> {
        let inner = PersistentCache::open(db_path)?;
        Ok(Self { inner })
    }

    pub fn db(&self) -> Arc<Env> {
        self.inner.db()
    }

    pub fn get_class_source(&self, key: &str) -> Result<Option<String>> {
        self.inner.get_class_source(key)
    }

    pub fn stats(&self) -> Result<CacheStats> {
        self.inner.stats()
    }
}

fn open_env(db_path: &PathBuf) -> Result<Env> {
    let mut options = EnvOpenOptions::new();
    options.map_size(DEFAULT_MAP_SIZE);
    options.max_dbs(DEFAULT_MAX_DBS);
    // SAFETY: We do not use NO_LOCK and keep default LMDB locking guarantees.
    // NO_SUB_DIR preserves current single-path CLI behavior for --db.
    unsafe {
        options.flags(EnvFlags::NO_SUB_DIR);
        options
            .open(db_path)
            .with_context(|| format!("Failed to create/open db env: {}", db_path.display()))
    }
}

fn table_len(db: &StrDb, rtxn: &RoTxn<'_>) -> Result<u64> {
    let mut count = 0u64;
    for item in db.iter(rtxn)? {
        let _ = item?;
        count += 1;
    }
    Ok(count)
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
