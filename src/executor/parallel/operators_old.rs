//! Parallel execution operators
//!
//! This module provides parallel-aware implementations of the basic operators
//! for distributed query processing across multiple workers.

use crate::{Result, RustgreSQLError};
use crate::executor::{QueryResult, RowData, TableScanner, EvaluationContext, ExpressionEvaluator, ThreeValuedLogic};
use crate::sql::ast::{Expression, BinaryOperator, SetOperator as SetOperatorType, JoinType};
use crate::types::{Value, ValueKind};
use crate::catalog::TableDef;
use crate::executor::parallel::{
    ParallelScanner, DefaultParallelScanner, ParallelScannerConfig, TaskScheduler, TaskId, TaskType,
    ParallelExecutionContext, ResourceManager, ResourceType, ConcurrentBufferPool,
    ParallelScanIterator, PartitionStrategy, LoadBalanceStrategy
};

use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use serde::{Serialize, Deserialize};

// Type alias for compatibility
type Row = RowData;
type Schema = crate::catalog::SchemaDef;

/// Configuration for parallel operators
#[derive(Debug, Clone)]
pub struct ParallelOperatorConfig {
    /// Number of parallel workers
    pub worker_count: usize,
    /// Maximum batch size for processing
    pub max_batch_size: usize,
    /// Whether to enable adaptive batching
    pub adaptive_batching: bool,
    /// Memory limit per worker (in bytes)
    pub memory_limit_per_worker: usize,
    /// Timeout for individual tasks (in seconds)
    pub task_timeout_secs: u64,
}

impl Default for ParallelOperatorConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get(),
            max_batch_size: 1000,
            adaptive_batching: true,
            memory_limit_per_worker: 100 * 1024 * 1024, // 100MB
            task_timeout_secs: 300, // 5 minutes
        }
    }
}

/// Statistics for parallel operator execution
#[derive(Debug, Default)]
pub struct ParallelOperatorStats {
    /// Total number of tasks executed
    pub total_tasks: AtomicU64,
    /// Number of successful tasks
    pub successful_tasks: AtomicU64,
    /// Number of failed tasks
    pub failed_tasks: AtomicU64,
    /// Total rows processed
    pub total_rows_processed: AtomicU64,
    /// Total execution time (microseconds)
    pub total_execution_time_us: AtomicU64,
    /// Average throughput (rows per second)
    pub avg_throughput: f64,
    /// Memory usage peak (bytes)
    pub peak_memory_usage: AtomicU64,
    /// Parallel efficiency (0.0 to 1.0)
    pub parallel_efficiency: f64,
}

/// Base trait for parallel operators
pub trait ParallelOperator: Send + Sync {
    /// Execute the operator in parallel
    fn execute_parallel(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult>;

    /// Get execution statistics
    fn get_stats(&self) -> &ParallelOperatorStats;

    /// Estimate resource requirements
    fn estimate_resources(&self) -> ResourceRequirement;
}

/// Resource requirements for an operator
#[derive(Debug, Clone)]
pub struct ResourceRequirement {
    /// CPU cores needed
    pub cpu_cores: f64,
    /// Memory needed (in bytes)
    pub memory_bytes: u64,
    /// I/O bandwidth needed (in bytes per second)
    pub io_bandwidth: u64,
    /// Estimated execution time (in microseconds)
    pub estimated_time_us: u64,
}

/// Parallel scan operator that extends the basic scan with parallel processing
#[derive(Debug)]
pub struct ParallelScanOperator {
    /// Table name to scan
    pub table_name: String,
    /// Parallel scanner implementation
    pub scanner: Arc<dyn ParallelScanner>,
    /// Configuration for parallel processing
    pub config: ParallelOperatorConfig,
    /// Optional expression filter to apply during scan
    pub filter_predicate: Option<Expression>,
    /// Optional column projection
    pub projection: Option<Vec<String>>,
    /// Statistics
    pub stats: ParallelOperatorStats,
    /// Buffer pool for memory management
    pub buffer_pool: Option<Arc<ConcurrentBufferPool>>,
    /// Resource manager
    pub resource_manager: Option<Arc<ResourceManager>>,
}

impl ParallelScanOperator {
    /// Create a new parallel scan operator
    pub fn new(
        table_name: String,
        scanner: Arc<dyn ParallelScanner>,
        config: ParallelOperatorConfig,
    ) -> Self {
        Self {
            table_name,
            scanner,
            config,
            filter_predicate: None,
            projection: None,
            stats: ParallelOperatorStats::default(),
            buffer_pool: None,
            resource_manager: None,
        }
    }

    /// Create with a regular table scanner
    pub fn from_table_scanner(
        table_name: String,
        table_scanner: TableScanner,
        parallel_config: ParallelScannerConfig,
        operator_config: ParallelOperatorConfig,
    ) -> Self {
        let scanner = Arc::new(DefaultParallelScanner::new(
            // Use catalog manager from table scanner if available
            // For now, we'll create a simple one
            Arc::new(crate::catalog::CatalogManager::new()),
            table_scanner,
            parallel_config,
        ));

        Self::new(table_name, scanner, operator_config)
    }

    /// Create with buffer pool
    pub fn with_buffer_pool(
        table_name: String,
        scanner: Arc<dyn ParallelScanner>,
        config: ParallelOperatorConfig,
        buffer_pool: Arc<ConcurrentBufferPool>,
    ) -> Self {
        Self {
            table_name,
            scanner,
            config,
            filter_predicate: None,
            projection: None,
            stats: ParallelOperatorStats::default(),
            buffer_pool: Some(buffer_pool),
            resource_manager: None,
        }
    }

    /// Add a filter predicate to apply during scan
    pub fn with_filter(mut self, predicate: Expression) -> Self {
        self.filter_predicate = Some(predicate);
        self
    }

    /// Add column projection
    pub fn with_projection(mut self, columns: Vec<String>) -> Self {
        self.projection = Some(columns);
        self
    }

    /// Set resource manager
    pub fn with_resource_manager(mut self, resource_manager: Arc<ResourceManager>) -> Self {
        self.resource_manager = Some(resource_manager);
        self
    }

