//! Parallel execution metrics collection
//!
//! Provides comprehensive metrics collection for parallel query execution
//! to monitor performance and identify bottlenecks.

use crate::executor::parallel::context::AggregateWorkerStats;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Comprehensive metrics for parallel execution
#[derive(Debug, Clone)]
pub struct ParallelExecutionMetrics {
    /// Query identifier
    pub query_id: String,
    /// Execution start time
    pub start_time: Instant,
    /// Execution end time (None if still running)
    pub end_time: Option<Instant>,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Number of parallel workers used
    pub workers_used: usize,
    /// CPU utilization statistics
    pub cpu_metrics: CpuMetrics,
    /// Memory usage statistics
    pub memory_metrics: MemoryMetrics,
    /// I/O performance statistics
    pub io_metrics: IoMetrics,
    /// Load balancing metrics
    pub load_balance_metrics: LoadBalanceMetrics,
    /// Task scheduling metrics
    pub scheduling_metrics: SchedulingMetrics,
    /// Data processing metrics
    pub processing_metrics: ProcessingMetrics,
    /// Error metrics
    pub error_metrics: ErrorMetrics,
}

/// CPU utilization metrics
#[derive(Debug, Clone)]
pub struct CpuMetrics {
    /// Overall CPU utilization percentage (0.0 to 1.0)
    pub utilization: f64,
    /// Average CPU utilization across workers
    pub avg_worker_utilization: f64,
    /// Maximum CPU utilization of any worker
    pub max_worker_utilization: f64,
    /// Minimum CPU utilization of any worker
    pub min_worker_utilization: f64,
    /// CPU time spent in system calls
    pub system_cpu_time_ms: u64,
    /// CPU time spent in user space
    pub user_cpu_time_ms: u64,
    /// Context switches
    pub context_switches: u64,
}

/// Memory usage metrics
#[derive(Debug, Clone)]
pub struct MemoryMetrics {
    /// Peak memory usage in bytes
    pub peak_memory_usage: usize,
    /// Average memory usage in bytes
    pub avg_memory_usage: usize,
    /// Memory allocated per worker
    pub memory_per_worker: Vec<usize>,
    /// Memory deallocations count
    pub deallocations: u64,
    /// Memory allocation failures
    pub allocation_failures: u64,
    /// Garbage collection statistics (if applicable)
    pub gc_stats: Option<GcStats>,
}

/// Garbage collection statistics
#[derive(Debug, Clone)]
pub struct GcStats {
    /// Number of garbage collections
    pub collections: u64,
    /// Total time spent in garbage collection
    pub total_time_ms: u64,
    /// Memory freed by garbage collection
    pub memory_freed: usize,
}

/// I/O performance metrics
#[derive(Debug, Clone)]
pub struct IoMetrics {
    /// Total I/O operations
    pub total_io_operations: u64,
    /// Number of read operations
    pub read_operations: u64,
    /// Number of write operations
    pub write_operations: u64,
    /// Bytes read
    pub bytes_read: u64,
    /// Bytes written
    pub bytes_written: u64,
    /// Average I/O latency in microseconds
    pub avg_io_latency_us: f64,
    /// Maximum I/O latency
    pub max_io_latency_us: u64,
    /// I/O throughput in MB/s
    pub io_throughput_mbps: f64,
    /// I/O wait time in milliseconds
    pub io_wait_time_ms: u64,
}

/// Load balancing metrics
#[derive(Debug, Clone)]
pub struct LoadBalanceMetrics {
    /// Load imbalance score (0.0 = perfect balance, 1.0 = severe imbalance)
    pub load_imbalance_score: f64,
    /// Work distribution efficiency (0.0 to 1.0)
    pub work_distribution_efficiency: f64,
    /// Task steal count
    pub task_steals: u64,
    /// Worker idle time percentage
    pub avg_worker_idle_time_percent: f64,
    /// Time spent in work stealing
    pub work_stealing_time_ms: u64,
}

/// Task scheduling metrics
#[derive(Debug, Clone)]
pub struct SchedulingMetrics {
    /// Total tasks scheduled
    pub total_tasks_scheduled: u64,
    /// Tasks completed successfully
    pub tasks_completed: u64,
    /// Tasks failed
    pub tasks_failed: u64,
    /// Average task execution time in milliseconds
    pub avg_task_execution_time_ms: f64,
    /// Maximum task execution time
    pub max_task_execution_time_ms: u64,
    /// Minimum task execution time
    pub min_task_execution_time_ms: u64,
    /// Queue wait time statistics
    pub queue_wait_time: QueueWaitTime,
}

