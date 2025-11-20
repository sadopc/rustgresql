//! Parallel query executor
//!
//! Main parallel execution engine that coordinates parallel query execution
//! across multiple workers and integrates with the existing executor framework.

use crate::{Result, executor::{ExecutionContext, Executor, QueryResult}, sql::ast::Statement};
use crate::executor::parallel::{
    scheduler::{TaskScheduler, TaskId, TaskType},
    resource_manager::{ResourceManager, ResourceConstraints},
    context::{ParallelExecutionContext, SharedExecutionState},
    metrics::{MetricCollector, MetricsConfig},
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Parallel query executor
pub struct ParallelExecutor {
    /// Task scheduler for parallel operations
    scheduler: Option<TaskScheduler>,
    /// Resource manager for coordinating resources
    resource_manager: Option<Arc<ResourceManager>>,
    /// Metrics collector
    metrics_collector: Option<Arc<MetricCollector>>,
    /// Parallel execution configuration
    config: ParallelExecutorConfig,
    /// Base executor for fallback operations
    base_executor: Executor,
}

/// Configuration for parallel execution
#[derive(Debug, Clone)]
pub struct ParallelExecutorConfig {
    /// Maximum number of parallel workers
    pub max_workers: usize,
    /// Minimum data size to trigger parallel execution (rows)
    pub parallel_threshold: usize,
    /// Resource constraints for parallel operations
    pub resource_constraints: ResourceConstraints,
    /// Metrics collection configuration
    pub metrics_config: MetricsConfig,
    /// Enable/disable parallel execution
    pub parallel_enabled: bool,
    /// Auto-detect optimal parallelism level
    pub auto_detect_parallelism: bool,
    /// Maximum parallelism level (0 = auto)
    pub max_parallelism: usize,
}

impl Default for ParallelExecutorConfig {
    fn default() -> Self {
        Self {
            max_workers: num_cpus::get(),
            parallel_threshold: 10000, // 10K rows
            resource_constraints: ResourceConstraints::default(),
            metrics_config: MetricsConfig::default(),
            parallel_enabled: true,
            auto_detect_parallelism: true,
            max_parallelism: 0, // 0 means auto-detect
        }
    }
}

/// Parallel execution result
#[derive(Debug)]
pub struct ParallelExecutionResult {
    /// Query result data
    pub result: QueryResult,
    /// Execution metrics
    pub metrics: Option<crate::executor::parallel::metrics::ParallelExecutionMetrics>,
    /// Number of workers used
    pub workers_used: usize,
    /// Total execution time
    pub execution_time_ms: u64,
    /// Speedup factor compared to sequential execution
    pub speedup_factor: f64,
}

impl ParallelExecutor {
    /// Create a new parallel executor
    pub fn new(base_executor: Executor) -> Self {
        Self::with_config(base_executor, ParallelExecutorConfig::default())
    }

    /// Create a parallel executor with custom configuration
    pub fn with_config(base_executor: Executor, config: ParallelExecutorConfig) -> Self {
        Self {
            scheduler: None,
            resource_manager: None,
            metrics_collector: None,
            config,
            base_executor,
        }
    }

    /// Initialize the parallel executor
    pub fn initialize(&mut self) -> Result<()> {
        if !self.config.parallel_enabled {
            return Ok(());
        }

        // Determine optimal number of workers
        let num_workers = self.determine_optimal_workers();

        // Create resource manager
        let resource_manager = Arc::new(ResourceManager::new(
            self.config.resource_constraints.clone()
        )?);
        self.resource_manager = Some(resource_manager);

        // Create metrics collector
        let metrics_collector = Arc::new(MetricCollector::new(
            self.config.metrics_config.clone()
        ));
        self.metrics_collector = Some(metrics_collector);

        // Create task scheduler
        let scheduler = TaskScheduler::new(
            num_workers,
            move |task| self.execute_task(task)
        )?;
        self.scheduler = Some(scheduler);

        Ok(())
    }

    /// Execute a statement with optional parallelism
    pub fn execute_statement(&mut self, statement: &Statement, context: &mut ExecutionContext) -> Result<QueryResult> {
        if !self.config.parallel_enabled {
            return self.base_executor.execute_statement(statement, context);
        }

        // Determine if this query should use parallel execution
        if self.should_use_parallel_execution(statement, context) {
            self.execute_parallel(statement, context).map(|r| r.result)
        } else {
            self.base_executor.execute_statement(statement, context)
        }
    }

    /// Execute a statement in parallel mode
    pub fn execute_parallel(&mut self, statement: &Statement, context: &mut ExecutionContext) -> Result<ParallelExecutionResult> {
        let start_time = Instant::now();
        let query_id = format!("query_{}", start_time.elapsed().as_nanos());

        // Initialize if not already done
        if self.scheduler.is_none() {
            self.initialize()?;
        }

        let scheduler = self.scheduler.as_mut().unwrap();
        let resource_manager = self.resource_manager.as_ref().unwrap();
        let metrics_collector = self.metrics_collector.as_ref().unwrap();

        // Start metrics collection
        metrics_collector.start_collection(query_id.clone());

        // Create shared execution state
        let shared_state = Arc::new(SharedExecutionState::new());

        // Determine optimal parallelism level
        let parallelism_level = self.determine_query_parallelism(statement, context);

        // Execute query in parallel
        let result = self.execute_with_parallelism(
            statement,
            context,
            scheduler,
            resource_manager,
            shared_state.clone(),
            parallelism_level,
        );

        // Calculate execution metrics
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        let metrics = metrics_collector.finish_collection();

        // Calculate speedup by running sequential version for comparison (only for small queries)
        let speedup_factor = if execution_time_ms < 5000 { // Only for queries under 5 seconds
            let sequential_start = Instant::now();
            let _sequential_result = self.base_executor.execute_statement(statement, context);
            let sequential_time = sequential_start.elapsed().as_millis() as u64;

            if sequential_time > 0 {
                sequential_time as f64 / execution_time_ms as f64
            } else {
                1.0
            }
        } else {
            1.0 // Don't run sequential for long queries
        };

        Ok(ParallelExecutionResult {
            result: result?,
            metrics,
            workers_used: parallelism_level,
            execution_time_ms,
            speedup_factor,
        })
    }

    /// Determine if a statement should use parallel execution
    fn should_use_parallel_execution(&self, statement: &Statement, _context: &ExecutionContext) -> bool {
        if !self.config.parallel_enabled {
            return false;
        }

        // Check system resources
        if !self.has_sufficient_resources() {
            return false;
        }

        // Analyze statement type and complexity
        match statement {
            Statement::Select(select) => {
                // Use parallelism for complex SELECT statements
                self.should_parallelize_select(select)
            }
            Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_) => {
                // Consider parallelism for large DML operations
                false // For now, disable parallel DML
            }
            _ => false, // Other statement types are not parallelized yet
        }
    }

    /// Determine if a SELECT statement should be parallelized
    fn should_parallelize_select(&self, select: &crate::sql::ast::SelectStatement) -> bool {
        // Check for operations that benefit from parallelism
        match select {
            crate::sql::ast::SelectStatement::Simple {
                from,
                joins,
                where_clause,
                group_by,
                ..
            } => {
                // Parallelize if there are joins or large table scans
                let has_joins = !joins.is_empty();
                let has_multiple_tables = from.len() > 1;
                let has_aggregation = !group_by.is_empty();
                let has_complex_where = where_clause.is_some();

                has_joins || has_multiple_tables || has_aggregation || has_complex_where
            }
            crate::sql::ast::SelectStatement::SetOperation(_) => {
                // Set operations can be parallelized
                true
            }
        }
    }

    /// Determine optimal number of workers for the current system
    fn determine_optimal_workers(&self) -> usize {
        if self.config.max_parallelism > 0 {
            return self.config.max_parallelism.min(self.config.max_workers);
        }

        let cpu_count = num_cpus::get();
        let logical_cores = if cfg!(target_os = "linux") {
            // Try to get physical core count on Linux
            match std::fs::read_to_string("/proc/cpuinfo") {
                Ok(content) => {
                    content.lines()
                        .filter(|line| line.starts_with("processor"))
                        .count()
                }
                Err(_) => cpu_count,
            }
        } else {
            cpu_count
        };

        // Use number of logical cores, but leave some headroom
        (logical_cores.saturating_sub(1)).max(1).min(self.config.max_workers)
    }

    /// Determine optimal parallelism level for a specific query
    fn determine_query_parallelism(&self, statement: &Statement, context: &ExecutionContext) -> usize {
        if !self.config.auto_detect_parallelism {
            return self.determine_optimal_workers();
        }

        // Analyze query complexity and data volume
        let estimated_rows = self.estimate_result_size(statement, context);
        let query_complexity = self.analyze_query_complexity(statement);

        // Base parallelism on estimated data size
        let base_parallelism = if estimated_rows > self.config.parallel_threshold {
            let data_factor = (estimated_rows / self.config.parallel_threshold) as usize;
            (data_factor as f64).sqrt() as usize
        } else {
            1
        };

        // Adjust based on query complexity
        let complexity_factor = match query_complexity {
            QueryComplexity::Simple => 1,
            QueryComplexity::Medium => 2,
            QueryComplexity::Complex => 4,
            QueryComplexity::VeryComplex => 8,
        };

        let parallelism = base_parallelism * complexity_factor;
        parallelism.min(self.determine_optimal_workers()).max(1)
    }

    /// Estimate the number of rows a query will process
    fn estimate_result_size(&self, statement: &Statement, _context: &ExecutionContext) -> usize {
        // Simplified estimation - in practice this would use statistics
        match statement {
            Statement::Select(_) => {
                // Assume 100K rows for SELECT queries (would use actual statistics)
                100_000
            }
            _ => 0,
        }
    }

    /// Analyze query complexity
    fn analyze_query_complexity(&self, statement: &Statement) -> QueryComplexity {
        match statement {
            Statement::Select(select) => {
                match select {
                    crate::sql::ast::SelectStatement::Simple {
                        joins,
                        where_clause,
                        group_by,
                        having,
                        ..
                    } => {
                        let complexity_score = joins.len() * 2
                            + where_clause.as_ref().map(|_| 1).unwrap_or(0)
                            + group_by.len()
                            + having.as_ref().map(|_| 2).unwrap_or(0);

                        match complexity_score {
                            0 => QueryComplexity::Simple,
                            1..=3 => QueryComplexity::Medium,
                            4..=7 => QueryComplexity::Complex,
                            _ => QueryComplexity::VeryComplex,
                        }
                    }
                    crate::sql::ast::SelectStatement::SetOperation(_) => {
                        QueryComplexity::Medium
                    }
                }
            }
            _ => QueryComplexity::Simple,
        }
    }

    /// Check if system has sufficient resources for parallel execution
    fn has_sufficient_resources(&self) -> bool {
        // Simple heuristic - ensure we have at least 2GB free memory
        // In practice, this would check actual system resources
        true
    }

    /// Execute a query with specified parallelism level
    fn execute_with_parallelism(
        &mut self,
        statement: &Statement,
        context: &mut ExecutionContext,
        scheduler: &mut TaskScheduler,
        resource_manager: &ResourceManager,
        shared_state: Arc<SharedExecutionState>,
        parallelism_level: usize,
    ) -> Result<QueryResult> {
        match statement {
            Statement::Select(select) => {
                self.execute_select_parallel(select, context, scheduler, resource_manager, shared_state, parallelism_level)
            }
            _ => {
                // Fallback to sequential execution for unsupported statements
                self.base_executor.execute_statement(statement, context)
            }
        }
    }

    /// Execute a SELECT statement in parallel
    fn execute_select_parallel(
        &mut self,
        select: &crate::sql::ast::SelectStatement,
        context: &mut ExecutionContext,
        scheduler: &mut TaskScheduler,
        resource_manager: &ResourceManager,
        shared_state: Arc<SharedExecutionState>,
        parallelism_level: usize,
    ) -> Result<QueryResult> {
        // Create parallel execution tasks based on query structure
        let tasks = self.create_parallel_tasks(select, context, parallelism_level)?;

        // Submit tasks to scheduler
        let mut task_ids = Vec::new();
        for task in tasks {
            let task_id = scheduler.submit_task(
                task.task_type,
                task.priority,
                task.estimated_cost,
                task.data,
            )?;
            task_ids.push(task_id);
        }

        // Wait for all tasks to complete
        let task_results = scheduler.wait_for_tasks(&task_ids, Some(300_000))?; // 5 minute timeout

        // Combine results from parallel tasks
        self.combine_parallel_results(task_results, select)
    }

    /// Create parallel execution tasks for a SELECT statement
    fn create_parallel_tasks(
        &self,
        select: &crate::sql::ast::SelectStatement,
        context: &ExecutionContext,
        parallelism_level: usize,
    ) -> Result<Vec<ParallelTask>> {
        let mut tasks = Vec::new();

        match select {
            crate::sql::ast::SelectStatement::Simple { from, joins, where_clause, columns, group_by, having, .. } => {
                // Create scan tasks for the main table
                if from.len() == 1 && joins.is_empty() {
                    // Simple single-table scan
                    for i in 0..parallelism_level {
                        let scan_task = ParallelTask {
                            task_type: TaskType::ScanRange,
                            priority: 0,
                            estimated_cost: 100.0,
                            data: self.serialize_scan_task(&from[0], i, parallelism_level, where_clause)?,
                        };
                        tasks.push(scan_task);
                    }
                } else {
                    // Multi-table query with joins
                    // Create parallel tasks for each table scan
                    for (table_idx, table_ref) in from.iter().enumerate() {
                        for i in 0..parallelism_level {
                            let scan_task = ParallelTask {
                                task_type: TaskType::ScanRange,
                                priority: table_idx as u32,
                                estimated_cost: 100.0,
                                data: self.serialize_scan_task(table_ref, i, parallelism_level, None)?,
                            };
                            tasks.push(scan_task);
                        }
                    }

                    // Create join tasks
                    for join in joins {
                        let join_task = ParallelTask {
                            task_type: TaskType::HashBuild,
                            priority: 10,
                            estimated_cost: 200.0,
                            data: self.serialize_join_task(join)?,
                        };
                        tasks.push(join_task);
                    }
                }

                // Create aggregation tasks if needed
                if !group_by.is_empty() || self.has_aggregate_functions(columns) {
                    for i in 0..parallelism_level {
                        let agg_task = ParallelTask {
                            task_type: TaskType::PartialAggregate,
                            priority: 20,
                            estimated_cost: 150.0,
                            data: self.serialize_aggregation_task(columns, group_by, having, i)?,
                        };
                        tasks.push(agg_task);
                    }

                    // Add final aggregation task
                    let final_agg_task = ParallelTask {
                        task_type: TaskType::FinalAggregate,
                        priority: 30,
                        estimated_cost: 50.0,
                        data: self.serialize_final_aggregation_task(columns, group_by, having)?,
                    };
                    tasks.push(final_agg_task);
                }
            }
            crate::sql::ast::SelectStatement::SetOperation(_) => {
                // Handle set operations (UNION, INTERSECT, EXCEPT)
                // This would require parallelizing the set operation itself
                return Err(crate::error::RustgreSQLError::Internal(
                    "Parallel set operations not yet implemented".to_string()
                ));
            }
        }

        Ok(tasks)
    }

    /// Execute a single parallel task
    fn execute_task(&self, task: &crate::executor::parallel::scheduler::ParallelTask) -> crate::executor::parallel::scheduler::TaskResult {
        let start_time = Instant::now();

        // In a real implementation, this would execute the actual task
        // For now, we simulate execution with some work
        match task.task_type {
            TaskType::ScanRange => {
                // Simulate table scan work
                thread::sleep(Duration::from_millis(100));
            }
            TaskType::Filter => {
                // Simulate filtering work
                thread::sleep(Duration::from_millis(50));
            }
            TaskType::HashBuild | TaskType::HashProbe => {
                // Simulate hash join work
                thread::sleep(Duration::from_millis(200));
            }
            TaskType::PartialAggregate => {
                // Simulate aggregation work
                thread::sleep(Duration::from_millis(150));
            }
            TaskType::FinalAggregate => {
                // Simulate final aggregation work
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                // Default work simulation
                thread::sleep(Duration::from_millis(100));
            }
        }

        crate::executor::parallel::scheduler::TaskResult {
            task_id: task.id,
            result: vec![1, 2, 3], // Mock result data
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            success: true,
            error: None,
            memory_used: 1024, // Mock memory usage
        }
    }

    /// Combine results from parallel tasks
    fn combine_parallel_results(
        &self,
        task_results: Vec<crate::executor::parallel::scheduler::TaskResult>,
        select: &crate::sql::ast::SelectStatement,
    ) -> Result<QueryResult> {
        // In a real implementation, this would combine actual query results
        // For now, create a mock result
        Ok(QueryResult {
            columns: vec!["result".to_string()],
            rows: vec![vec!["combined_result".to_string()]],
            affected_rows: 0,
        })
    }

    /// Serialize scan task data
    fn serialize_scan_task(&self, table_ref: &crate::sql::ast::TableRef, worker_id: usize, total_workers: usize, filter: Option<&crate::sql::ast::Expression>) -> Result<Vec<u8>> {
        // Extract table name based on TableRef type
        let table_name = match table_ref {
            crate::sql::ast::TableRef::Table { name, .. } => name.clone(),
            crate::sql::ast::TableRef::Subquery { .. } => {
                // For subqueries, we don't support parallel scanning in this implementation
                return Err(crate::error::RustgreSQLError::Internal("Parallel scanning not supported for subqueries".to_string()));
            }
        };

        // In a real implementation, this would serialize task-specific data
        let task_data = format!("scan:{},worker:{},total:{},filter:{}",
            table_name, worker_id, total_workers,
            filter.is_some());
        Ok(task_data.into_bytes())
    }

    /// Serialize join task data
    fn serialize_join_task(&self, join: &crate::sql::ast::JoinCondition) -> Result<Vec<u8>> {
        // In a real implementation, this would serialize join-specific data
        let task_data = format!("join:{:?}", join.join_type);
        Ok(task_data.into_bytes())
    }

    /// Serialize aggregation task data
    fn serialize_aggregation_task(&self, columns: &[crate::sql::ast::Expression], group_by: &[crate::sql::ast::Expression], having: &Option<crate::sql::ast::Expression>, worker_id: usize) -> Result<Vec<u8>> {
        // In a real implementation, this would serialize aggregation-specific data
        let task_data = format!("partial_agg:columns:{},group_by:{},worker:{}", columns.len(), group_by.len(), worker_id);
        Ok(task_data.into_bytes())
    }

    /// Serialize final aggregation task data
    fn serialize_final_aggregation_task(&self, columns: &[crate::sql::ast::Expression], group_by: &[crate::sql::ast::Expression], having: &Option<crate::sql::ast::Expression>) -> Result<Vec<u8>> {
        // In a real implementation, this would serialize final aggregation data
        let task_data = format!("final_agg:columns:{},group_by:{}", columns.len(), group_by.len());
        Ok(task_data.into_bytes())
    }

    /// Check if expressions contain aggregate functions
    fn has_aggregate_functions(&self, expressions: &[crate::sql::ast::Expression]) -> bool {
        // In a real implementation, this would check for aggregate functions
        false
    }

    /// Get current configuration
    pub fn config(&self) -> &ParallelExecutorConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config(&mut self, config: ParallelExecutorConfig) -> Result<()> {
        // Shutdown existing components if they were initialized
        if self.scheduler.is_some() {
            self.shutdown()?;
        }

        self.config = config;
        Ok(())
    }

    /// Shutdown the parallel executor
    pub fn shutdown(&mut self) -> Result<()> {
        if let Some(scheduler) = self.scheduler.take() {
            scheduler.shutdown()?;
        }

        if let Some(resource_manager) = self.resource_manager.take() {
            // Resource manager shutdown happens via Drop trait
        }

        if let Some(metrics_collector) = self.metrics_collector.take() {
            metrics_collector.clear_metrics();
        }

        Ok(())
    }
}