    /// Execute scan with adaptive parallelism
    fn execute_adaptive_scan(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        context.base_context.log(&format!("Starting parallel scan of table: {}", self.table_name));

        // Estimate total rows and determine optimal parallelism
        let total_rows = self.scanner.estimate_total_rows()?;
        let optimal_workers = self.calculate_optimal_workers(total_rows);

        context.base_context.log(&format!("Estimated {} rows, using {} workers", total_rows, optimal_workers));

        // Adjust scanner configuration
        let mut scanner_config = ParallelScannerConfig::default();
        scanner_config.worker_count = optimal_workers;
        scanner_config.adaptive_partitioning = true;
        scanner_config.partition_strategy = if total_rows > 100000 {
            PartitionStrategy::Hash
        } else {
            PartitionStrategy::Range
        };

        // Create parallel scan iterator
        let mut scan_iterator = ParallelScanIterator::new(
            // We need to clone the scanner for the iterator
            // In a real implementation, this would be handled differently
            Box::new(self.create_scanner_with_config(scanner_config.clone())),
            scanner_config,
        )?;

        let mut rows = Vec::new();
        let mut column_names = Vec::new();
        let mut rows_processed = 0;

        // Process rows in batches
        loop {
            let batch_start = Instant::now();
            let mut batch_rows = Vec::new();

            // Collect a batch of rows
            for _ in 0..self.config.max_batch_size {
                match scan_iterator.next_row()? {
                    Some(row_data) => {
                        // Apply filter predicate if present
                        if let Some(ref predicate) = self.filter_predicate {
                            if self.evaluate_filter(predicate, &row_data)? {
                                // Apply projection if present
                                let processed_row = self.apply_projection(&row_data)?;
                                batch_rows.push(processed_row);
                            }
                        } else {
                            let processed_row = self.apply_projection(&row_data)?;
                            batch_rows.push(processed_row);
                        }
                    }
                    None => break,
                }
            }

            if batch_rows.is_empty() {
                break;
            }

            // Handle first row to get column names
            if rows.is_empty() && !batch_rows.is_empty() {
                column_names = self.get_column_names(&batch_rows[0])?;
            }

            rows.extend(batch_rows);
            rows_processed += batch_rows.len();

            // Update statistics
            let batch_time = batch_start.elapsed().as_micros() as u64;
            self.stats.total_execution_time_us.fetch_add(batch_time, Ordering::Relaxed);

            // Adaptive batch size adjustment
            if self.config.adaptive_batching {
                self.adjust_batch_size(batch_time, batch_rows.len());
            }

            // Memory pressure check
            if let Some(ref buffer_pool) = self.buffer_pool {
                let memory_usage = self.estimate_memory_usage();
                if memory_usage > self.config.memory_limit_per_worker {
                    context.base_context.log("Memory pressure detected, flushing results");
                    self.flush_if_needed(context)?;
                }
            }
        }

        // Final statistics
        let total_time = start_time.elapsed().as_micros() as u64;
        self.stats.total_rows_processed.store(rows_processed as u64, Ordering::Relaxed);
        self.stats.total_execution_time_us.fetch_add(total_time, Ordering::Relaxed);

        // Calculate throughput
        let total_time_seconds = total_time as f64 / 1_000_000.0;
        if total_time_seconds > 0.0 {
            self.stats.avg_throughput = rows_processed as f64 / total_time_seconds;
        }

        context.base_context.log(&format!("Parallel scan completed: {} rows in {:.2}s, throughput: {:.2} rows/s",
                             rows_processed, total_time_seconds, self.stats.avg_throughput));

        Ok(QueryResult {
            rows,
            column_names,
        })
    }

    /// Calculate optimal number of workers based on data size
    fn calculate_optimal_workers(&self, total_rows: usize) -> usize {
        let min_rows_per_worker = 1000; // Minimum rows to justify a worker

        if total_rows < min_rows_per_worker {
            return 1; // Sequential processing for small datasets
        }

        let max_workers = self.config.worker_count;
        let optimal_workers = (total_rows / min_rows_per_worker).min(max_workers);

        // Ensure at least 2 workers for parallel processing if data is large enough
        if total_rows >= 10000 && optimal_workers == 1 {
            2
        } else {
            optimal_workers
        }
    }

    /// Create a scanner with specific configuration
    fn create_scanner_with_config(&self, config: ParallelScannerConfig) -> Box<dyn ParallelScanner> {
        // For this implementation, we create a new scanner instance
        // In a real implementation, this would clone or reconfigure the existing scanner
        Box::new(DefaultParallelScanner::new(
            Arc::new(crate::catalog::CatalogManager::new()),
            // Create a dummy table scanner - in real implementation this would be provided
            TableScanner::new(
                Arc::new(crate::catalog::CatalogManager::new()),
                Arc::new(crate::storage::BufferPoolManager::new(
                    1000,
                    Arc::new(std::sync::Mutex::new(
                        // Create a simple file manager - in real implementation this would be provided
                        crate::storage::file_manager::DefaultFileManager::create(
                            &format!("{}.db", self.table_name), 8192
                        ).unwrap_or_else(|_| panic!("Failed to create file manager"))
                    ))
                )),
                &self.table_name,
            ).unwrap(),
            config,
        ))
    }

    /// Evaluate filter predicate on a row
    fn evaluate_filter(&self, predicate: &Expression, row: &RowData) -> Result<bool> {
        let eval_context = self.create_evaluation_context(row);
        let evaluator = ExpressionEvaluator;

        match evaluator.evaluate(predicate, &eval_context)? {
            value => {
                match ThreeValuedLogic::from_value(&value) {
                    ThreeValuedLogic::True => Ok(true),
                    ThreeValuedLogic::False | ThreeValuedLogic::Unknown => Ok(false),
                }
            }
        }
    }

    /// Apply column projection to a row
    fn apply_projection(&self, row: &RowData) -> Result<Vec<Value>> {
        if let Some(ref projection) = self.projection {
            let mut projected_row = Vec::new();

            // In a real implementation, this would map column names to indices
            // For now, we'll just return the original row values
            projected_row.extend_from_slice(&row.values[..projection.len().min(row.values.len())]);

            Ok(projected_row)
        } else {
            Ok(row.values.clone())
        }
    }

    /// Get column names for the result
    fn get_column_names(&self, sample_row: &[Value]) -> Result<Vec<String>> {
        if let Some(ref projection) = self.projection {
            Ok(projection.clone())
        } else {
            // Get column names from scanner
            let table_def = self.scanner.get_table_def();
            Ok(table_def.columns.iter().map(|col| col.name.clone()).collect())
        }
    }