/// Queue wait time metrics
#[derive(Debug, Clone)]
pub struct QueueWaitTime {
    /// Average time tasks spent in queue
    pub avg_wait_time_ms: f64,
    /// Maximum time any task spent in queue
    pub max_wait_time_ms: u64,
    /// Minimum wait time
    pub min_wait_time_ms: u64,
    /// Queue depth over time
    pub avg_queue_depth: f64,
}

/// Data processing metrics
#[derive(Debug, Clone)]
pub struct ProcessingMetrics {
    /// Total rows processed
    pub total_rows_processed: u64,
    /// Rows processed per worker
    pub rows_per_worker: Vec<u64>,
    /// Processing throughput in rows/second
    pub processing_throughput_rows_per_sec: f64,
    /// Data volume processed in MB
    pub data_volume_mb: f64,
    /// Vectorization efficiency (0.0 to 1.0)
    pub vectorization_efficiency: f64,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Branch misprediction rate
    pub branch_misprediction_rate: f64,
}

/// Error metrics
#[derive(Debug, Clone)]
pub struct ErrorMetrics {
    /// Total errors encountered
    pub total_errors: u64,
    /// Workers that encountered errors
    pub workers_with_errors: usize,
    /// Error types and counts
    pub error_types: HashMap<String, u64>,
    /// Recovery actions taken
    pub recovery_actions: u64,
    /// Time spent in error handling
    pub error_handling_time_ms: u64,
}

/// Metrics collector for parallel execution
pub struct MetricCollector {
    /// Current execution metrics
    current_metrics: Arc<Mutex<Option<ParallelExecutionMetrics>>>,
    /// Historical metrics for analysis
    historical_metrics: Arc<Mutex<Vec<ParallelExecutionMetrics>>>,
    /// Real-time metric updates
    realtime_updates: Arc<Mutex<Vec<RealtimeMetricUpdate>>>,
    /// Aggregated statistics over time
    aggregated_stats: Arc<Mutex<AggregatedStatistics>>,
    /// Metric collection configuration
    config: MetricsConfig,
}

/// Real-time metric update
#[derive(Debug, Clone)]
pub struct RealtimeMetricUpdate {
    /// Timestamp of the update
    pub timestamp: Instant,
    /// Metric type
    pub metric_type: MetricType,
    /// Metric value
    pub value: MetricValue,
    /// Worker ID (if applicable)
    pub worker_id: Option<usize>,
    /// Task ID (if applicable)
    pub task_id: Option<u64>,
}

/// Types of metrics that can be collected
#[derive(Debug, Clone, PartialEq)]
pub enum MetricType {
    /// CPU utilization
    CpuUtilization,
    /// Memory usage
    MemoryUsage,
    /// I/O operation
    IoOperation,
    /// Task completion
    TaskCompletion,
    /// Error occurrence
    Error,
    /// Queue depth
    QueueDepth,
    /// Cache hit/miss
    CacheHit,
    /// Load imbalance
    LoadImbalance,
}

/// Metric value variants
#[derive(Debug, Clone)]
pub enum MetricValue {
    /// Integer value
    Integer(i64),
    /// Floating point value
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// String value
    String(String),
    /// Duration value
    Duration(Duration),
}

/// Aggregated statistics over multiple executions
#[derive(Debug, Clone)]
pub struct AggregatedStatistics {
    /// Total parallel queries executed
    pub total_queries: u64,
    /// Average number of workers per query
    pub avg_workers_per_query: f64,
    /// Average execution time
    pub avg_execution_time_ms: f64,
    /// Average speedup factor
    pub avg_speedup_factor: f64,
    /// Success rate
    pub success_rate: f64,
    /// Performance trends over time
    pub performance_trend: PerformanceTrend,
}

/// Performance trend analysis
#[derive(Debug, Clone)]
pub enum PerformanceTrend {
    /// Performance improving
    Improving,
    /// Performance degrading
    Degrading,
    /// Performance stable
    Stable,
    /// Insufficient data
    InsufficientData,
}

