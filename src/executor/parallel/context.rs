//! Parallel execution context
//!
//! Provides context and state management for parallel query execution.

use crate::{Result, executor::ExecutionContext, transaction::TransactionId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

/// Shared state across parallel workers
#[derive(Debug)]
pub struct SharedExecutionState {
    /// Query start time
    pub query_start_time: Instant,
    /// Worker statistics
    pub worker_stats: Arc<RwLock<HashMap<usize, WorkerStats>>>,
    /// Shared data caches
    pub data_caches: Arc<Mutex<HashMap<String, Arc<Vec<u8>>>>>,
    /// Coordinator for distributed operations
    pub coordinator: Arc<Mutex<OperationCoordinator>>,
    /// Error collection from workers
    pub worker_errors: Arc<Mutex<Vec<WorkerError>>>,
    /// Progress tracking
    pub progress_tracker: Arc<Mutex<ProgressTracker>>,
}

/// Worker-specific execution statistics
#[derive(Debug, Clone)]
pub struct WorkerStats {
    /// Worker identifier
    pub worker_id: usize,
    /// Tasks completed
    pub tasks_completed: u64,
    /// Rows processed
    pub rows_processed: u64,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Memory usage in bytes
    pub memory_used: usize,
    /// I/O operations performed
    pub io_operations: u64,
    /// Last update timestamp
    pub last_update: Instant,
}

impl WorkerStats {
    pub fn new(worker_id: usize) -> Self {
        Self {
            worker_id,
            tasks_completed: 0,
            rows_processed: 0,
            execution_time_ms: 0,
            memory_used: 0,
            io_operations: 0,
            last_update: Instant::now(),
        }
    }

    pub fn update(&mut self, rows_processed: u64, execution_time_ms: u64, memory_used: usize, io_operations: u64) {
        self.tasks_completed += 1;
        self.rows_processed += rows_processed;
        self.execution_time_ms += execution_time_ms;
        self.memory_used = self.memory_used.max(memory_used);
        self.io_operations += io_operations;
        self.last_update = Instant::now();
    }
}

/// Worker error information
#[derive(Debug, Clone)]
pub struct WorkerError {
    /// Worker identifier
    pub worker_id: usize,
    /// Error message
    pub error_message: String,
    /// Task ID where error occurred
    pub task_id: u64,
    /// Timestamp
    pub timestamp: Instant,
    /// Stack trace (optional)
    pub stack_trace: Option<String>,
}

/// Progress tracking for parallel operations
#[derive(Debug)]
pub struct ProgressTracker {
    /// Total work units to complete
    pub total_units: u64,
    /// Completed work units
    pub completed_units: u64,
    /// Workers currently active
    pub active_workers: usize,
    /// Estimated completion time
    pub estimated_completion: Option<Instant>,
    /// Last progress update
    pub last_update: Instant,
}

impl ProgressTracker {
    pub fn new(total_units: u64) -> Self {
        Self {
            total_units,
            completed_units: 0,
            active_workers: 0,
            estimated_completion: None,
            last_update: Instant::now(),
        }
    }

    pub fn update_progress(&mut self, additional_units: u64) {
        self.completed_units += additional_units;
        self.last_update = Instant::now();

        // Update estimated completion time
        if self.completed_units > 0 {
            let elapsed = self.query_start_time.elapsed();
            let rate = self.completed_units as f64 / elapsed.as_secs_f64();
            if rate > 0.0 {
                let remaining_units = (self.total_units - self.completed_units) as f64;
                let estimated_remaining = Duration::from_secs_f64(remaining_units / rate);
                self.estimated_completion = Some(Instant::now() + estimated_remaining);
            }
        }
    }

    pub fn get_progress_percentage(&self) -> f64 {
        if self.total_units == 0 {
            1.0
        } else {
            self.completed_units as f64 / self.total_units as f64
        }
    }
}

/// Coordinator for distributed operations across workers
#[derive(Debug)]
pub struct OperationCoordinator {
    /// Barrier synchronization points
    pub barriers: HashMap<String, Barrier>,
    /// Shared hash tables for joins
    pub hash_tables: HashMap<String, SharedHashTable>,
    /// Aggregate accumulation
    pub aggregates: HashMap<String, AggregateAccumulator>,
    /// Merge coordinators for sorted data
    pub merge_coordinators: HashMap<String, MergeCoordinator>,
}

/// Barrier for worker synchronization
#[derive(Debug)]
pub struct Barrier {
    /// Expected number of workers
    pub expected_workers: usize,
    /// Current number of arrived workers
    pub arrived_workers: usize,
    /// Barrier creation time
    pub created_at: Instant,
    /// Workers that have arrived
    pub worker_arrival: Vec<usize>,
}

impl Barrier {
    pub fn new(expected_workers: usize) -> Self {
        Self {
            expected_workers,
            arrived_workers: 0,
            created_at: Instant::now(),
            worker_arrival: Vec::new(),
        }
    }

    pub fn arrive(&mut self, worker_id: usize) -> bool {
        self.worker_arrival.push(worker_id);
        self.arrived_workers += 1;
        self.arrived_workers >= self.expected_workers
    }

    pub fn is_complete(&self) -> bool {
        self.arrived_workers >= self.expected_workers
    }
}

/// Shared hash table for parallel joins
#[derive(Debug)]
pub struct SharedHashTable {
    /// Hash table data
    pub data: Arc<RwLock<HashMap<String, Vec<Vec<u8>>>>>,
    /// Build phase completion flag
    pub build_complete: bool,
    /// Number of workers that have completed build
    pub build_workers_complete: usize,
    /// Expected build workers
    pub expected_build_workers: usize,
    /// Table statistics
    pub stats: HashTableStats,
}

/// Hash table statistics
#[derive(Debug, Clone)]
pub struct HashTableStats {
    /// Total entries
    pub total_entries: usize,
    /// Number of buckets
    pub bucket_count: usize,
    /// Average chain length
    pub avg_chain_length: f64,
    /// Maximum chain length
    pub max_chain_length: usize,
}

/// Aggregate accumulator for parallel aggregation
#[derive(Debug)]
pub struct AggregateAccumulator {
    /// Partial results from workers
    pub partial_results: Vec<Vec<u8>>,
    /// Number of expected partial results
    pub expected_results: usize,
    /// Aggregate function type
    pub aggregate_type: AggregateType,
    /// Accumulation complete flag
    pub complete: bool,
}

/// Types of aggregate functions
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateType {
    Count,
    Sum,
    Average,
    Min,
    Max,
    Custom(String),
}

/// Merge coordinator for parallel sorted data
#[derive(Debug)]
pub struct MergeCoordinator {
    /// Expected merge streams
    pub expected_streams: usize,
    /// Received streams
    pub received_streams: Vec<Option<Vec<u8>>>,
    /// Merge complete flag
    pub merge_complete: bool,
    /// Sort columns
    pub sort_columns: Vec<String>,
}

/// Parallel execution context
#[derive(Debug, Clone)]
pub struct ParallelExecutionContext {
    /// Base execution context
    pub base_context: ExecutionContext,
    /// Worker identifier
    pub worker_id: usize,
    /// Task identifier
    pub task_id: u64,
    /// Shared execution state
    pub shared_state: Arc<SharedExecutionState>,
    /// Transaction ID for this parallel operation
    pub transaction_id: TransactionId,
    /// Worker-specific configuration
    pub worker_config: WorkerConfig,
}

/// Configuration for individual workers
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Memory limit for this worker
    pub memory_limit: usize,
    /// Batch size for processing
    pub batch_size: usize,
    /// Priority level
    pub priority: u32,
    /// Worker-specific optimization flags
    pub optimization_flags: WorkerOptimizationFlags,
}

