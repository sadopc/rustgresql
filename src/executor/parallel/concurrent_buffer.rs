//! Concurrent buffer pool for parallel query execution
//!
//! This module extends the base buffer pool with sharding and lock-free techniques
//! to support high-concurrency access patterns in parallel query processing.

use crate::error::Result;
use crate::PageId;
use crate::storage::{BufferPoolManager, FileManager, Page};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Configuration for concurrent buffer pool
#[derive(Debug, Clone)]
pub struct ConcurrentBufferPoolConfig {
    /// Total number of buffer frames across all shards
    pub total_capacity: usize,
    /// Number of shards (should be power of 2 for optimal performance)
    pub shard_count: usize,
    /// Maximum number of retry attempts for lock-free operations
    pub max_retries: usize,
    /// Backoff strategy for contended operations
    pub backoff_strategy: BackoffStrategy,
    /// Statistics collection interval
    pub stats_interval: Duration,
}

impl Default for ConcurrentBufferPoolConfig {
    fn default() -> Self {
        let num_cpus = num_cpus::get();
        Self {
            total_capacity: 10000,
            shard_count: num_cpus.next_power_of_two(),
            max_retries: 10,
            backoff_strategy: BackoffStrategy::Exponential,
            stats_interval: Duration::from_secs(1),
        }
    }
}

/// Backoff strategy for handling contention
#[derive(Debug, Clone)]
pub enum BackoffStrategy {
    /// No backoff - spin wait
    None,
    /// Fixed delay
    Fixed(Duration),
    /// Exponential backoff with jitter
    Exponential,
    /// Adaptive based on contention level
    Adaptive,
}

/// Statistics for a single shard
#[derive(Debug, Default)]
pub struct ShardStats {
    /// Number of cache hits
    pub hits: AtomicU64,
    /// Number of cache misses
    pub misses: AtomicU64,
    /// Number of evictions
    pub evictions: AtomicU64,
    /// Number of contentions (retry attempts)
    pub contentions: AtomicU64,
    /// Average lock acquisition time (microseconds)
    pub avg_lock_time: AtomicU64,
    /// Current number of pages in shard
    pub current_size: AtomicUsize,
}

impl ShardStats {
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        if hits + misses == 0 {
            0.0
        } else {
            hits as f64 / (hits + misses) as f64
        }
    }

    pub fn contention_rate(&self) -> f64 {
        let contentions = self.contentions.load(Ordering::Relaxed);
        let total_operations = self.hits.load(Ordering::Relaxed) + self.misses.load(Ordering::Relaxed);
        if total_operations == 0 {
            0.0
        } else {
            contentions as f64 / total_operations as f64
        }
    }
}

/// A single shard of the concurrent buffer pool
#[derive(Debug)]
struct ConcurrentBufferShard {
    /// Local buffer frames for this shard
    frames: Vec<Mutex<BufferFrame>>,
    /// Local LRU list for this shard
    lru_list: Mutex<VecDeque<PageId>>,
    /// Page ID to frame index mapping for this shard
    page_to_frame: RwLock<HashMap<PageId, usize>>,
    /// Shard statistics
    stats: ShardStats,
    /// Shard ID for debugging
    shard_id: usize,
}

impl ConcurrentBufferShard {
    fn new(shard_id: usize, capacity: usize) -> Self {
        let mut frames = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            frames.push(Mutex::new(BufferFrame::new()));
        }

        Self {
            frames,
            lru_list: Mutex::new(VecDeque::new()),
            page_to_frame: RwLock::new(HashMap::new()),
            stats: ShardStats::default(),
            shard_id,
        }
    }

    /// Find an available frame within this shard
    fn find_available_frame(&self) -> Result<usize> {
        for (i, frame) in self.frames.iter().enumerate() {
            let f = frame.lock().unwrap();
            if f.page_id.is_none() {
                return Ok(i);
            }
        }

        // Try LRU eviction within shard
        {
            let mut lru_list = self.lru_list.lock().unwrap();
            let page_to_frame = self.page_to_frame.read().unwrap();

            while let Some(&page_id) = lru_list.front() {
                if let Some(&frame_idx) = page_to_frame.get(&page_id) {
                    let frame = &self.frames[frame_idx];
                    let f = frame.lock().unwrap();
                    if f.is_evictable() {
                        lru_list.pop_front();
                        self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                        return Ok(frame_idx);
                    }
                }
                lru_list.pop_front();
            }
        }

        Err(crate::error::RustgreSQLError::Storage(
            format!("No available buffer frames in shard {}", self.shard_id)
        ))
    }

    /// Update LRU list for this shard
    fn update_lru(&self, page_id: PageId) {
        let mut lru_list = self.lru_list.lock().unwrap();
        lru_list.retain(|&pid| pid != page_id);
        lru_list.push_back(page_id);
    }

    /// Get current size of this shard
    fn size(&self) -> usize {
        self.page_to_frame.read().unwrap().len()
    }
}