/// Metrics collection configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Enable real-time collection
    pub enable_realtime: bool,
    /// Collection interval in milliseconds
    pub collection_interval_ms: u64,
    /// Maximum historical metrics to keep
    pub max_historical_metrics: usize,
    /// Enable detailed worker metrics
    pub enable_worker_details: bool,
    /// Export metrics to external systems
    pub enable_export: bool,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enable_realtime: true,
            collection_interval_ms: 1000, // 1 second
            max_historical_metrics: 1000,
            enable_worker_details: true,
            enable_export: false,
        }
    }
}

impl MetricCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            current_metrics: Arc::new(Mutex::new(None)),
            historical_metrics: Arc::new(Mutex::new(Vec::new())),
            realtime_updates: Arc::new(Mutex::new(Vec::new())),
            aggregated_stats: Arc::new(Mutex::new(AggregatedStatistics {
                total_queries: 0,
                avg_workers_per_query: 0.0,
                avg_execution_time_ms: 0.0,
                avg_speedup_factor: 0.0,
                success_rate: 0.0,
                performance_trend: PerformanceTrend::InsufficientData,
            })),
            config,
        }
    }

    /// Start collecting metrics for a new query
    pub fn start_collection(&self, query_id: String) {
        let metrics = ParallelExecutionMetrics {
            query_id,
            start_time: Instant::now(),
            end_time: None,
            total_execution_time_ms: 0,
            workers_used: 0,
            cpu_metrics: CpuMetrics {
                utilization: 0.0,
                avg_worker_utilization: 0.0,
                max_worker_utilization: 0.0,
                min_worker_utilization: 0.0,
                system_cpu_time_ms: 0,
                user_cpu_time_ms: 0,
                context_switches: 0,
            },
            memory_metrics: MemoryMetrics {
                peak_memory_usage: 0,
                avg_memory_usage: 0,
                memory_per_worker: Vec::new(),
                deallocations: 0,
                allocation_failures: 0,
                gc_stats: None,
            },
            io_metrics: IoMetrics {
                total_io_operations: 0,
                read_operations: 0,
                write_operations: 0,
                bytes_read: 0,
                bytes_written: 0,
                avg_io_latency_us: 0.0,
                max_io_latency_us: 0,
                io_throughput_mbps: 0.0,
                io_wait_time_ms: 0,
            },
            load_balance_metrics: LoadBalanceMetrics {
                load_imbalance_score: 0.0,
                work_distribution_efficiency: 0.0,
                task_steals: 0,
                avg_worker_idle_time_percent: 0.0,
                work_stealing_time_ms: 0,
            },
            scheduling_metrics: SchedulingMetrics {
                total_tasks_scheduled: 0,
                tasks_completed: 0,
                tasks_failed: 0,
                avg_task_execution_time_ms: 0.0,
                max_task_execution_time_ms: 0,
                min_task_execution_time_ms: u64::MAX,
                queue_wait_time: QueueWaitTime {
                    avg_wait_time_ms: 0.0,
                    max_wait_time_ms: 0,
                    min_wait_time_ms: u64::MAX,
                    avg_queue_depth: 0.0,
                },
            },
            processing_metrics: ProcessingMetrics {
                total_rows_processed: 0,
                rows_per_worker: Vec::new(),
                processing_throughput_rows_per_sec: 0.0,
                data_volume_mb: 0.0,
                vectorization_efficiency: 0.0,
                cache_hit_rate: 0.0,
                branch_misprediction_rate: 0.0,
            },
            error_metrics: ErrorMetrics {
                total_errors: 0,
                workers_with_errors: 0,
                error_types: HashMap::new(),
                recovery_actions: 0,
                error_handling_time_ms: 0,
            },
        };

        *self.current_metrics.lock().unwrap() = Some(metrics);
    }

    /// Finish collecting metrics for current query
    pub fn finish_collection(&self) -> Option<ParallelExecutionMetrics> {
        let mut current = self.current_metrics.lock().unwrap();
        if let Some(mut metrics) = current.take() {
            metrics.end_time = Some(Instant::now());
            metrics.total_execution_time_ms = metrics.start_time.elapsed().as_millis() as u64;

            // Add to historical metrics
            {
                let mut historical = self.historical_metrics.lock().unwrap();
                historical.push(metrics.clone());

                // Limit historical metrics
                if historical.len() > self.config.max_historical_metrics {
                    historical.remove(0);
                }
            }

            // Update aggregated statistics
            self.update_aggregated_stats(&metrics);

            Some(metrics)
        } else {
            None
        }
    }

    /// Record a real-time metric update
    pub fn record_realtime_metric(&self, metric_type: MetricType, value: MetricValue, worker_id: Option<usize>, task_id: Option<u64>) {
        if !self.config.enable_realtime {
            return;
        }

        let update = RealtimeMetricUpdate {
            timestamp: Instant::now(),
            metric_type,
            value,
            worker_id,
            task_id,
        };

        {
            let mut realtime = self.realtime_updates.lock().unwrap();
            realtime.push(update);

            // Keep only recent updates (last hour)
            let cutoff = Instant::now() - Duration::from_secs(3600);
            realtime.retain(|u| u.timestamp > cutoff);
        }
    }

    /// Update metrics from worker statistics
    pub fn update_from_worker_stats(&self, worker_stats: &AggregateWorkerStats) {
        let mut current = self.current_metrics.lock().unwrap();
        if let Some(ref mut metrics) = *current {
            metrics.workers_used = worker_stats.worker_count;

            // Update processing metrics
            metrics.processing_metrics.total_rows_processed = worker_stats.total_rows;

            // Calculate throughput
            let elapsed_seconds = metrics.start_time.elapsed().as_secs_f64();
            if elapsed_seconds > 0.0 {
                metrics.processing_metrics.processing_throughput_rows_per_sec =
                    worker_stats.total_rows as f64 / elapsed_seconds;
            }

            // Update scheduling metrics
            metrics.scheduling_metrics.tasks_completed = worker_stats.total_tasks;

            // Update memory metrics
            metrics.memory_metrics.peak_memory_usage = worker_stats.total_memory_used;
        }
    }

    /// Update load balancing metrics
    pub fn update_load_balance_metrics(&self, load_imbalance_score: f64, task_steals: u64, idle_time_percent: f64) {
        let mut current = self.current_metrics.lock().unwrap();
        if let Some(ref mut metrics) = *current {
            metrics.load_balance_metrics.load_imbalance_score = load_imbalance_score;
            metrics.load_balance_metrics.task_steals = task_steals;
            metrics.load_balance_metrics.avg_worker_idle_time_percent = idle_time_percent;
        }
    }

    /// Update I/O metrics
    pub fn update_io_metrics(&self, read_ops: u64, write_ops: u64, bytes_read: u64, bytes_written: u64, avg_latency_us: f64) {
        let mut current = self.current_metrics.lock().unwrap();
        if let Some(ref mut metrics) = *current {
            metrics.io_metrics.read_operations += read_ops;
            metrics.io_metrics.write_operations += write_ops;
            metrics.io_metrics.bytes_read += bytes_read;
            metrics.io_metrics.bytes_written += bytes_written;
            metrics.io_metrics.avg_io_latency_us = avg_latency_us;
            metrics.io_metrics.max_io_latency_us = metrics.io_metrics.max_io_latency_us.max(avg_latency_us as u64);
            metrics.io_metrics.total_io_operations = metrics.io_metrics.read_operations + metrics.io_metrics.write_operations;
        }
    }

    /// Record an error
    pub fn record_error(&self, error_type: &str, worker_id: usize) {
        let mut current = self.current_metrics.lock().unwrap();
        if let Some(ref mut metrics) = *current {
            metrics.error_metrics.total_errors += 1;
            *metrics.error_metrics.error_types.entry(error_type.to_string()).or_insert(0) += 1;

            // Update workers with errors count
            let workers_with_errors = metrics.error_metrics.workers_with_errors;
            metrics.error_metrics.workers_with_errors = workers_with_errors.max(worker_id + 1);
        }
    }

    /// Get current metrics
    pub fn get_current_metrics(&self) -> Option<ParallelExecutionMetrics> {
        self.current_metrics.lock().unwrap().clone()
    }

    /// Get historical metrics
    pub fn get_historical_metrics(&self, limit: Option<usize>) -> Vec<ParallelExecutionMetrics> {
        let historical = self.historical_metrics.lock().unwrap();
        if let Some(limit) = limit {
            historical.iter().rev().take(limit).cloned().collect()
        } else {
            historical.clone()
        }
    }

    /// Get real-time metric updates
    pub fn get_realtime_updates(&self, since: Option<Instant>) -> Vec<RealtimeMetricUpdate> {
        let realtime = self.realtime_updates.lock().unwrap();
        if let Some(since) = since {
            realtime.iter()
                .filter(|u| u.timestamp > since)
                .cloned()
                .collect()
        } else {
            realtime.clone()
        }
    }

    /// Get aggregated statistics
    pub fn get_aggregated_stats(&self) -> AggregatedStatistics {
        self.aggregated_stats.lock().unwrap().clone()
    }

    /// Export metrics in various formats
    pub fn export_metrics(&self, format: ExportFormat) -> Result<String> {
        let current = self.get_current_metrics().ok_or_else(|| {
            crate::error::RustgreSQLError::Internal("No metrics available for export".to_string())
        })?;

        match format {
            ExportFormat::Json => self.export_json(&current),
            ExportFormat::Csv => self.export_csv(&current),
            ExportFormat::Prometheus => self.export_prometheus(&current),
        }
    }

    /// Clear all metrics
    pub fn clear_metrics(&self) {
        *self.current_metrics.lock().unwrap() = None;
        self.historical_metrics.lock().unwrap().clear();
        self.realtime_updates.lock().unwrap().clear();
    }

    /// Update aggregated statistics
    fn update_aggregated_stats(&self, metrics: &ParallelExecutionMetrics) {
        let mut stats = self.aggregated_stats.lock().unwrap();

        stats.total_queries += 1;

        // Update running averages
        let total = stats.total_queries as f64;
        stats.avg_workers_per_query =
            (stats.avg_workers_per_query * (total - 1.0) + metrics.workers_used as f64) / total;

        stats.avg_execution_time_ms =
            (stats.avg_execution_time_ms * (total - 1.0) + metrics.total_execution_time_ms as f64) / total;

        // Update success rate (simple heuristic - no errors = success)
        let success = if metrics.error_metrics.total_errors == 0 { 1.0 } else { 0.0 };
        stats.success_rate = (stats.success_rate * (total - 1.0) + success) / total;

        // Update performance trend (simplified)
        stats.performance_trend = self.calculate_performance_trend();
    }

    /// Calculate performance trend
    fn calculate_performance趋势(&self) -> PerformanceTrend {
        let historical = self.historical_metrics.lock().unwrap();
        if historical.len() < 5 {
            return PerformanceTrend::InsufficientData;
        }

        // Simple trend analysis based on recent execution times
        let recent_times: Vec<f64> = historical.iter()
            .rev()
            .take(10)
            .map(|m| m.total_execution_time_ms as f64)
            .collect();

        if recent_times.len() < 3 {
            return PerformanceTrend::InsufficientData;
        }

        // Calculate trend slope (simplified linear regression)
        let n = recent_times.len() as f64;
        let sum_x: f64 = (0..recent_times.len()).map(|i| i as f64).sum();
        let sum_y: f64 = recent_times.iter().sum();
        let sum_xy: f64 = recent_times.iter().enumerate()
            .map(|(i, &y)| i as f64 * y)
            .sum();
        let sum_x2: f64 = (0..recent_times.len()).map(|i| (i as f64).powi(2)).sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2));

        if slope.abs() < 1.0 {
            PerformanceTrend::Stable
        } else if slope > 0.0 {
            PerformanceTrend::Degrading
        } else {
            PerformanceTrend::Improving
        }
    }
}