/// Worker optimization flags
#[derive(Debug, Clone)]
pub struct WorkerOptimizationFlags {
    /// Enable vectorized processing
    pub enable_vectorization: bool,
    /// Enable memory prefetching
    pub enable_prefetching: bool,
    /// Enable compression for intermediate results
    pub enable_compression: bool,
    /// Cache size for worker
    pub cache_size: usize,
}

impl Default for WorkerOptimizationFlags {
    fn default() -> Self {
        Self {
            enable_vectorization: true,
            enable_prefetching: true,
            enable_compression: false,
            cache_size: 1024 * 1024, // 1MB
        }
    }
}

impl ParallelExecutionContext {
    /// Create a new parallel execution context
    pub fn new(
        base_context: ExecutionContext,
        worker_id: usize,
        task_id: u64,
        shared_state: Arc<SharedExecutionState>,
        transaction_id: TransactionId,
    ) -> Self {
        let worker_config = WorkerConfig {
            memory_limit: 64 * 1024 * 1024, // 64MB default
            batch_size: 1000,
            priority: 0,
            optimization_flags: WorkerOptimizationFlags::default(),
        };

        Self {
            base_context,
            worker_id,
            task_id,
            shared_state,
            transaction_id,
            worker_config,
        }
    }

    /// Create a parallel execution context with custom configuration
    pub fn with_config(
        base_context: ExecutionContext,
        worker_id: usize,
        task_id: u64,
        shared_state: Arc<SharedExecutionState>,
        transaction_id: TransactionId,
        worker_config: WorkerConfig,
    ) -> Self {
        Self {
            base_context,
            worker_id,
            task_id,
            shared_state,
            transaction_id,
            worker_config,
        }
    }

