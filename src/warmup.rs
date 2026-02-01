//! Background warmup system for preloading frequently accessed JARs.
//!
//! The warmer maintains a priority queue of warmup tasks and executes them
//! using a thread pool. Tasks are prioritized by access frequency and can
//! operate in two modes:
//!
//! - `TopLevelOnly`: Decompile only top-level classes (fast)
//! - `AllClasses`: Decompile all classes including inner classes (thorough)
//!
//! The warmer coordinates with the hotspot tracker to identify which JARs
//! should be warmed based on access patterns.

use anyhow::Result;
use rayon::ThreadPool;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::buffer::{PendingWrite, WriteBufferHandle};
use crate::cfr::Cfr;
use crate::hotspot::HotspotTracker;
use crate::parse::parse_decompiled_output;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmupMode {
    TopLevelOnly,
    AllClasses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarmupPriority {
    Low = 0,
    Normal = 1,
    High = 2,
}

#[derive(Debug, Clone)]
pub struct WarmupTask {
    pub jar_path: PathBuf,
    pub priority: WarmupPriority,
    pub mode: WarmupMode,
    pub exclude_fqns: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct WarmerConfig {
    pub max_concurrent: usize,
    pub poll_interval_ms: u64,
}

impl Default for WarmerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            poll_interval_ms: 50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WarmerStats {
    pub pending_tasks: Arc<AtomicUsize>,
    pub running_tasks: Arc<AtomicUsize>,
    pub completed_tasks: Arc<AtomicU64>,
    pub failed_tasks: Arc<AtomicU64>,
}

impl WarmerStats {
    fn new() -> Self {
        Self {
            pending_tasks: Arc::new(AtomicUsize::new(0)),
            running_tasks: Arc::new(AtomicUsize::new(0)),
            completed_tasks: Arc::new(AtomicU64::new(0)),
            failed_tasks: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub struct Warmer {
    tx: Option<Sender<WarmupTask>>,
    stats: WarmerStats,
    handle: Option<JoinHandle<()>>,
}

impl Warmer {
    pub fn new(
        cfr: Cfr,
        buffer: WriteBufferHandle,
        hotspot: Option<HotspotTracker>,
        config: WarmerConfig,
    ) -> Result<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<WarmupTask>();
        let stats = WarmerStats::new();
        let handle = spawn_warmer(rx, cfr, buffer, hotspot, config, stats.clone());
        Ok(Self {
            tx: Some(tx),
            stats,
            handle: Some(handle),
        })
    }

    pub fn submit(&self, task: WarmupTask) -> Result<()> {
        if let Some(tx) = self.tx.as_ref() {
            tx.send(task)?;
            self.stats
                .pending_tasks
                .fetch_add(1, AtomicOrdering::Relaxed);
        }
        Ok(())
    }

    pub fn stats(&self) -> WarmerStats {
        self.stats.clone()
    }

    pub fn shutdown_and_drain(&mut self) -> Result<()> {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        Ok(())
    }
}

#[derive(Debug)]
struct QueuedTask {
    priority: WarmupPriority,
    seq: u64,
    task: WarmupTask,
}

impl PartialEq for QueuedTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}

impl Eq for QueuedTask {}

impl PartialOrd for QueuedTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedTask {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

fn spawn_warmer(
    rx: Receiver<WarmupTask>,
    cfr: Cfr,
    buffer: WriteBufferHandle,
    hotspot: Option<HotspotTracker>,
    config: WarmerConfig,
    stats: WarmerStats,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.max_concurrent.max(1))
            .build()
            .unwrap();
        let mut queue: BinaryHeap<QueuedTask> = BinaryHeap::new();
        let mut in_flight: HashSet<PathBuf> = HashSet::new();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<PathBuf>();
        let next_seq = AtomicU64::new(0);
        let draining = AtomicBool::new(false);

        loop {
            while let Ok(done) = done_rx.try_recv() {
                in_flight.remove(&done);
            }

            match rx.recv_timeout(Duration::from_millis(config.poll_interval_ms)) {
                Ok(task) => {
                    let seq = next_seq.fetch_add(1, AtomicOrdering::Relaxed);
                    queue.push(QueuedTask {
                        priority: task.priority,
                        seq,
                        task,
                    });
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    draining.store(true, AtomicOrdering::Relaxed);
                }
            }

            while stats.running_tasks.load(AtomicOrdering::Relaxed) < config.max_concurrent.max(1) {
                let Some(queued) = queue.pop() else { break };
                if in_flight.contains(&queued.task.jar_path) {
                    stats.pending_tasks.fetch_sub(1, AtomicOrdering::Relaxed);
                    continue;
                }
                in_flight.insert(queued.task.jar_path.clone());

                stats.pending_tasks.fetch_sub(1, AtomicOrdering::Relaxed);
                stats.running_tasks.fetch_add(1, AtomicOrdering::Relaxed);

                let cfr = cfr.clone();
                let buffer = buffer.clone();
                let stats = stats.clone();
                let done_tx = done_tx.clone();
                let hotspot = hotspot.clone();
                let jar_path = queued.task.jar_path.clone();
                let mode = queued.task.mode;
                let exclude_fqns = queued.task.exclude_fqns.clone();

                spawn_on_pool(&pool, move || {
                    let outcome =
                        warmup_jar(&cfr, &buffer, jar_path.as_path(), mode, &exclude_fqns);
                    match outcome {
                        Ok(class_count) => {
                            stats.completed_tasks.fetch_add(1, AtomicOrdering::Relaxed);
                            if let Some(hotspot) = hotspot.as_ref() {
                                let jar_key = jar_path.to_string_lossy().to_string();
                                let _ = hotspot.mark_warmed(&jar_key, class_count as u32);
                            }
                        }
                        Err(_) => {
                            stats.failed_tasks.fetch_add(1, AtomicOrdering::Relaxed);
                        }
                    }

                    stats.running_tasks.fetch_sub(1, AtomicOrdering::Relaxed);
                    let _ = done_tx.send(jar_path);
                });
            }

            if draining.load(AtomicOrdering::Relaxed)
                && queue.is_empty()
                && stats.running_tasks.load(AtomicOrdering::Relaxed) == 0
            {
                break;
            }
        }
    })
}

fn spawn_on_pool(pool: &ThreadPool, f: impl FnOnce() + Send + 'static) {
    pool.spawn(f);
}

fn warmup_jar(
    cfr: &Cfr,
    buffer: &WriteBufferHandle,
    jar_path: &Path,
    mode: WarmupMode,
    exclude_fqns: &HashSet<String>,
) -> Result<usize> {
    let jar_key = jar_path.to_string_lossy().to_string();
    let decompiled = cfr.decompile_jar(jar_path)?;
    let classes = parse_decompiled_output(&decompiled);
    let class_count = classes.len();

    for cls in classes {
        if exclude_fqns.contains(&cls.class_name) {
            continue;
        }
        if mode == WarmupMode::TopLevelOnly && cls.class_name.contains('$') {
            continue;
        }
        if cls.class_name.ends_with("package-info") || cls.class_name.ends_with("module-info") {
            continue;
        }

        let key = format!("{}::{jar_key}", cls.class_name);
        let _ = buffer.enqueue(PendingWrite {
            key,
            source: cls.content,
        });
    }

    Ok(class_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_task_orders_by_priority_then_fifo() {
        let dummy = WarmupTask {
            jar_path: PathBuf::from("a.jar"),
            priority: WarmupPriority::Low,
            mode: WarmupMode::TopLevelOnly,
            exclude_fqns: HashSet::new(),
        };

        let mut heap = BinaryHeap::new();
        heap.push(QueuedTask {
            priority: WarmupPriority::Normal,
            seq: 0,
            task: dummy.clone(),
        });
        heap.push(QueuedTask {
            priority: WarmupPriority::High,
            seq: 2,
            task: dummy.clone(),
        });
        heap.push(QueuedTask {
            priority: WarmupPriority::High,
            seq: 1,
            task: dummy.clone(),
        });

        let first = heap.pop().unwrap();
        assert_eq!(first.priority, WarmupPriority::High);
        assert_eq!(first.seq, 1);

        let second = heap.pop().unwrap();
        assert_eq!(second.priority, WarmupPriority::High);
        assert_eq!(second.seq, 2);

        let third = heap.pop().unwrap();
        assert_eq!(third.priority, WarmupPriority::Normal);
        assert_eq!(third.seq, 0);
    }
}