/// Export format options
#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    Json,
    Csv,
    Prometheus,
}

impl MetricCollector {
    fn export_json(&self, metrics: &ParallelExecutionMetrics) -> Result<String> {
        serde_json::to_string_pretty(metrics).map_err(|e| {
            crate::error::RustgreSQLError::Internal(format!("JSON export failed: {}", e))
        })
    }

    fn export_csv(&self, metrics: &ParallelExecutionMetrics) -> Result<String> {
        let mut csv = String::new();
        csv.push_str("metric,value\n");
        csv.push_str(&format!("query_id,{}\n", metrics.query_id));
        csv.push_str(&format!("total_execution_time_ms,{}\n", metrics.total_execution_time_ms));
        csv.push_str(&format!("workers_used,{}\n", metrics.workers_used));
        csv.push_str(&format!("cpu_utilization,{}\n", metrics.cpu_metrics.utilization));
        csv.push_str(&format!("peak_memory_usage,{}\n", metrics.memory_metrics.peak_memory_usage));
        csv.push_str(&format!("total_rows_processed,{}\n", metrics.processing_metrics.total_rows_processed));
        csv.push_str(&format!("total_io_operations,{}\n", metrics.io_metrics.total_io_operations));
        csv.push_str(&format!("load_imbalance_score,{}\n", metrics.load_balance_metrics.load_imbalance_score));
        csv.push_str(&format!("total_errors,{}\n", metrics.error_metrics.total_errors));
        Ok(csv)
    }