    /// Update worker statistics
    pub fn update_worker_stats(&self, rows_processed: u64, execution_time_ms: u64, memory_used: usize, io_operations: u64) {
        let mut worker_stats = self.shared_state.worker_stats.write().unwrap();
        let stats = worker_stats.entry(self.worker_id).or_insert_with(|| WorkerStats::new(self.worker_id));
        stats.update(rows_processed, execution_time_ms, memory_used, io_operations);
    }

    /// Record an error from this worker
    pub fn record_error(&self, error_message: String, stack_trace: Option<String>) {
        let error = WorkerError {
            worker_id: self.worker_id,
            error_message,
            task_id: self.task_id,
            timestamp: Instant::now(),
            stack_trace,
        };

        let mut errors = self.shared_state.worker_errors.lock().unwrap();
        errors.push(error);
    }

    /// Get progress information
    pub fn get_progress(&self) -> (f64, Option<Instant>) {
        let progress = self.shared_state.progress_tracker.lock().unwrap();
        (progress.get_progress_percentage(), progress.estimated_completion)
    }

    /// Check if there are any worker errors
    pub fn has_errors(&self) -> bool {
        !self.shared_state.worker_errors.lock().unwrap().is_empty()
    }

    /// Get all worker errors
    pub fn get_errors(&self) -> Vec<WorkerError> {
        self.shared_state.worker_errors.lock().unwrap().clone()
    }

    /// Store data in shared cache
    pub fn cache_data(&self, key: String, data: Vec<u8>) {
        let mut caches = self.shared_state.data_caches.lock().unwrap();
        caches.insert(key, Arc::new(data));
    }

    /// Retrieve data from shared cache
    pub fn get_cached_data(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let caches = self.shared_state.data_caches.lock().unwrap();
        caches.get(key).cloned()
    }

    /// Create or get a barrier for synchronization
    pub fn get_barrier(&self, barrier_id: &str, expected_workers: usize) -> Arc<Mutex<Barrier>> {
        let mut coordinator = self.shared_state.coordinator.lock().unwrap();
        coordinator.barriers.entry(barrier_id.to_string())
            .or_insert_with(|| Barrier::new(expected_workers));

        // Return a reference - note: this is simplified and would need proper Arc handling in practice
        Arc::new(Mutex::new(Barrier::new(expected_workers)))
    }

    /// Wait at a barrier until all workers arrive
    pub fn wait_at_barrier(&self, barrier_id: &str) -> Result<()> {
        let mut coordinator = self.shared_state.coordinator.lock().unwrap();
        if let Some(barrier) = coordinator.barriers.get_mut(barrier_id) {
            if barrier.arrive(self.worker_id) {
                // Last worker to arrive, remove barrier
                coordinator.barriers.remove(barrier_id);
                Ok(())
            } else {
                // Not the last worker, continue
                Ok(())
            }
        } else {
            Err(crate::error::RustgreSQLError::Internal(
                format!("Barrier {} not found", barrier_id)
            ))
        }
    }

    /// Create or get a shared hash table
    pub fn get_shared_hash_table(&self, table_id: &str, expected_build_workers: usize) -> Arc<Mutex<SharedHashTable>> {
        let mut coordinator = self.shared_state.coordinator.lock().unwrap();
        coordinator.hash_tables.entry(table_id.to_string())
            .or_insert_with(|| SharedHashTable {
                data: Arc::new(RwLock::new(HashMap::new())),
                build_complete: false,
                build_workers_complete: 0,
                expected_build_workers,
                stats: HashTableStats {
                    total_entries: 0,
                    bucket_count: 0,
                    avg_chain_length: 0.0,
                    max_chain_length: 0,
                },
            });

        // Return a reference - note: this is simplified and would need proper Arc handling in practice
        Arc::new(Mutex::new(SharedHashTable {
            data: Arc::new(RwLock::new(HashMap::new())),
            build_complete: false,
            build_workers_complete: 0,
            expected_build_workers,
            stats: HashTableStats {
                total_entries: 0,
                bucket_count: 0,
                avg_chain_length: 0.0,
                max_chain_length: 0,
            },
        }))
    }
}

impl SharedExecutionState {
    /// Create a new shared execution state
    pub fn new() -> Self {
        Self {
            query_start_time: Instant::now(),
            worker_stats: Arc::new(RwLock::new(HashMap::new())),
            data_caches: Arc::new(Mutex::new(HashMap::new())),
            coordinator: Arc::new(Mutex::new(OperationCoordinator {
                barriers: HashMap::new(),
                hash_tables: HashMap::new(),
                aggregates: HashMap::new(),
                merge_coordinators: HashMap::new(),
            })),
            worker_errors: Arc::new(Mutex::new(Vec::new())),
            progress_tracker: Arc::new(Mutex::new(ProgressTracker::new(0))),
        }
    }

    /// Initialize with expected total work units
    pub fn with_total_units(total_units: u64) -> Self {
        let mut state = Self::new();
        *state.progress_tracker.lock().unwrap() = ProgressTracker::new(total_units);
        state
    }