/// Reuse the BufferFrame from storage module by redefining it here
/// This is necessary because BufferFrame is private in the storage module
#[derive(Debug)]
struct BufferFrame {
    /// The page data
    page: Option<crate::storage::Page>,
    /// Page ID if page is loaded
    page_id: Option<PageId>,
    /// Reference count for the page
    pin_count: u32,
    /// Whether the page is dirty (needs to be written to disk)
    dirty: bool,
    /// LRU list position
    lru_index: Option<usize>,
}

impl BufferFrame {
    fn new() -> Self {
        Self {
            page: None,
            page_id: None,
            pin_count: 0,
            dirty: false,
            lru_index: None,
        }
    }

    /// Pin the page (increment reference count)
    fn pin(&mut self) {
        self.pin_count += 1;
    }

    /// Unpin the page (decrement reference count)
    fn unpin(&mut self) -> Result<()> {
        if self.pin_count == 0 {
            return Err(crate::error::RustgreSQLError::Internal(
                "Attempt to unpin page with pin_count = 0".to_string()
            ));
        }
        self.pin_count -= 1;
        Ok(())
    }

    /// Check if page is available for eviction
    fn is_evictable(&self) -> bool {
        self.pin_count == 0
    }
}

/// Concurrent buffer pool with sharding for reduced contention
#[derive(Debug)]
pub struct ConcurrentBufferPool {
    /// Shards of the buffer pool
    shards: Vec<ConcurrentBufferShard>,
    /// Configuration
    config: ConcurrentBufferPoolConfig,
    /// Global statistics
    global_stats: Arc<RwLock<ConcurrentBufferPoolStats>>,
    /// File manager for I/O operations
    file_manager: Arc<Mutex<dyn FileManager + Send>>,
    /// Request count for round-robin load balancing
    request_counter: AtomicUsize,
}

/// Global statistics for the concurrent buffer pool
#[derive(Debug, Default)]
pub struct ConcurrentBufferPoolStats {
    /// Total number of cache requests
    pub total_requests: AtomicU64,
    /// Total number of cache hits
    pub total_hits: AtomicU64,
    /// Total number of cache misses
    pub total_misses: AtomicU64,
    /// Total number of evictions
    pub total_evictions: AtomicU64,
    /// Total number of contentions
    pub total_contentions: AtomicU64,
    /// Average response time (microseconds)
    pub avg_response_time: AtomicU64,
    /// Number of active parallel workers
    pub active_workers: AtomicUsize,
    /// Peak number of workers
    pub peak_workers: AtomicUsize,
}

impl ConcurrentBufferPoolStats {
    pub fn global_hit_rate(&self) -> f64 {
        let hits = self.total_hits.load(Ordering::Relaxed);
        let misses = self.total_misses.load(Ordering::Relaxed);
        if hits + misses == 0 {
            0.0
        } else {
            hits as f64 / (hits + misses) as f64
        }
    }

    pub fn global_contention_rate(&self) -> f64 {
        let contentions = self.total_contentions.load(Ordering::Relaxed);
        let total_operations = self.total_hits.load(Ordering::Relaxed) + self.total_misses.load(Ordering::Relaxed);
        if total_operations == 0 {
            0.0
        } else {
            contentions as f64 / total_operations as f64
        }
    }
}

impl ConcurrentBufferPool {
    /// Create a new concurrent buffer pool
    pub fn new(
        config: ConcurrentBufferPoolConfig,
        file_manager: Arc<Mutex<dyn FileManager + Send>>,
    ) -> Self {
        let capacity_per_shard = config.total_capacity / config.shard_count;
        let mut shards = Vec::with_capacity(config.shard_count);

        for i in 0..config.shard_count {
            shards.push(ConcurrentBufferShard::new(i, capacity_per_shard));
        }

        Self {
            shards,
            config,
            global_stats: Arc::new(RwLock::new(ConcurrentBufferPoolStats::default())),
            file_manager,
            request_counter: AtomicUsize::new(0),
        }
    }

    /// Get the shard ID for a given page ID
    fn get_shard_id(&self, page_id: PageId) -> usize {
        (page_id as usize) % self.config.shard_count
    }