    fn export_prometheus(&self, metrics: &ParallelExecutionMetrics) -> Result<String> {
        let mut output = String::new();

        output.push_str("# HELP rustgresql_parallel_execution_duration_seconds Total execution duration\n");
        output.push_str("# TYPE rustgresql_parallel_execution_duration_seconds gauge\n");
        output.push_str(&format!("rustgresql_parallel_execution_duration_seconds {{query_id=\"{}\"}} {}\n",
            metrics.query_id, metrics.total_execution_time_ms as f64 / 1000.0));

        output.push_str("# HELP rustgresql_parallel_workers_count Number of parallel workers\n");
        output.push_str("# TYPE rustgresql_parallel_workers_count gauge\n");
        output.push_str(&format!("rustgresql_parallel_workers_count {{query_id=\"{}\"}} {}\n",
            metrics.query_id, metrics.workers_used));

        output.push_str("# HELP rustgresql_parallel_cpu_utilization CPU utilization ratio\n");
        output.push_str("# TYPE rustgresql_parallel_cpu_utilization gauge\n");
        output.push_str(&format!("rustgresql_parallel_cpu_utilization {{query_id=\"{}\"}} {}\n",
            metrics.query_id, metrics.cpu_metrics.utilization));

        output.push_str("# HELP rustgresql_parallel_memory_bytes Memory usage in bytes\n");
        output.push_str("# TYPE rustgresql_parallel_memory_bytes gauge\n");
        output.push_str(&format!("rustgresql_parallel_memory_bytes {{query_id=\"{}\"}} {}\n",
            metrics.query_id, metrics.memory_metrics.peak_memory_usage));

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let config = MetricsConfig::default();
        let collector = MetricCollector::new(config);

        collector.start_collection("test_query_1".to_string());

        // Record some metrics
        collector.record_realtime_metric(
            MetricType::CpuUtilization,
            MetricValue::Float(0.75),
            Some(1),
            Some(100),
        );

        collector.record_error("TestError", 1);

        // Finish collection
        let metrics = collector.finish_collection();
        assert!(metrics.is_some());

        let metrics = metrics.unwrap();
        assert_eq!(metrics.query_id, "test_query_1");
        assert_eq!(metrics.error_metrics.total_errors, 1);
        assert!(metrics.total_execution_time_ms > 0);
    }