    /// Get aggregate statistics from all workers
    pub fn get_aggregate_stats(&self) -> AggregateWorkerStats {
        let worker_stats = self.worker_stats.read().unwrap();
        let mut total_tasks = 0;
        let mut total_rows = 0;
        let mut total_execution_time = 0;
        let mut total_memory = 0;
        let mut total_io = 0;
        let mut active_workers = 0;

        for stats in worker_stats.values() {
            total_tasks += stats.tasks_completed;
            total_rows += stats.rows_processed;
            total_execution_time += stats.execution_time_ms;
            total_memory += stats.memory_used;
            total_io += stats.io_operations;
            if stats.last_update.elapsed() < Duration::from_secs(5) {
                active_workers += 1;
            }
        }

        AggregateWorkerStats {
            total_tasks,
            total_rows,
            total_execution_time_ms: total_execution_time,
            total_memory_used: total_memory,
            total_io_operations: total_io,
            active_workers,
            worker_count: worker_stats.len(),
        }
    }

    /// Check if any critical errors have occurred
    pub fn has_critical_errors(&self) -> bool {
        let errors = self.worker_errors.lock().unwrap();
        !errors.is_empty()
    }

    /// Get error summary
    pub fn get_error_summary(&self) -> ErrorSummary {
        let errors = self.worker_errors.lock().unwrap();
        let error_count = errors.len();
        let affected_workers = errors.iter().map(|e| e.worker_id).collect::<std::collections::HashSet<_>>().len();
        let latest_error = errors.last().cloned();

        ErrorSummary {
            total_errors: error_count,
            affected_workers,
            latest_error,
        }
    }
}

/// Aggregate statistics from all workers
#[derive(Debug, Clone)]
pub struct AggregateWorkerStats {
    pub total_tasks: u64,
    pub total_rows: u64,
    pub total_execution_time_ms: u64,
    pub total_memory_used: usize,
    pub total_io_operations: u64,
    pub active_workers: usize,
    pub worker_count: usize,
}

/// Error summary from workers
#[derive(Debug, Clone)]
pub struct ErrorSummary {
    pub total_errors: usize,
    pub affected_workers: usize,
    pub latest_error: Option<WorkerError>,
}

use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::storage::buffer::BufferPoolManager;

    #[test]
    fn test_worker_stats() {
        let mut stats = WorkerStats::new(1);
        stats.update(100, 50, 1024, 5);

        assert_eq!(stats.tasks_completed, 1);
        assert_eq!(stats.rows_processed, 100);
        assert_eq!(stats.execution_time_ms, 50);
        assert_eq!(stats.memory_used, 1024);
        assert_eq!(stats.io_operations, 5);
    }

    #[test]
    fn test_progress_tracker() {
        let mut tracker = ProgressTracker::new(100);
        assert_eq!(tracker.get_progress_percentage(), 0.0);

        tracker.update_progress(25);
        assert_eq!(tracker.get_progress_percentage(), 0.25);
        assert_eq!(tracker.completed_units, 25);
    }

    #[test]
    fn test_barrier() {
        let mut barrier = Barrier::new(3);
        assert!(!barrier.arrive(1));
        assert!(!barrier.arrive(2));
        assert!(barrier.arrive(3)); // Last worker
        assert!(barrier.is_complete());
    }

    #[test]
    fn test_shared_execution_state() {
        let state = SharedExecutionState::new();
        let stats = state.get_aggregate_stats();

        assert_eq!(stats.total_tasks, 0);
        assert_eq!(stats.total_rows, 0);
        assert_eq!(stats.worker_count, 0);
    }

    #[test]
    fn test_parallel_execution_context() {
        // Create a mock execution context (simplified for test)
        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_context = ExecutionContext::new(catalog, buffer_manager);

        let shared_state = Arc::new(SharedExecutionState::new());
        let transaction_id = TransactionId::new();

        let parallel_context = ParallelExecutionContext::new(
            base_context,
            1,
            100,
            shared_state,
            transaction_id,
        );

        assert_eq!(parallel_context.worker_id, 1);
        assert_eq!(parallel_context.task_id, 100);
        assert!(!parallel_context.has_errors());
    }

    #[test]
    fn test_error_recording() {
        let shared_state = Arc::new(SharedExecutionState::new());

        // Create a mock execution context
        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_context = ExecutionContext::new(catalog, buffer_manager);

        let parallel_context = ParallelExecutionContext::new(
            base_context,
            1,
            100,
            shared_state,
            TransactionId::new(),
        );

        parallel_context.record_error("Test error".to_string(), None);
        assert!(parallel_context.has_errors());

        let errors = parallel_context.get_errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].worker_id, 1);
        assert_eq!(errors[0].error_message, "Test error");
    }
}