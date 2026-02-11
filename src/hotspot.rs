use anyhow::Result;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::JAR_HOTSPOT_TABLE;
use crate::warmup::{WarmupMode, WarmupPriority};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JarHotspot {
    pub access_count: u32,
    pub last_access: u64,
    pub warmed: bool,
    pub class_count: u32,
}

#[derive(Debug, Clone)]
pub struct WarmupRequest {
    pub priority: WarmupPriority,
    pub mode: WarmupMode,
}

#[derive(Debug, Clone)]
pub struct HotspotTracker {
    db: Arc<redb::Database>,
    warmup_threshold: u32,
}

impl HotspotTracker {
    pub fn new(db: Arc<redb::Database>, warmup_threshold: u32) -> Self {
        Self {
            db,
            warmup_threshold: warmup_threshold.max(1),
        }
    }

    pub fn record_access(&self, jar_key: &str) -> Result<Option<WarmupRequest>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut hotspot = self.get_hotspot(jar_key)?.unwrap_or_default();
        hotspot.access_count = hotspot.access_count.saturating_add(1);
        hotspot.last_access = now;

        let mut request = None;
        if !hotspot.warmed {
            if hotspot.access_count >= self.warmup_threshold {
                request = Some(WarmupRequest {
                    priority: WarmupPriority::High,
                    mode: WarmupMode::AllClasses,
                });
            } else if hotspot.access_count == 1 {
                request = Some(WarmupRequest {
                    priority: WarmupPriority::Normal,
                    mode: WarmupMode::TopLevelOnly,
                });
            }
        }

        self.put_hotspot(jar_key, &hotspot)?;
        Ok(request)
    }

    pub fn mark_warmed(&self, jar_key: &str, class_count: u32) -> Result<()> {
        let mut hotspot = self.get_hotspot(jar_key)?.unwrap_or_default();
        hotspot.warmed = true;
        hotspot.class_count = class_count;
        self.put_hotspot(jar_key, &hotspot)?;
        Ok(())
    }

    pub fn get_hotspot(&self, jar_key: &str) -> Result<Option<JarHotspot>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(JAR_HOTSPOT_TABLE)?;
        Ok(table
            .get(jar_key)?
            .and_then(|v| serde_json::from_str::<JarHotspot>(v.value()).ok()))
    }

    pub fn put_hotspot(&self, jar_key: &str, value: &JarHotspot) -> Result<()> {
        let payload = serde_json::to_string(value)?;
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(JAR_HOTSPOT_TABLE)?;
            table.insert(jar_key, payload.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn top_unwarmed_jars(&self, top: usize) -> Result<Vec<String>> {
        if top == 0 {
            return Ok(Vec::new());
        }

        let txn = self.db.begin_read()?;
        let table = txn.open_table(JAR_HOTSPOT_TABLE)?;
        let mut entries: Vec<(u32, u64, String)> = Vec::new();
        for item in table.iter()? {
            let (k, v) = item?;
            let jar_key = k.value().to_string();
            let Ok(h) = serde_json::from_str::<JarHotspot>(v.value()) else {
                continue;
            };
            if h.warmed || h.access_count == 0 {
                continue;
            }
            entries.push((h.access_count, h.last_access, jar_key));
        }

        entries.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        Ok(entries.into_iter().take(top).map(|e| e.2).collect())
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
    fn hotspot_records_access_and_triggers_threshold() -> Result<()> {
        let db_path = temp_db_path("hotspot_threshold");
        let cache = PersistentCache::open(db_path)?;
        let tracker = HotspotTracker::new(cache.db(), 2);

        let jar = "a.jar";
        let first = tracker.record_access(jar)?;
        assert!(matches!(
            first,
            Some(WarmupRequest {
                priority: WarmupPriority::Normal,
                mode: WarmupMode::TopLevelOnly
            })
        ));

        let second = tracker.record_access(jar)?;
        assert!(matches!(
            second,
            Some(WarmupRequest {
                priority: WarmupPriority::High,
                mode: WarmupMode::AllClasses
            })
        ));

        tracker.mark_warmed(jar, 10)?;
        let third = tracker.record_access(jar)?;
        assert!(third.is_none());
        Ok(())
    }
}