    /// Create evaluation context for a row
    fn create_evaluation_context(&self, row: &RowData) -> EvaluationContext {
        let table_def = self.scanner.get_table_def();
        let mut columns = HashMap::new();

        for (i, column_def) in table_def.columns.iter().enumerate() {
            if i < row.values.len() {
                columns.insert(column_def.name.clone(), row.values[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Adjust batch size based on performance
    fn adjust_batch_size(&self, batch_time: u64, batch_size: usize) {
        let target_time_us = 10000; // Target 10ms per batch

        if batch_time > target_time_us * 2 {
            // Batch is taking too long, reduce size
            let new_size = (batch_size / 2).max(100);
            // Note: In a real implementation, this would update the configuration
        } else if batch_time < target_time_us / 2 {
            // Batch is too fast, increase size
            let new_size = (batch_size * 2).min(5000);
            // Note: In a real implementation, this would update the configuration
        }
    }

    /// Estimate current memory usage
    fn estimate_memory_usage(&self) -> usize {
        // Simple estimation based on rows processed
        let rows = self.stats.total_rows_processed.load(Ordering::Relaxed);
        let avg_row_size = 100; // Estimated bytes per row
        (rows * avg_row_size) as usize
    }

    /// Flush intermediate results if needed
    fn flush_if_needed(&self, _context: &mut ParallelExecutionContext) -> Result<()> {
        // In a real implementation, this would write to disk or send to next operator
        Ok(())
    }
}

impl ParallelOperator for ParallelScanOperator {
    fn execute_parallel(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        self.stats.total_tasks.fetch_add(1, Ordering::Relaxed);

        let result = if self.config.adaptive_batching {
            self.execute_adaptive_scan(context)
        } else {
            // Fall back to simple parallel scan
            self.execute_adaptive_scan(context)
        };

        match &result {
            Ok(_) => {
                self.stats.successful_tasks.fetch_add(1, Ordering::Relaxed);

                // Note: parallel_efficiency calculation would require interior mutability
                // This is left as a future improvement
            }
            Err(_) => {
                self.stats.failed_tasks.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    fn get_stats(&self) -> &ParallelOperatorStats {
        &self.stats
    }

    fn estimate_resources(&self) -> ResourceRequirement {
        let total_rows = self.scanner.estimate_total_rows().unwrap_or(10000);

        ResourceRequirement {
            cpu_cores: self.config.worker_count as f64,
            memory_bytes: self.config.memory_limit_per_worker as u64,
            io_bandwidth: (total_rows * 100) as u64, // Estimate 100 bytes per row
            estimated_time_us: if total_rows > 0 {
                (total_rows as f64 / self.stats.avg_throughput.max(1.0) * 1_000_000.0) as u64
            } else {
                0
            },
        }
    }

}

// Additional implementation for ParallelScanOperator
impl ParallelScanOperator {
    // Helper method for parallel efficiency calculation
    fn estimate_sequential_time(&self) -> f64 {
        let total_rows = self.scanner.estimate_total_rows().unwrap_or(10000);
        let sequential_throughput = 1000.0; // Estimate 1000 rows/sec sequentially

        (total_rows as f64 / sequential_throughput) * 1_000_000.0 // Convert to microseconds
    }
}

/// Parallel filter operator
#[derive(Debug)]
pub struct ParallelFilterOperator {
    /// Input operator
    pub input: Box<dyn ParallelOperator>,
    /// Filter condition
    pub condition: Expression,
    /// Configuration
    pub config: ParallelOperatorConfig,
    /// Statistics
    pub stats: ParallelOperatorStats,
}

impl ParallelFilterOperator {
    pub fn new(input: Box<dyn ParallelOperator>, condition: Expression, config: ParallelOperatorConfig) -> Self {
        Self {
            input,
            condition,
            config,
            stats: ParallelOperatorStats::default(),
        }
    }
}

impl ParallelOperator for ParallelFilterOperator {
    fn execute_parallel(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        let input_result = self.input.execute_parallel(context)?;

        context.base_context.log(&format!("Applying parallel filter to {} rows", input_result.rows.len()));

        // Process rows in parallel batches
        let batch_size = self.config.max_batch_size;
        let mut filtered_rows = Vec::new();

        for chunk in input_result.rows.chunks(batch_size) {
            // For simplicity, process chunks sequentially but could be parallelized
            for row in chunk {
                // Create evaluation context and evaluate condition
                let eval_context = self.create_evaluation_context(&input_result.column_names, row);
                let evaluator = ExpressionEvaluator;

                match evaluator.evaluate(&self.condition, &eval_context)? {
                    value => {
                        match ThreeValuedLogic::from_value(&value) {
                            ThreeValuedLogic::True => {
                                filtered_rows.push(row.clone());
                            }
                            ThreeValuedLogic::False | ThreeValuedLogic::Unknown => {
                                // Filter out this row
                            }
                        }
                    }
                }
            }
        }

        let total_time = start_time.elapsed().as_micros() as u64;
        self.stats.total_rows_processed.store(filtered_rows.len() as u64, Ordering::Relaxed);
        self.stats.total_execution_time_us.store(total_time, Ordering::Relaxed);

        context.base_context.log(&format!("Parallel filter reduced {} rows to {}",
                             input_result.rows.len(), filtered_rows.len()));

        Ok(QueryResult {
            rows: filtered_rows,
            column_names: input_result.column_names,
        })
    }

    fn get_stats(&self) -> &ParallelOperatorStats {
        &self.stats
    }

    fn estimate_resources(&self) -> ResourceRequirement {
        let input_resources = self.input.estimate_resources();

        // Filter typically adds CPU overhead but reduces I/O
        ResourceRequirement {
            cpu_cores: input_resources.cpu_cores * 1.2, // 20% more CPU for filtering
            memory_bytes: input_resources.memory_bytes,
            io_bandwidth: input_resources.io_bandwidth, // Similar I/O initially
            estimated_time_us: input_resources.estimated_time_us,
        }
    }

}

// Additional implementation for ParallelFilterOperator
impl ParallelFilterOperator {
    // Helper method to create evaluation context for filtering
    fn create_evaluation_context(&self, column_names: &[String], row: &[Value]) -> EvaluationContext {
        let mut columns = HashMap::new();

        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }
}

/// Join side for hash operations
#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// Parallel hash join operator
pub struct ParallelHashJoinOperator {
    /// Left input operator
    left_input: Box<dyn ParallelOperator>,
    /// Right input operator
    right_input: Box<dyn ParallelOperator>,
    /// Join type (INNER, LEFT, RIGHT, FULL)
    join_type: JoinType,
    /// Join condition expression
    join_condition: Expression,
    /// Left join key expressions
    left_keys: Vec<Expression>,
    /// Right join key expressions
    right_keys: Vec<Expression>,
    /// Configuration
    config: ParallelOperatorConfig,
    /// Statistics
    stats: ParallelOperatorStats,
    /// Build hash table size threshold
    hash_table_threshold: usize,
    /// Number of hash partitions
    num_partitions: usize,
}

impl ParallelHashJoinOperator {
    /// Create a new parallel hash join operator
    pub fn new(
        left_input: Box<dyn ParallelOperator>,
        right_input: Box<dyn ParallelOperator>,
        join_type: JoinType,
        join_condition: Expression,
        left_keys: Vec<Expression>,
        right_keys: Vec<Expression>,
        config: ParallelOperatorConfig,
    ) -> Self {
        let num_partitions = config.worker_count.next_power_of_two();

        Self {
            left_input,
            right_input,
            join_type,
            join_condition,
            left_keys,
            right_keys,
            config,
            stats: ParallelOperatorStats::default(),
            hash_table_threshold: 10000, // Configurable threshold
            num_partitions,
        }
    }

    /// Set hash table threshold
    pub fn with_hash_threshold(mut self, threshold: usize) -> Self {
        self.hash_table_threshold = threshold;
        self
    }

    /// Set number of partitions
    pub fn with_partitions(mut self, num_partitions: usize) -> Self {
        self.num_partitions = num_partitions.next_power_of_two();
        self
    }

    /// Execute parallel hash join with partitioning
    fn execute_parallel_hash_join(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        context.base_context.log("Starting parallel hash join execution");

        // Get input data
        let left_result = self.left_input.execute_parallel(context)?;
        let right_result = self.right_input.execute_parallel(context)?;

        context.base_context.log(&format!(
            "Hash join inputs: {} left rows, {} right rows",
            left_result.rows.len(),
            right_result.rows.len()
        ));

        // Choose strategy based on data sizes
        if left_result.rows.len() < self.hash_table_threshold {
            self.execute_small_hash_join(&left_result, &right_result, context)
        } else {
            self.execute_partitioned_hash_join(&left_result, &right_result, context)
        }.map(|mut result| {
            // Update statistics
            let total_time = start_time.elapsed().as_micros() as u64;
            self.stats.total_execution_time_us.store(total_time, Ordering::Relaxed);
            self.stats.total_rows_processed.store(
                (left_result.rows.len() + right_result.rows.len()) as u64,
                Ordering::Relaxed
            );

            let total_time_seconds = total_time as f64 / 1_000_000.0;
            if total_time_seconds > 0.0 {
                self.stats.avg_throughput = result.rows.len() as f64 / total_time_seconds;
            }

            context.base_context.log(&format!(
                "Parallel hash join completed: {} result rows in {:.3}s, throughput: {:.2} rows/s",
                result.rows.len(),
                total_time_seconds,
                self.stats.avg_throughput
            ));

            result
        })
    }

    /// Execute hash join for small tables (single partition)
    fn execute_small_hash_join(
        &self,
        left_result: &QueryResult,
        right_result: &QueryResult,
        context: &mut ParallelExecutionContext,
    ) -> Result<QueryResult> {
        context.base_context.log("Using single-partition hash join strategy");

        // Build hash table from right input
        let hash_table = self.build_hash_table(&right_result.rows, &right_result.column_names)?;
        context.base_context.log(&format!("Built hash table with {} partitions", hash_table.len()));

        // Probe with left input
        let join_rows = self.probe_hash_table(&left_result.rows, &left_result.column_names, &hash_table)?;

        let mut column_names = left_result.column_names.clone();
        column_names.extend(right_result.column_names.iter().cloned());

        Ok(QueryResult {
            rows: join_rows,
            column_names,
        })
    }

    /// Execute partitioned hash join for large tables
    fn execute_partitioned_hash_join(
        &self,
        left_result: &QueryResult,
        right_result: &QueryResult,
        context: &mut ParallelExecutionContext,
    ) -> Result<QueryResult> {
        context.base_context.log(&format!("Using partitioned hash join with {} partitions", self.num_partitions));

        // Partition both inputs
        let left_partitions = self.partition_rows(&left_result.rows, &left_result.column_names, Side::Left)?;
        let right_partitions = self.partition_rows(&right_result.rows, &right_result.column_names, Side::Right)?;

        context.base_context.log(&format!(
            "Created partitions: {} left, {} right",
            left_partitions.len(),
            right_partitions.len()
        ));

        // Join each partition in parallel
        let join_handles: Vec<_> = left_partitions
            .into_iter()
            .zip(right_partitions.into_iter())
            .enumerate()
            .map(|(i, (left_partition, right_partition))| {
                let left_keys = self.left_keys.clone();
                let right_keys = self.right_keys.clone();
                let join_type = self.join_type.clone();
                let join_condition = self.join_condition.clone();
                let left_column_names = left_result.column_names.clone();
                let right_column_names = right_result.column_names.clone();

                thread::spawn(move || {
                    Self::join_partition(
                        left_partition,
                        right_partition,
                        left_keys,
                        right_keys,
                        join_type,
                        join_condition,
                        left_column_names,
                        right_column_names,
                    )
                })
            })
            .collect();

        // Collect results from all partitions
        let mut all_join_rows = Vec::new();
        let mut column_names = Vec::new();

        for (i, handle) in join_handles.into_iter().enumerate() {
            match handle.join() {
                Ok(Ok(partition_result)) => {
                    if i == 0 {
                        column_names = partition_result.column_names;
                    }
                    all_join_rows.extend(partition_result.rows);
                }
                Ok(Err(_)) | Err(_) => {
                    return Err(RustgreSQLError::Internal(format!(
                        "Failed to join partition {}",
                        i
                    )));
                }
            }
        }

        Ok(QueryResult {
            rows: all_join_rows,
            column_names,
        })
    }

    /// Build hash table from input rows
    fn build_hash_table(&self, rows: &[Vec<Value>], column_names: &[String]) -> Result<HashMap<u64, Vec<Vec<Value>>>> {
        let mut hash_table: HashMap<u64, Vec<Vec<Value>>> = HashMap::new();

        for row in rows {
            let hash_key = self.calculate_hash_key(row, column_names, Side::Right)?;
            hash_table.entry(hash_key).or_insert_with(Vec::new).push(row.clone());
        }

        Ok(hash_table)
    }

    /// Probe hash table with input rows
    fn probe_hash_table(
        &self,
        rows: &[Vec<Value>],
        column_names: &[String],
        hash_table: &HashMap<u64, Vec<Vec<Value>>>,
    ) -> Result<Vec<Vec<Value>>> {
        let mut join_rows = Vec::new();

        for left_row in rows {
            let hash_key = self.calculate_hash_key(left_row, column_names, Side::Left)?;

            if let Some(right_rows) = hash_table.get(&hash_key) {
                for right_row in right_rows {
                    // Apply additional join conditions
                    if self.evaluate_join_condition(left_row, right_row, column_names, column_names)? {
                        let mut join_row = left_row.clone();
                        join_row.extend_from_slice(right_row);
                        join_rows.push(join_row);
                    }
                }
            }
        }

        Ok(join_rows)
    }

    /// Partition rows based on hash keys
    fn partition_rows(
        &self,
        rows: &[Vec<Value>],
        column_names: &[String],
        side: Side,
    ) -> Result<Vec<Vec<Vec<Value>>>> {
        let mut partitions: Vec<Vec<Vec<Value>>> = vec![Vec::new(); self.num_partitions];
        let partition_mask = self.num_partitions - 1;

        for row in rows {
            let hash_key = self.calculate_hash_key(row, column_names, side)?;
            let partition_id = (hash_key as usize) & partition_mask;
            partitions[partition_id].push(row.clone());
        }

        Ok(partitions)
    }

    /// Join a single partition
    fn join_partition(
        left_partition: Vec<Vec<Value>>,
        right_partition: Vec<Vec<Value>>,
        left_keys: Vec<Expression>,
        right_keys: Vec<Expression>,
        join_type: JoinType,
        join_condition: Expression,
        left_column_names: Vec<String>,
        right_column_names: Vec<String>,
    ) -> Result<QueryResult> {
        // Build hash table from right partition
        let mut hash_table: HashMap<u64, Vec<Vec<Value>>> = HashMap::new();

        for right_row in &right_partition {
            let hash_key = Self::calculate_partition_hash_key(right_row, &right_column_names, &right_keys)?;
            hash_table.entry(hash_key).or_insert_with(Vec::new).push(right_row.clone());
        }

        // Probe with left partition
        let mut join_rows = Vec::new();
        for left_row in &left_partition {
            let hash_key = Self::calculate_partition_hash_key(left_row, &left_column_names, &left_keys)?;

            if let Some(matching_right_rows) = hash_table.get(&hash_key) {
                for right_row in matching_right_rows {
                    if Self::evaluate_partition_join_condition(
                        left_row,
                        right_row,
                        &left_column_names,
                        &right_column_names,
                        &join_condition,
                    )? {
                        let mut join_row = left_row.clone();
                        join_row.extend_from_slice(right_row);
                        join_rows.push(join_row);
                    }
                }
            }
        }

        let mut column_names = left_column_names;
        column_names.extend(right_column_names);

        Ok(QueryResult {
            rows: join_rows,
            column_names,
        })
    }

    /// Calculate hash key for a row
    fn calculate_hash_key(&self, row: &[Value], column_names: &[String], side: Side) -> Result<u64> {
        let keys = match side {
            Side::Left => &self.left_keys,
            Side::Right => &self.right_keys,
        };

        let mut hash_value = 0u64;

        for key in keys {
            let eval_context = self.create_evaluation_context(row, column_names);
            let evaluator = ExpressionEvaluator;
            let key_value = evaluator.evaluate(key, &eval_context)?;

            // Simple hash combination
            hash_value = hash_value.wrapping_mul(31).wrapping_add(self.value_hash(&key_value));
        }

        Ok(hash_value)
    }

    /// Calculate partition hash key
    fn calculate_partition_hash_key(
        row: &[Value],
        column_names: &[String],
        keys: &[Expression],
    ) -> Result<u64> {
        let mut hash_value = 0u64;
        let eval_context = Self::create_partition_evaluation_context(row, column_names);
        let evaluator = ExpressionEvaluator;

        for key in keys {
            let key_value = evaluator.evaluate(key, &eval_context)?;
            hash_value = hash_value.wrapping_mul(31).wrapping_add(Self::value_hash_static(&key_value));
        }

        Ok(hash_value)
    }

    /// Hash a value
    fn value_hash(&self, value: &Value) -> u64 {
        Self::value_hash_static(value)
    }

    /// Hash a value (static version)
    fn value_hash_static(value: &Value) -> u64 {
        match &value.kind {
            ValueKind::Null(NullValue) => 0,
            ValueKind::Integer(i) => *i as u64,
            ValueKind::Float(f) => f.to_bits(),
            ValueKind::String(s) => {
                let mut hash = 0u64;
                for byte in s.as_bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(*byte as u64);
                }
                hash
            }
            ValueKind::Boolean(b) => if *b { 1 } else { 0 },
        }
    }

    /// Evaluate join condition
    fn evaluate_join_condition(
        &self,
        left_row: &[Value],
        right_row: &[Value],
        left_column_names: &[String],
        right_column_names: &[String],
    ) -> Result<bool> {
        let eval_context = self.create_join_evaluation_context(left_row, right_row, left_column_names, right_column_names);
        let evaluator = ExpressionEvaluator;

        match evaluator.evaluate(&self.join_condition, &eval_context)? {
            value => {
                match ThreeValuedLogic::from_value(&value) {
                    ThreeValuedLogic::True => Ok(true),
                    ThreeValuedLogic::False | ThreeValuedLogic::Unknown => Ok(false),
                }
            }
        }
    }

    /// Evaluate partition join condition
    fn evaluate_partition_join_condition(
        left_row: &[Value],
        right_row: &[Value],
        left_column_names: &[String],
        right_column_names: &[String],
        join_condition: &Expression,
    ) -> Result<bool> {
        let eval_context = Self::create_join_evaluation_context_static(
            left_row,
            right_row,
            left_column_names,
            right_column_names,
        );
        let evaluator = ExpressionEvaluator;

        match evaluator.evaluate(join_condition, &eval_context)? {
            value => {
                match ThreeValuedLogic::from_value(&value) {
                    ThreeValuedLogic::True => Ok(true),
                    ThreeValuedLogic::False | ThreeValuedLogic::Unknown => Ok(false),
                }
            }
        }
    }

    /// Create evaluation context for a row
    fn create_evaluation_context(&self, row: &[Value], column_names: &[String]) -> EvaluationContext {
        let mut columns = HashMap::new();

        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Create evaluation context for partition
    fn create_partition_evaluation_context(row: &[Value], column_names: &[String]) -> EvaluationContext {
        let mut columns = HashMap::new();

        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Create join evaluation context
    fn create_join_evaluation_context(
        &self,
        left_row: &[Value],
        right_row: &[Value],
        left_column_names: &[String],
        right_column_names: &[String],
    ) -> EvaluationContext {
        Self::create_join_evaluation_context_static(left_row, right_row, left_column_names, right_column_names)
    }

    /// Create join evaluation context (static version)
    fn create_join_evaluation_context_static(
        left_row: &[Value],
        right_row: &[Value],
        left_column_names: &[String],
        right_column_names: &[String],
    ) -> EvaluationContext {
        let mut columns = HashMap::new();

        // Add left columns with table prefix
        for (i, column_name) in left_column_names.iter().enumerate() {
            if i < left_row.len() {
                columns.insert(format!("l.{}", column_name), left_row[i].clone());
                columns.insert(column_name.clone(), left_row[i].clone());
            }
        }

        // Add right columns with table prefix
        for (i, column_name) in right_column_names.iter().enumerate() {
            if i < right_row.len() {
                columns.insert(format!("r.{}", column_name), right_row[i].clone());
                // Add suffixed version if there might be name conflicts
                columns.insert(format!("{}_r", column_name), right_row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }
}

impl ParallelOperator for ParallelHashJoinOperator {
    fn execute_parallel(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        self.stats.total_tasks.fetch_add(1, Ordering::Relaxed);

        let result = self.execute_parallel_hash_join(context);

        match &result {
            Ok(_) => {
                self.stats.successful_tasks.fetch_add(1, Ordering::Relaxed);

                // Note: parallel_efficiency calculation would require interior mutability
                // This is left as a future improvement
            }
            Err(_) => {
                self.stats.failed_tasks.fetch_add(1, Ordering::Relaxed);
            }
        }

        result
    }

    fn get_stats(&self) -> &ParallelOperatorStats {
        &self.stats
    }

    fn estimate_resources(&self) -> ResourceRequirement {
        let left_resources = self.left_input.estimate_resources();
        let right_resources = self.right_input.estimate_resources();

        // Hash join requires more memory for hash tables but reduces I/O
        ResourceRequirement {
            cpu_cores: (left_resources.cpu_cores + right_resources.cpu_cores) * 1.5, // 50% more CPU
            memory_bytes: (left_resources.memory_bytes + right_resources.memory_bytes) * 2, // 2x memory for hash tables
            io_bandwidth: (left_resources.io_bandwidth + right_resources.io_bandwidth), // Combined I/O
            estimated_time_us: left_resources.estimated_time_us + right_resources.estimated_time_us,
        }
    }
}

// Additional implementation for ParallelHashJoinOperator
impl ParallelHashJoinOperator {
    // Helper method for parallel efficiency calculation
    fn estimate_sequential_time(&self) -> f64 {
        let left_resources = self.left_input.estimate_resources();
        let right_resources = self.right_input.estimate_resources();

        // Estimate hash join would take 1.5x sequential time of inputs
        (left_resources.estimated_time_us + right_resources.estimated_time_us) as f64 * 1.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogManager;

    #[test]
    fn test_parallel_scan_operator_creation() {
        let catalog_manager = Arc::new(CatalogManager::new());

        // Create a simple file manager for testing
        let file_manager = Arc::new(std::sync::Mutex::new(
            crate::storage::file_manager::DefaultFileManager::create("test_scan.db", 8192).unwrap()
        ));
        let buffer_manager = Arc::new(crate::storage::BufferPoolManager::new(100, file_manager));

        // Create a test table first
        catalog_manager.create_table("test_table", vec![
            crate::catalog::ColumnDef {
                name: "id".to_string(),
                data_type: crate::types::DataType {
                    kind: crate::types::DataTypeKind::Integer,
                    nullable: false,
                },
                nullable: false,
                default_value: None,
                primary_key: true,
            },
        ]).unwrap();

        let table_scanner = TableScanner::new(
            catalog_manager.clone(),
            buffer_manager,
            "test_table"
        ).unwrap();

        let parallel_config = ParallelScannerConfig::default();
        let operator_config = ParallelOperatorConfig::default();

        let operator = ParallelScanOperator::from_table_scanner(
            "test_table".to_string(),
            table_scanner,
            parallel_config,
            operator_config,
        );

        assert_eq!(operator.table_name, "test_table");
        assert_eq!(operator.config.worker_count, num_cpus::get());
    }

    #[test]
    fn test_parallel_filter_operator_creation() {
        let catalog_manager = Arc::new(CatalogManager::new());

        // Create a simple file manager for testing
        let file_manager = Arc::new(std::sync::Mutex::new(
            crate::storage::file_manager::DefaultFileManager::create("test_filter.db", 8192).unwrap()
        ));
        let buffer_manager = Arc::new(crate::storage::BufferPoolManager::new(100, file_manager));

        // Create a test table first
        catalog_manager.create_table("test_table", vec![
            crate::catalog::ColumnDef {
                name: "id".to_string(),
                data_type: crate::types::DataType {
                    kind: crate::types::DataTypeKind::Integer,
                    nullable: false,
                },
                nullable: false,
                default_value: None,
                primary_key: true,
            },
        ]).unwrap();

        let table_scanner = TableScanner::new(
            catalog_manager.clone(),
            buffer_manager,
            "test_table"
        ).unwrap();

        let parallel_config = ParallelScannerConfig::default();
        let operator_config = ParallelOperatorConfig::default();

        let scan_operator = Box::new(ParallelScanOperator::from_table_scanner(
            "test_table".to_string(),
            table_scanner,
            parallel_config,
            operator_config,
        ));

        let condition = crate::sql::ast::Expression::Literal(crate::sql::ast::LiteralValue::Boolean(true));
        let filter_operator = ParallelFilterOperator::new(scan_operator, condition, operator_config);

        assert!(!filter_operator.config.adaptive_batching);
    }
}

/// Parallel aggregate operator for multi-threaded aggregation
#[derive(Debug)]
pub struct ParallelAggregateOperator {
    /// Child operator to aggregate data from
    child: Box<dyn ParallelOperator>,
    /// Group by expressions
    group_by: Vec<Expression>,
    /// Aggregate functions
    aggregates: Vec<AggregateFunction>,
    /// Operator configuration
    config: ParallelOperatorConfig,
    /// Performance statistics
    stats: ParallelOperatorStats,
    /// Buffer pool for memory management
    buffer_pool: Option<Arc<ConcurrentBufferPool>>,
    /// Resource manager for execution
    resource_manager: Option<Arc<ResourceManager>>,
    /// Hash table for partitioned aggregation
    partition_hash_table: Vec<Arc<RwLock<HashMap<String, AggregateState>>>>,
}

/// Aggregate function definition
#[derive(Debug, Clone)]
pub struct AggregateFunction {
    /// Function type (COUNT, SUM, AVG, MIN, MAX)
    pub function_type: AggregateFunctionType,
    /// Input expression
    pub input_expr: Expression,
    /// Output column name
    pub output_name: String,
}

/// Types of aggregate functions
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateFunctionType {
    Count,
    Sum,
    Average,
    Minimum,
    Maximum,
}

/// State for tracking aggregate computations
#[derive(Debug, Clone)]
pub struct AggregateState {
    /// Count of values in this group
    pub count: i64,
    /// Sum of numeric values
    pub sum: f64,
    /// Minimum value
    pub minimum: Value,
    /// Maximum value
    pub maximum: Value,
    /// Whether this state has been initialized
    pub initialized: bool,
}

impl Default for AggregateState {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            minimum: Value::null(),
            maximum: Value::null(),
            initialized: false,
        }
    }
}

impl ParallelAggregateOperator {
    /// Create a new parallel aggregate operator
    pub fn new(
        child: Box<dyn ParallelOperator>,
        group_by: Vec<Expression>,
        aggregates: Vec<AggregateFunction>,
        config: ParallelOperatorConfig,
    ) -> Self {
        let num_partitions = config.max_workers;
        let partition_hash_table = (0..num_partitions)
            .map(|_| Arc::new(RwLock::new(HashMap::new())))
            .collect();

        Self {
            child,
            group_by,
            aggregates,
            config,
            stats: ParallelOperatorStats::default(),
            buffer_pool: None,
            resource_manager: None,
            partition_hash_table,
        }
    }

    /// Create with buffer pool
    pub fn with_buffer_pool(
        child: Box<dyn ParallelOperator>,
        group_by: Vec<Expression>,
        aggregates: Vec<AggregateFunction>,
        config: ParallelOperatorConfig,
        buffer_pool: Arc<ConcurrentBufferPool>,
    ) -> Self {
        let num_partitions = config.max_workers;
        let partition_hash_table = (0..num_partitions)
            .map(|_| Arc::new(RwLock::new(HashMap::new())))
            .collect();

        Self {
            child,
            group_by,
            aggregates,
            config,
            stats: ParallelOperatorStats::default(),
            buffer_pool: Some(buffer_pool),
            resource_manager: None,
            partition_hash_table,
        }
    }

    /// Set resource manager
    pub fn with_resource_manager(mut self, resource_manager: Arc<ResourceManager>) -> Self {
        self.resource_manager = Some(resource_manager);
        self
    }

    /// Execute parallel aggregation using partitioned hash aggregation
    fn execute_parallel_aggregation(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        context.base_context.log("Starting parallel aggregation");

        // Step 1: Execute child operator to get input data
        let input_result = self.child.execute_parallel(context)?;
        let input_rows = input_result.rows.len();

        if input_rows == 0 {
            context.base_context.log("No input rows for aggregation");
            return Ok(QueryResult {
                rows: vec![],
                schema: input_result.schema,
            });
        }

        // Step 2: Determine optimal number of partitions based on data size
        let optimal_partitions = self.calculate_optimal_partitions(input_rows);
        context.base_context.log(&format!(
            "Using {} partitions for aggregating {} rows",
            optimal_partitions, input_rows
        ));

        // Step 3: Partition input data and build partial aggregates
        let partition_results = self.build_partial_aggregates(context, &input_result, optimal_partitions)?;

        // Step 4: Combine partial aggregates from all partitions
        let final_result = self.combine_partial_aggregates(context, partition_results)?;

        let execution_time = start_time.elapsed();
        self.stats.update_execution_time(execution_time);
        self.stats.increment_rows_processed(final_result.rows.len());
        self.stats.record_memory_usage(self.estimate_memory_usage(input_rows));

        context.base_context.log(&format!(
            "Parallel aggregation completed: {} output rows in {:?}",
            final_result.rows.len(),
            execution_time
        ));

        Ok(final_result)
    }

    /// Calculate optimal number of partitions based on input size
    fn calculate_optimal_partitions(&self, input_rows: usize) -> usize {
        let available_workers = self.config.max_workers;

        // For small datasets, use fewer partitions to avoid overhead
        if input_rows < 1000 {
            return 1;
        }

        // Scale partitions based on data size
        let recommended_partitions = (input_rows / 10000).max(1).min(available_workers);

        // Use adaptive logic from the optimization
        if let Some(resource_manager) = &self.resource_manager {
            let memory_per_partition = self.estimate_memory_per_partition(input_rows);
            let max_memory_partitions = resource_manager.get_available_memory() / memory_per_partition;

            recommended_partitions.min(max_memory_partitions as usize)
        } else {
            recommended_partitions
        }
    }

    /// Build partial aggregates in parallel partitions
    fn build_partial_aggregates(
        &self,
        context: &mut ParallelExecutionContext,
        input_result: &QueryResult,
        num_partitions: usize,
    ) -> Result<Vec<HashMap<String, AggregateState>>> {
        let mut handles = vec![];
        let chunk_size = (input_result.rows.len() + num_partitions - 1) / num_partitions;

        context.base_context.log(&format!(
            "Building partial aggregates with {} partitions, chunk size: {}",
            num_partitions, chunk_size
        ));

        // Process each partition in parallel
        for partition_id in 0..num_partitions {
            let start_idx = partition_id * chunk_size;
            let end_idx = std::cmp::min(start_idx + chunk_size, input_result.rows.len());

            if start_idx >= input_result.rows.len() {
                break;
            }

            let rows_chunk = input_result.rows[start_idx..end_idx].to_vec();
            let group_by_exprs = self.group_by.clone();
            let aggregates = self.aggregates.clone();

            let handle = std::thread::spawn(move || {
                Self::process_aggregation_partition_static(rows_chunk, group_by_exprs, aggregates, partition_id)
            });

            handles.push(handle);
        }

        // Collect results from all partitions
        let mut partition_results = Vec::new();
        for handle in handles {
            let result = handle.join().map_err(|e| {
                RustgreSQLError::Internal(format!("Thread join error: {:?}", e))
            })?;
            partition_results.push(result);
        }

        Ok(partition_results)
    }

    /// Process a single partition of data for aggregation
    fn process_aggregation_partition(
        &self,
        rows: Vec<Row>,
        group_by_exprs: Vec<Expression>,
        aggregates: Vec<AggregateFunction>,
        _partition_id: usize,
    ) -> HashMap<String, AggregateState> {
        Self::process_aggregation_partition_static(rows, group_by_exprs, aggregates, _partition_id)
    }

    /// Static version of partition processing (for use in threads)
    fn process_aggregation_partition_static(
        rows: Vec<Row>,
        group_by_exprs: Vec<Expression>,
        aggregates: Vec<AggregateFunction>,
        _partition_id: usize,
    ) -> HashMap<String, AggregateState> {
        let mut partition_aggregates: HashMap<String, AggregateState> = HashMap::new();

        for row in rows {
            // Calculate group by key
            let group_key = if group_by_exprs.is_empty() {
                "GLOBAL_GROUP".to_string() // Single group for aggregate without GROUP BY
            } else {
                Self::calculate_group_key_static(&row, &group_by_exprs)
            };

            // Get or create aggregate state for this group
            let aggregate_state = partition_aggregates.entry(group_key).or_default();

            // Update aggregate state
            Self::update_aggregate_state_static(aggregate_state, &row, &aggregates);
        }

        partition_aggregates
    }

    /// Calculate group by key for a row
    fn calculate_group_key(&self, row: &Row, group_by_exprs: &[Expression]) -> String {
        Self::calculate_group_key_static(row, group_by_exprs)
    }

    /// Static version of group key calculation
    fn calculate_group_key_static(row: &Row, group_by_exprs: &[Expression]) -> String {
        let mut key_parts = Vec::new();

        for expr in group_by_exprs {
            // For simplicity, using string representation of evaluated expression
            // In a real implementation, this would properly evaluate expressions
            let value_str = format!("{:?}", row.values.get(0).unwrap_or(&Value::null()));
            key_parts.push(value_str);
        }

        key_parts.join("|")
    }

    /// Update aggregate state with new row data
    fn update_aggregate_state(
        &self,
        state: &mut AggregateState,
        row: &Row,
        aggregates: &[AggregateFunction],
    ) {
        Self::update_aggregate_state_static(state, row, aggregates)
    }

    /// Static version of aggregate state update
    fn update_aggregate_state_static(
        state: &mut AggregateState,
        row: &Row,
        aggregates: &[AggregateFunction],
    ) {
        if !state.initialized {
            state.initialized = true;
            if let Some(first_value) = row.values.first() {
                state.minimum = first_value.clone();
                state.maximum = first_value.clone();
            }
        }

        for aggregate in aggregates {
            // For simplicity, assuming aggregation on first column
            let value = row.values.get(0).unwrap_or(&Value::null());

            match aggregate.function_type {
                AggregateFunctionType::Count => {
                    state.count += 1;
                }
                AggregateFunctionType::Sum => {
                    if let ValueKind::Float(f) = &value.kind {
                        state.sum += f;
                    } else if let ValueKind::Integer(i) = &value.kind {
                        state.sum += *i as f64;
                    }
                }
                AggregateFunctionType::Average => {
                    // Average is calculated later from sum and count
                }
                AggregateFunctionType::Minimum => {
                    if let ValueKind::Float(f) = &value.kind {
                        if let ValueKind::Float(current_min) = &state.minimum.kind {
                            if f < current_min {
                                state.minimum = value.clone();
                            }
                        }
                    }
                }
                AggregateFunctionType::Maximum => {
                    if let ValueKind::Float(f) = &value.kind {
                        if let ValueKind::Float(current_max) = &state.maximum.kind {
                            if f > current_max {
                                state.maximum = value.clone();
                            }
                        }
                    }
                }
            }
        }
    }

    /// Combine partial aggregates from all partitions
    fn combine_partial_aggregates(
        &self,
        context: &mut ParallelExecutionContext,
        partition_results: Vec<HashMap<String, AggregateState>>,
    ) -> Result<QueryResult> {
        let mut combined_aggregates: HashMap<String, AggregateState> = HashMap::new();

        // Merge all partition results
        for partition_map in partition_results {
            for (group_key, partition_state) in partition_map {
                let combined_state = combined_aggregates.entry(group_key).or_default();

                combined_state.count += partition_state.count;
                combined_state.sum += partition_state.sum;

                // Update min/max
                if partition_state.initialized {
                    if !combined_state.initialized {
                        combined_state.minimum = partition_state.minimum.clone();
                        combined_state.maximum = partition_state.maximum.clone();
                        combined_state.initialized = true;
                    } else {
                        // Simple comparison - in real implementation would be type-aware
                        combined_state.minimum = partition_state.minimum;
                        combined_state.maximum = partition_state.maximum;
                    }
                }
            }
        }

        // Generate final result rows
        let mut result_rows = Vec::new();
        for (group_key, aggregate_state) in combined_aggregates {
            let mut result_values = Vec::new();

            // Add group by values
            if self.group_by.is_empty() {
                // No GROUP BY, so no group columns
            } else {
                // Parse group key to extract group by values
                let group_parts: Vec<&str> = group_key.split('|').collect();
                for part in group_parts {
                    result_values.push(Value::string(part.to_string()));
                }
            }

            // Add aggregate results
            for aggregate in &self.aggregates {
                let aggregate_value = match aggregate.function_type {
                    AggregateFunctionType::Count => Value::integer(aggregate_state.count),
                    AggregateFunctionType::Sum => Value::float(aggregate_state.sum),
                    AggregateFunctionType::Average => {
                        if aggregate_state.count > 0 {
                            Value::float(aggregate_state.sum / aggregate_state.count as f64)
                        } else {
                            Value::null()
                        }
                    }
                    AggregateFunctionType::Minimum => aggregate_state.minimum.clone(),
                    AggregateFunctionType::Maximum => aggregate_state.maximum.clone(),
                };
                result_values.push(aggregate_value);
            }

            result_rows.push(Row { values: result_values });
        }

        context.base_context.log(&format!(
            "Combined partial aggregates: {} result groups",
            result_rows.len()
        ));

        // Create result schema (simplified)
        let result_schema = self.create_result_schema()?;

        Ok(QueryResult {
            rows: result_rows,
            schema: result_schema,
        })
    }

    /// Create result schema for aggregation output
    fn create_result_schema(&self) -> Result<Schema> {
        let mut columns = Vec::new();

        // Add GROUP BY columns
        for (i, _expr) in self.group_by.iter().enumerate() {
            columns.push(crate::catalog::ColumnDef {
                name: format!("group_col_{}", i + 1),
                data_type: crate::types::DataType {
                    kind: crate::types::DataTypeKind::Text,
                    nullable: true,
                },
                nullable: true,
                default_value: None,
                primary_key: false,
            });
        }

        // Add aggregate result columns
        for aggregate in &self.aggregates {
            let data_type = match aggregate.function_type {
                AggregateFunctionType::Count | AggregateFunctionType::Sum | AggregateFunctionType::Average => {
                    crate::types::DataTypeKind::Float
                }
                AggregateFunctionType::Minimum | AggregateFunctionType::Maximum => {
                    crate::types::DataTypeKind::Text // Simplified - should be based on input type
                }
            };

            columns.push(crate::catalog::ColumnDef {
                name: aggregate.output_name.clone(),
                data_type: crate::types::DataType {
                    kind: data_type,
                    nullable: true,
                },
                nullable: true,
                default_value: None,
                primary_key: false,
            });
        }

        Ok(Schema { columns })
    }

    /// Estimate memory usage for aggregation
    fn estimate_memory_usage(&self, input_rows: usize) -> usize {
        // Estimate based on number of groups and aggregates
        let estimated_groups = (input_rows as f64 / 100.0).ceil() as usize; // Assume 100 rows per group
        let memory_per_group = self.aggregates.len() * std::mem::size_of::<AggregateState>();

        estimated_groups * memory_per_group + (input_rows * std::mem::size_of::<Value>())
    }

    /// Estimate memory needed per partition
    fn estimate_memory_per_partition(&self, input_rows: usize) -> usize {
        self.estimate_memory_usage(input_rows) / self.config.max_workers
    }
}

impl ParallelOperator for ParallelAggregateOperator {
    fn execute_parallel(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        // Check for adaptive parallelism
        if self.config.adaptive_parallelism {
            return self.execute_adaptive_aggregation(context);
        }

        self.execute_parallel_aggregation(context)
    }

    fn get_stats(&self) -> &ParallelOperatorStats {
        &self.stats
    }

    fn estimate_resources(&self) -> ResourceRequirement {
        // Memory intensive operation requiring multiple hash tables
        let base_memory = 1024 * 1024; // 1MB base
        let per_aggregate_memory = 512 * 1024; // 512KB per aggregate function
        let per_worker_memory = 2 * 1024 * 1024; // 2MB per worker thread

        ResourceRequirement {
            cpu_cores: self.config.max_workers as f64,
            memory_bytes: (base_memory +
                (self.aggregates.len() * per_aggregate_memory) +
                (self.config.max_workers * per_worker_memory)) as u64,
            io_bandwidth: 0, // Primarily compute-bound
        }
    }
}

// Additional implementation for ParallelAggregateOperator
impl ParallelAggregateOperator {
    /// Execute aggregation with adaptive parallelism based on data characteristics
    fn execute_adaptive_aggregation(&self, context: &mut ParallelExecutionContext) -> Result<QueryResult> {
        let start_time = Instant::now();
        context.base_context.log("Starting adaptive parallel aggregation");

        // Get a sample of input data to estimate characteristics
        let input_result = self.child.execute_parallel(context)?;
        let sample_size = std::cmp::min(1000, input_result.rows.len());
        let sample_rows = &input_result.rows[..sample_size];

        // Estimate optimal workers based on data characteristics
        let estimated_workers = self.estimate_optimal_workers_sample(sample_rows);

        context.base_context.log(&format!(
            "Adaptive aggregation: using {} workers for {} input rows",
            estimated_workers, input_result.rows.len()
        ));

        // Create a modified config with optimal workers
        let mut adaptive_config = self.config;
        adaptive_config.max_workers = estimated_workers;

        // Create adaptive operator and execute
        let adaptive_operator = ParallelAggregateOperator {
            child: self.child.clone_operator(),
            group_by: self.group_by.clone(),
            aggregates: self.aggregates.clone(),
            config: adaptive_config,
            stats: ParallelOperatorStats::default(),
            buffer_pool: self.buffer_pool.clone(),
            resource_manager: self.resource_manager.clone(),
            partition_hash_table: (0..estimated_workers)
                .map(|_| Arc::new(RwLock::new(HashMap::new())))
                .collect(),
        };

        let result = adaptive_operator.execute_parallel_aggregation(context)?;

        let execution_time = start_time.elapsed();
        context.base_context.log(&format!(
            "Adaptive aggregation completed in {:?} with {} workers",
            execution_time, estimated_workers
        ));

        Ok(result)
    }

    /// Estimate optimal workers based on sample data characteristics
    fn estimate_optimal_workers_sample(&self, sample_rows: &[Row]) -> usize {
        self.estimate_optimal_workers_sample_with_groups(sample_rows, &self.group_by)
    }

    /// Static version for estimating optimal workers
    fn estimate_optimal_workers_sample_with_groups(
        sample_rows: &[Row],
        group_by_exprs: &[Expression],
    ) -> usize {
        if sample_rows.is_empty() {
            return 1;
        }

        // Analyze sample characteristics
        let mut unique_groups = std::collections::HashSet::new();
        let mut total_values = 0;

        for row in sample_rows {
            // Simple group key calculation
            let group_key = if group_by_exprs.is_empty() {
                "GLOBAL_GROUP".to_string()
            } else {
                Self::calculate_group_key_static(row, group_by_exprs)
            };

            unique_groups.insert(group_key);
            total_values += row.values.len();
        }

        // More workers for:
        // 1. Large number of groups (better parallelism)
        // 2. Many values per row (more computation)
        // 3. Multiple aggregate functions

        let group_factor = (unique_groups.len() as f64 / 100.0).sqrt() as usize;
        let value_factor = (total_values as f64 / 500.0) as usize;
        let aggregate_factor = 1; // Default factor

        let base_workers = 1 + group_factor + value_factor + aggregate_factor;
        base_workers.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::parallel::context::ParallelExecutionContext;
    use crate::executor::ExecutionContext;
    use crate::storage::buffer::BufferManager;
    use crate::catalog::CatalogManager;
    use crate::storage::file::DefaultFileManager;

    #[test]
    fn test_parallel_aggregate_operator_creation() {
        let child = Box::new(ParallelScanOperator::from_table_scanner(
            "test_table".to_string(),
            crate::executor::parallel::scanner::DefaultParallelScanner::new(
                std::sync::Arc::new(CatalogManager::new()),
                std::sync::Arc::new(BufferManager::new(1000)),
                "test_table",
            ).unwrap(),
            crate::executor::parallel::scanner::ParallelScannerConfig::default(),
            crate::executor::parallel::operators::ParallelOperatorConfig::default(),
        ));

        let aggregates = vec![
            AggregateFunction {
                function_type: AggregateFunctionType::Count,
                input_expr: crate::sql::ast::Expression::Literal(crate::sql::ast::LiteralValue::Integer(1)),
                output_name: "count".to_string(),
            },
            AggregateFunction {
                function_type: AggregateFunctionType::Sum,
                input_expr: crate::sql::ast::Expression::Literal(crate::sql::ast::LiteralValue::Integer(1)),
                output_name: "sum".to_string(),
            },
        ];

        let operator = ParallelAggregateOperator::new(
            child,
            vec![], // No GROUP BY
            aggregates,
            ParallelOperatorConfig::default(),
        );

        assert_eq!(operator.aggregates.len(), 2);
        assert_eq!(operator.group_by.len(), 0);
    }

    #[test]
    fn test_aggregate_state_default() {
        let state = AggregateState::default();
        assert_eq!(state.count, 0);
        assert_eq!(state.sum, 0.0);
        assert!(!state.initialized);
    }

    #[test]
    fn test_parallel_aggregate_with_group_by() {
        let child = Box::new(ParallelScanOperator::from_table_scanner(
            "test_table".to_string(),
            crate::executor::parallel::scanner::DefaultParallelScanner::new(
                std::sync::Arc::new(CatalogManager::new()),
                std::sync::Arc::new(BufferManager::new(1000)),
                "test_table",
            ).unwrap(),
            crate::executor::parallel::scanner::ParallelScannerConfig::default(),
            crate::executor::parallel::operators::ParallelOperatorConfig::default(),
        ));

        let aggregates = vec![
            AggregateFunction {
                function_type: AggregateFunctionType::Count,
                input_expr: crate::sql::ast::Expression::Literal(crate::sql::ast::LiteralValue::Integer(1)),
                output_name: "count".to_string(),
            },
        ];

        let group_by = vec![
            crate::sql::ast::Expression::Column("id".to_string()),
        ];

        let operator = ParallelAggregateOperator::new(
            child,
            group_by,
            aggregates,
            ParallelOperatorConfig::default(),
        );

        assert_eq!(operator.aggregates.len(), 1);
        assert_eq!(operator.group_by.len(), 1);
    }
}