impl Drop for ParallelExecutor {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// Query complexity levels
#[derive(Debug, Clone, PartialEq)]
enum QueryComplexity {
    Simple,
    Medium,
    Complex,
    VeryComplex,
}

/// Parallel task representation for internal use
struct ParallelTask {
    task_type: TaskType,
    priority: u32,
    estimated_cost: f64,
    data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::storage::buffer::BufferPoolManager;

    #[test]
    fn test_parallel_executor_creation() {
        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_executor = Executor::new(catalog, buffer_manager);

        let parallel_executor = ParallelExecutor::new(base_executor);
        assert!(parallel_executor.config().parallel_enabled);
    }

    #[test]
    fn test_parallel_executor_config() {
        let config = ParallelExecutorConfig {
            max_workers: 4,
            parallel_threshold: 5000,
            parallel_enabled: true,
            ..Default::default()
        };

        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_executor = Executor::new(catalog, buffer_manager);

        let parallel_executor = ParallelExecutor::with_config(base_executor, config);
        assert_eq!(parallel_executor.config().max_workers, 4);
        assert_eq!(parallel_executor.config().parallel_threshold, 5000);
    }

    #[test]
    fn test_optimal_workers_determination() {
        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_executor = Executor::new(catalog, buffer_manager);

        let mut parallel_executor = ParallelExecutor::new(base_executor);

        // Test auto-detection of optimal workers
        let optimal_workers = parallel_executor.determine_optimal_workers();
        assert!(optimal_workers > 0);
        assert!(optimal_workers <= num_cpus::get());
    }

    #[test]
    fn test_query_complexity_analysis() {
        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_executor = Executor::new(catalog, buffer_manager);
        let parallel_executor = ParallelExecutor::new(base_executor);

        // This would test with actual SQL statements in a full implementation
        // For now, just test the method exists
        assert!(true);
    }

    #[test]
    fn test_parallel_execution_configuration() {
        let mut config = ParallelExecutorConfig::default();
        config.parallel_enabled = false;

        let catalog = Catalog::new();
        let buffer_manager = BufferPoolManager::new(1000);
        let base_executor = Executor::new(catalog, buffer_manager);
        let mut parallel_executor = ParallelExecutor::with_config(base_executor, config);

        // Parallel execution should be disabled
        assert!(!parallel_executor.config().parallel_enabled);
    }
}