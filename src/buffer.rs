//! Write buffering for batch database operations.
//!
//! The `WriteBuffer` batches pending writes and flushes them to the database
//! in configurable intervals or when the batch size is reached. This significantly
//! improves write performance by reducing transaction overhead.
//!
//! A background thread handles the actual flushing, allowing the main thread
//! to continue processing without blocking on database writes.

use anyhow::Result;
use heed::types::Str;
use heed::{Database, Env};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cache::CLASSES_DB;

type StrDb = Database<Str, Str>;

#[derive(Debug, Clone)]
pub struct PendingWrite {
    pub key: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct WriteBufferHandle {
    tx: Sender<PendingWrite>,
    pending: Arc<AtomicUsize>,
    gauge_path: Option<PathBuf>,
}

impl WriteBufferHandle {
    pub fn enqueue(&self, entry: PendingWrite) -> Result<()> {
        let prev = self.pending.fetch_add(1, Ordering::Relaxed);
        if prev == 0
            && let Some(path) = self.gauge_path.as_deref()
        {
            let _ = write_gauge(path, 1);
        }

        if self.tx.send(entry).is_ok() {
            return Ok(());
        }

        self.pending.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BufferConfig {
    pub batch_size: usize,
    pub flush_interval_ms: u64,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            flush_interval_ms: 50,
        }
    }
}

pub struct WriteBuffer {
    tx: Option<Sender<PendingWrite>>,
    pending: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
    gauge_path: Option<PathBuf>,
}

impl WriteBuffer {
    pub fn new(db: Arc<Env>, config: BufferConfig, gauge_path: PathBuf) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<PendingWrite>();
        let pending = Arc::new(AtomicUsize::new(0));
        let pending_for_thread = Arc::clone(&pending);
        let handle = spawn_flusher(rx, db, config, pending_for_thread, Some(gauge_path.clone()));

        Self {
            tx: Some(tx),
            pending,
            handle: Some(handle),
            gauge_path: Some(gauge_path),
        }
    }

    pub fn enqueue(&self, entry: PendingWrite) -> Result<()> {
        if let Some(tx) = self.tx.as_ref() {
            let prev = self.pending.fetch_add(1, Ordering::Relaxed);
            if prev == 0
                && let Some(path) = self.gauge_path.as_deref()
            {
                let _ = write_gauge(path, 1);
            }
            if tx.send(entry).is_ok() {
                return Ok(());
            }
            self.pending.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn handle(&self) -> Option<WriteBufferHandle> {
        self.tx.as_ref().map(|tx| WriteBufferHandle {
            tx: tx.clone(),
            pending: Arc::clone(&self.pending),
            gauge_path: self.gauge_path.clone(),
        })
    }

    pub fn pending_count(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    pub fn shutdown_and_flush(&mut self) -> Result<()> {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        if let Some(path) = self.gauge_path.as_deref() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}

fn spawn_flusher(
    rx: Receiver<PendingWrite>,
    db: Arc<Env>,
    config: BufferConfig,
    pending: Arc<AtomicUsize>,
    gauge_path: Option<PathBuf>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut batch = Vec::with_capacity(config.batch_size.max(1));

        loop {
            while let Ok(entry) = rx.try_recv() {
                batch.push(entry);
                if batch.len() >= config.batch_size.max(1) {
                    break;
                }
            }

            if !batch.is_empty() {
                let drained = batch.len();
                let _ = batch_write(&db, &batch);
                pending.fetch_sub(drained, Ordering::Relaxed);
                if let Some(path) = gauge_path.as_deref() {
                    let _ = write_gauge(path, pending.load(Ordering::Relaxed));
                }
                batch.clear();
            }

            match rx.recv_timeout(Duration::from_millis(config.flush_interval_ms)) {
                Ok(entry) => batch.push(entry),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    while let Ok(entry) = rx.try_recv() {
                        batch.push(entry);
                        if batch.len() >= config.batch_size.max(1) {
                            let drained = batch.len();
                            let _ = batch_write(&db, &batch);
                            pending.fetch_sub(drained, Ordering::Relaxed);
                            batch.clear();
                        }
                    }
                    if !batch.is_empty() {
                        let drained = batch.len();
                        let _ = batch_write(&db, &batch);
                        pending.fetch_sub(drained, Ordering::Relaxed);
                        batch.clear();
                    }
                    if let Some(path) = gauge_path.as_deref() {
                        let _ = write_gauge(path, 0);
                        let _ = std::fs::remove_file(path);
                    }
                    break;
                }
            }
        }
    })
}

fn batch_write(env: &Env, batch: &[PendingWrite]) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut wtxn = env.write_txn()?;
    let table: StrDb = env.create_database::<Str, Str>(&mut wtxn, Some(CLASSES_DB))?;
    for entry in batch {
        table.put(&mut wtxn, entry.key.as_str(), entry.source.as_str())?;
    }
    wtxn.commit()?;
    Ok(())
}

fn write_gauge(path: &Path, value: usize) -> Result<()> {
    std::fs::write(path, format!("{value}\n"))?;
    Ok(())
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
            "class_finder_test_{}_{}_{}.lmdb",
            std::process::id(),
            nanos,
            name
        ))
    }

    #[test]
    fn write_buffer_flushes_to_source_cache_on_shutdown() -> Result<()> {
        let db_path = temp_db_path("buffer_flush");
        let cache = PersistentCache::open(db_path.clone())?;
        let gauge = cache.pending_gauge_path();
        let mut buffer = WriteBuffer::new(
            cache.db(),
            BufferConfig {
                batch_size: 2,
                flush_interval_ms: 10_000,
            },
            gauge.clone(),
        );

        assert_eq!(buffer.pending_count(), 0);
        buffer.enqueue(PendingWrite {
            key: "a.A::jar1".to_string(),
            source: "class A {}".to_string(),
        })?;
        assert!(buffer.pending_count() >= 1);
        assert!(std::fs::read_to_string(&gauge).unwrap_or_default().trim() != "0");
        assert!(cache.stats()?.write_buffer_pending >= 1);

        buffer.shutdown_and_flush()?;
        assert_eq!(
            cache.get_class_source("a.A::jar1")?.as_deref(),
            Some("class A {}")
        );
        assert!(!gauge.exists());

        drop(cache);
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }
}