    /// Apply backoff strategy based on retry count
    fn apply_backoff(&self, retry_count: usize) {
        match self.config.backoff_strategy {
            BackoffStrategy::None => {
                // Spin wait
                std::hint::spin_loop();
            }
            BackoffStrategy::Fixed(duration) => {
                std::thread::sleep(duration);
            }
            BackoffStrategy::Exponential => {
                let delay = Duration::from_micros(10 * (1 << retry_count.min(8)) as u64);
                std::thread::sleep(delay);
            }
            BackoffStrategy::Adaptive => {
                // Adaptive based on recent contention
                let stats = self.global_stats.read().unwrap();
                let contention_rate = stats.global_contention_rate();
                drop(stats);

                if contention_rate > 0.5 {
                    std::thread::sleep(Duration::from_micros(100));
                } else if contention_rate > 0.2 {
                    std::thread::sleep(Duration::from_micros(10));
                } else {
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Fetch a page with retry logic for handling contention
    pub fn fetch_page(&self, page_id: PageId) -> Result<Arc<Mutex<Page>>> {
        let start_time = Instant::now();
        let shard_id = self.get_shard_id(page_id);
        let shard = &self.shards[shard_id];

        // Update request counter
        self.request_counter.fetch_add(1, Ordering::Relaxed);

        // Try to acquire page with retry logic
        for retry in 0..=self.config.max_retries {
            // First, try to find page in cache
            {
                let page_to_frame = shard.page_to_frame.read().unwrap();
                if let Some(&frame_idx) = page_to_frame.get(&page_id) {
                    let frame = &shard.frames[frame_idx];

                    // Try to acquire frame lock with non-blocking attempt
                    if let Ok(mut f) = frame.try_lock() {
                        f.pin();
                        shard.update_lru(page_id);
                        shard.stats.hits.fetch_add(1, Ordering::Relaxed);

                        if let Some(ref page) = f.page {
                            // Update global statistics
                            {
                                let mut stats = self.global_stats.write().unwrap();
                                stats.total_requests.fetch_add(1, Ordering::Relaxed);
                                stats.total_hits.fetch_add(1, Ordering::Relaxed);

                                let response_time = start_time.elapsed().as_micros() as u64;
                                let current_avg = stats.avg_response_time.load(Ordering::Relaxed);
                                let new_avg = (current_avg + response_time) / 2;
                                stats.avg_response_time.store(new_avg, Ordering::Relaxed);
                            }

                            return Ok(Arc::new(Mutex::new(page.clone())));
                        }
                    } else {
                        // Frame is locked, record contention
                        shard.stats.contentions.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            // Page not in cache or frame locked, load from disk
            if retry == 0 {
                shard.stats.misses.fetch_add(1, Ordering::Relaxed);
            }

            match self.load_page_into_shard(page_id, shard_id) {
                Ok(page) => {
                    // Update global statistics
                    {
                        let mut stats = self.global_stats.write().unwrap();
                        stats.total_requests.fetch_add(1, Ordering::Relaxed);
                        stats.total_misses.fetch_add(1, Ordering::Relaxed);

                        let response_time = start_time.elapsed().as_micros() as u64;
                        let current_avg = stats.avg_response_time.load(Ordering::Relaxed);
                        let new_avg = (current_avg + response_time) / 2;
                        stats.avg_response_time.store(new_avg, Ordering::Relaxed);
                    }

                    return Ok(page);
                }
                Err(e) => {
                    if retry < self.config.max_retries {
                        shard.stats.contentions.fetch_add(1, Ordering::Relaxed);
                        self.apply_backoff(retry);
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(crate::error::RustgreSQLError::Storage(
            format!("Failed to fetch page {} after {} retries", page_id, self.config.max_retries)
        ))
    }

    /// Load a page into a specific shard
    fn load_page_into_shard(&self, page_id: PageId, shard_id: usize) -> Result<Arc<Mutex<Page>>> {
        let shard = &self.shards[shard_id];

        // Find available frame in shard
        let frame_idx = shard.find_available_frame()?;
        let frame = &shard.frames[frame_idx];

        let mut f = frame.lock().unwrap();

        // If frame contains a dirty page, write it back
        if f.dirty {
            if let (Some(old_page_id), Some(ref old_page)) = (f.page_id, f.page.clone()) {
                let fm = self.file_manager.lock().unwrap();
                fm.write_page(old_page_id, old_page.clone())?;
            }
        }

        // Update mappings within shard
        {
            let mut page_to_frame = shard.page_to_frame.write().unwrap();

            // Remove old mapping if exists
            if let Some(old_page_id) = f.page_id {
                page_to_frame.remove(&old_page_id);
            }

            // Add new mapping
            page_to_frame.insert(page_id, frame_idx);
        }

        // Load page from disk
        let fm = self.file_manager.lock().unwrap();
        let page = fm.read_page(page_id)?;

        f.page = Some(page.clone());
        f.page_id = Some(page_id);
        f.pin();
        f.dirty = false;

        shard.update_lru(page_id);
        shard.stats.current_size.store(shard.size(), Ordering::Relaxed);

        Ok(Arc::new(Mutex::new(page)))
    }

    /// Unpin a page
    pub fn unpin_page(&self, page_id: PageId, dirty: bool) -> Result<()> {
        let shard_id = self.get_shard_id(page_id);
        let shard = &self.shards[shard_id];

        let page_to_frame = shard.page_to_frame.read().unwrap();
        if let Some(&frame_idx) = page_to_frame.get(&page_id) {
            let frame = &shard.frames[frame_idx];
            let mut f = frame.lock().unwrap();
            f.unpin()?;
            f.dirty = f.dirty || dirty;
            Ok(())
        } else {
            Err(crate::error::RustgreSQLError::PageNotFound(page_id))
        }
    }

    /// Flush all dirty pages to disk
    pub fn flush_all_pages(&self) -> Result<()> {
        let fm = self.file_manager.lock().unwrap();

        for shard in &self.shards {
            let page_to_frame = shard.page_to_frame.read().unwrap();

            for (&frame_idx, &page_id) in page_to_frame.iter() {
                let frame = &shard.frames[frame_idx];
                let f = frame.lock().unwrap();

                if f.dirty && f.page.is_some() {
                    fm.write_page(page_id, f.page.as_ref().unwrap().clone())?;
                }
            }
        }

        Ok(())
    }

    /// Get comprehensive statistics for all shards
    pub fn get_stats(&self) -> ConcurrentBufferPoolStatsSnapshot {
        let global_stats = self.global_stats.read().unwrap();
        let shard_stats: Vec<(usize, ShardStats)> = self.shards.iter()
            .enumerate()
            .map(|(i, shard)| {
                (
                    i,
                    ShardStats {
                        hits: AtomicU64::new(shard.stats.hits.load(Ordering::Relaxed)),
                        misses: AtomicU64::new(shard.stats.misses.load(Ordering::Relaxed)),
                        evictions: AtomicU64::new(shard.stats.evictions.load(Ordering::Relaxed)),
                        contentions: AtomicU64::new(shard.stats.contentions.load(Ordering::Relaxed)),
                        avg_lock_time: AtomicU64::new(shard.stats.avg_lock_time.load(Ordering::Relaxed)),
                        current_size: AtomicUsize::new(shard.stats.current_size.load(Ordering::Relaxed)),
                    }
                )
            })
            .collect();

        ConcurrentBufferPoolStatsSnapshot {
            global_stats: ConcurrentBufferPoolStats {
                total_requests: AtomicU64::new(global_stats.total_requests.load(Ordering::Relaxed)),
                total_hits: AtomicU64::new(global_stats.total_hits.load(Ordering::Relaxed)),
                total_misses: AtomicU64::new(global_stats.total_misses.load(Ordering::Relaxed)),
                total_evictions: AtomicU64::new(global_stats.total_evictions.load(Ordering::Relaxed)),
                total_contentions: AtomicU64::new(global_stats.total_contentions.load(Ordering::Relaxed)),
                avg_response_time: AtomicU64::new(global_stats.avg_response_time.load(Ordering::Relaxed)),
                active_workers: AtomicUsize::new(global_stats.active_workers.load(Ordering::Relaxed)),
                peak_workers: AtomicUsize::new(global_stats.peak_workers.load(Ordering::Relaxed)),
            },
            shard_stats,
        }
    }

    /// Register a parallel worker (for statistics)
    pub fn register_worker(&self) -> WorkerHandle {
        let current_workers = self.global_stats.read().unwrap().active_workers.load(Ordering::Relaxed);
        let new_count = current_workers + 1;

        {
            let mut stats = self.global_stats.write().unwrap();
            stats.active_workers.store(new_count, Ordering::Relaxed);

            let peak = stats.peak_workers.load(Ordering::Relaxed);
            if new_count > peak {
                stats.peak_workers.store(new_count, Ordering::Relaxed);
            }
        }

        WorkerHandle {
            pool_id: std::ptr::addr_of!(*self) as usize,
            worker_id: current_workers,
            stats: self.global_stats.clone(),
        }
    }

    /// Get load balancing information
    pub fn get_load_balance_info(&self) -> LoadBalanceInfo {
        let shard_sizes: Vec<usize> = self.shards.iter()
            .map(|shard| shard.size())
            .collect();

        let total_pages: usize = shard_sizes.iter().sum();
        let avg_pages_per_shard = total_pages as f64 / self.shards.len() as f64;

        let variance = shard_sizes.iter()
            .map(|&size| {
                let diff = size as f64 - avg_pages_per_shard;
                diff * diff
            })
            .sum::<f64>() / self.shards.len() as f64;

        LoadBalanceInfo {
            shard_sizes,
            avg_pages_per_shard,
            variance,
            imbalance_ratio: if avg_pages_per_shard > 0.0 {
                variance.sqrt() / avg_pages_per_shard
            } else {
                0.0
            },
        }
    }
}

/// Handle for a registered worker
#[derive(Debug)]
pub struct WorkerHandle {
    pool_id: usize,
    worker_id: usize,
    stats: Arc<RwLock<ConcurrentBufferPoolStats>>,
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        let current = self.stats.read().unwrap().active_workers.load(Ordering::Relaxed);
        if current > 0 {
            self.stats.write().unwrap().active_workers.store(current - 1, Ordering::Relaxed);
        }
    }
}

/// Snapshot of buffer pool statistics
#[derive(Debug)]
pub struct ConcurrentBufferPoolStatsSnapshot {
    pub global_stats: ConcurrentBufferPoolStats,
    pub shard_stats: Vec<(usize, ShardStats)>,
}

/// Load balancing information
#[derive(Debug)]
pub struct LoadBalanceInfo {
    pub shard_sizes: Vec<usize>,
    pub avg_pages_per_shard: f64,
    pub variance: f64,
    pub imbalance_ratio: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_utils::MockFileManager;

    #[test]
    fn test_concurrent_buffer_pool_creation() {
        let config = ConcurrentBufferPoolConfig::default();
        let file_manager = Arc::new(Mutex::new(MockFileManager::new()));
        let pool = ConcurrentBufferPool::new(config, file_manager);

        assert_eq!(pool.shards.len(), num_cpus::get().next_power_of_two());
    }

    #[test]
    fn test_shard_mapping() {
        let config = ConcurrentBufferPoolConfig {
            shard_count: 4,
            ..Default::default()
        };
        let file_manager = Arc::new(Mutex::new(MockFileManager::new()));
        let pool = ConcurrentBufferPool::new(config, file_manager);

        // Test that page IDs are mapped to shards consistently
        assert_eq!(pool.get_shard_id(0), 0);
        assert_eq!(pool.get_shard_id(4), 0);
        assert_eq!(pool.get_shard_id(1), 1);
        assert_eq!(pool.get_shard_id(5), 1);
        assert_eq!(pool.get_shard_id(2), 2);
        assert_eq!(pool.get_shard_id(6), 2);
        assert_eq!(pool.get_shard_id(3), 3);
        assert_eq!(pool.get_shard_id(7), 3);
    }

    #[test]
    fn test_worker_registration() {
        let config = ConcurrentBufferPoolConfig::default();
        let file_manager = Arc::new(Mutex::new(MockFileManager::new()));
        let pool = ConcurrentBufferPool::new(config, file_manager);

        {
            let _worker = pool.register_worker();
            let stats = pool.global_stats.read().unwrap();
            assert_eq!(stats.active_workers.load(Ordering::Relaxed), 1);
            assert_eq!(stats.peak_workers.load(Ordering::Relaxed), 1);
        }

        // Worker handle should be dropped
        let stats = pool.global_stats.read().unwrap();
        assert_eq!(stats.active_workers.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_load_balance_info() {
        let config = ConcurrentBufferPoolConfig {
            shard_count: 4,
            total_capacity: 1000,
            ..Default::default()
        };
        let file_manager = Arc::new(Mutex::new(MockFileManager::new()));
        let pool = ConcurrentBufferPool::new(config, file_manager);

        let info = pool.get_load_balance_info();
        assert_eq!(info.shard_sizes.len(), 4);
        assert_eq!(info.avg_pages_per_shard, 0.0); // All shards initially empty
        assert_eq!(info.variance, 0.0);
        assert_eq!(info.imbalance_ratio, 0.0);
    }
}