    #[test]
    fn test_worker_stats_update() {
        let config = MetricsConfig::default();
        let collector = MetricCollector::new(config);

        collector.start_collection("test_query_2".to_string());

        let worker_stats = AggregateWorkerStats {
            total_tasks: 10,
            total_rows: 10000,
            total_execution_time_ms: 5000,
            total_memory_used: 1024 * 1024,
            total_io_operations: 100,
            active_workers: 2,
            worker_count: 2,
        };

        collector.update_from_worker_stats(&worker_stats);

        let metrics = collector.get_current_metrics();
        assert!(metrics.is_some());

        let metrics = metrics.unwrap();
        assert_eq!(metrics.workers_used, 2);
        assert_eq!(metrics.processing_metrics.total_rows_processed, 10000);
        assert_eq!(metrics.memory_metrics.peak_memory_usage, 1024 * 1024);
    }

    #[test]
    fn test_metrics_export() {
        let config = MetricsConfig::default();
        let collector = MetricCollector::new(config);

        collector.start_collection("export_test".to_string());
        let metrics = collector.finish_collection();
        assert!(metrics.is_some());

        let json_export = collector.export_metrics(ExportFormat::Json);
        assert!(json_export.is_ok());

        let csv_export = collector.export_metrics(ExportFormat::Csv);
        assert!(csv_export.is_ok());

        let prometheus_export = collector.export_metrics(ExportFormat::Prometheus);
        assert!(prometheus_export.is_ok());
    }

    #[test]
    fn test_aggregated_statistics() {
        let config = MetricsConfig::default();
        let collector = MetricCollector::new(config);

        collector.start_collection("query_1".to_string());
        collector.finish_collection();

        collector.start_collection("query_2".to_string());
        collector.finish_collection();

        let stats = collector.get_aggregated_stats();
        assert_eq!(stats.total_queries, 2);
        assert_eq!(stats.success_rate, 1.0); // Both queries had no errors
    }
}