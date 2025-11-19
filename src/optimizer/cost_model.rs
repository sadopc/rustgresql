//! Cost model for query optimization
//!
//! Provides cost estimation for different query plan operations
//! based on I/O, CPU, and memory costs.

use crate::{Result, sql::ast::Expression};
use std::collections::HashMap;

/// Cost estimate for a query operation
#[derive(Debug, Clone, Copy)]
pub struct CostEstimate {
    /// I/O cost (disk reads/writes)
    pub io_cost: f64,
    /// CPU cost (computation, comparisons, etc.)
    pub cpu_cost: f64,
    /// Memory cost (buffer usage)
    pub memory_cost: f64,
    /// Total cost (weighted sum of all costs)
    pub total_cost: f64,
    /// Parallel execution cost (coordination overhead)
    pub parallel_cost: f64,
    /// Number of workers for parallel execution
    pub parallel_workers: usize,
}

/// Parallel execution cost estimate
#[derive(Debug, Clone, Copy)]
pub struct ParallelCostEstimate {
    /// Base sequential cost estimate
    pub sequential_cost: CostEstimate,
    /// Parallel cost estimate
    pub parallel_cost: CostEstimate,
    /// Speedup factor (sequential_time / parallel_time)
    pub speedup: f64,
    /// Parallel efficiency (speedup / num_workers)
    pub efficiency: f64,
    /// Optimal number of workers
    pub optimal_workers: usize,
}

impl CostEstimate {
    /// Create a new cost estimate
    pub fn new(io_cost: f64, cpu_cost: f64, memory_cost: f64) -> Self {
        let total_cost = io_cost + cpu_cost * 0.1 + memory_cost * 0.01; // Standard PostgreSQL weights
        Self {
            io_cost,
            cpu_cost,
            memory_cost,
            total_cost,
            parallel_cost: 0.0,
            parallel_workers: 1,
        }
    }

    /// Create a new cost estimate with parallel information
    pub fn with_parallel(io_cost: f64, cpu_cost: f64, memory_cost: f64, parallel_cost: f64, parallel_workers: usize) -> Self {
        let total_cost = io_cost + cpu_cost * 0.1 + memory_cost * 0.01 + parallel_cost;
        Self {
            io_cost,
            cpu_cost,
            memory_cost,
            total_cost,
            parallel_cost,
            parallel_workers,
        }
    }

    /// Create a zero cost estimate
    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Add another cost estimate to this one
    pub fn add(&self, other: &CostEstimate) -> CostEstimate {
        CostEstimate::with_parallel(
            self.io_cost + other.io_cost,
            self.cpu_cost + other.cpu_cost,
            self.memory_cost + other.memory_cost,
            self.parallel_cost + other.parallel_cost,
            std::cmp::max(self.parallel_workers, other.parallel_workers),
        )
    }

    /// Multiply this cost estimate by a factor
    pub fn multiply(&self, factor: f64) -> CostEstimate {
        CostEstimate::with_parallel(
            self.io_cost * factor,
            self.cpu_cost * factor,
            self.memory_cost * factor,
            self.parallel_cost * factor,
            self.parallel_workers,
        )
    }

    /// Convert to parallel cost estimate with given number of workers
    pub fn to_parallel(&self, workers: usize, parallel_overhead: f64) -> ParallelCostEstimate {
        let parallel_workers = workers.max(1);
        let speedup = self.estimate_speedup(parallel_workers);
        let parallel_cost = self.multiply(1.0 / speedup);

        // Add parallel coordination overhead
        let parallel_cost_with_overhead = CostEstimate::with_parallel(
            parallel_cost.io_cost,
            parallel_cost.cpu_cost,
            parallel_cost.memory_cost,
            parallel_overhead,
            parallel_workers,
        );

        ParallelCostEstimate {
            sequential_cost: *self,
            parallel_cost: parallel_cost_with_overhead,
            speedup,
            efficiency: speedup / parallel_workers as f64,
            optimal_workers: workers,
        }
    }

    /// Estimate speedup for given number of workers using Amdahl's Law
    pub fn estimate_speedup(&self, workers: usize) -> f64 {
        if workers <= 1 {
            return 1.0;
        }

        // Assume 80% of the work can be parallelized (configurable in real system)
        let parallel_fraction = 0.8;
        let serial_fraction = 1.0 - parallel_fraction;

        // Amdahl's Law: Speedup = 1 / (serial_fraction + parallel_fraction / workers)
        let speedup = 1.0 / (serial_fraction + parallel_fraction / workers as f64);

        // Cap speedup at theoretical maximum
        speedup.min(workers as f64)
    }
}

impl PartialEq for CostEstimate {
    fn eq(&self, other: &Self) -> bool {
        (self.total_cost - other.total_cost).abs() < f64::EPSILON
    }
}

impl PartialOrd for CostEstimate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.total_cost.partial_cmp(&other.total_cost)
    }
}

impl Eq for CostEstimate {}

impl Ord for CostEstimate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.total_cost.partial_cmp(&other.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Cost model configuration
#[derive(Debug, Clone)]
pub struct CostModelConfig {
    /// Cost per sequential page read
    pub seq_page_cost: f64,
    /// Cost per random page read
    pub random_page_cost: f64,
    /// Cost per CPU operation
    pub cpu_tuple_cost: f64,
    /// Cost per index operation
    pub cpu_index_tuple_cost: f64,
    /// Cost per operator (function call, etc.)
    pub cpu_operator_cost: f64,
    /// Buffer pool size in pages
    pub effective_cache_size: usize,
    /// Parallel execution configuration
    pub parallel_config: ParallelCostConfig,
}

/// Parallel execution cost configuration
#[derive(Debug, Clone)]
pub struct ParallelCostConfig {
    /// Base parallel startup overhead
    pub parallel_startup_cost: f64,
    /// Cost per additional worker
    pub parallel_worker_cost: f64,
    /// Communication overhead per tuple
    pub tuple_comm_cost: f64,
    /// Maximum parallel workers per operation
    pub max_parallel_workers: usize,
    /// Minimum table size for parallel execution
    pub min_parallel_table_size: usize,
    /// Parallel efficiency threshold (below this, don't use parallel)
    pub min_parallel_efficiency: f64,
    /// Memory overhead per worker
    pub parallel_memory_per_worker: f64,
}

impl Default for CostModelConfig {
    fn default() -> Self {
        // PostgreSQL-like default values
        Self {
            seq_page_cost: 1.0,
            random_page_cost: 4.0,  // Random access is typically 4x slower
            cpu_tuple_cost: 0.01,
            cpu_index_tuple_cost: 0.005,
            cpu_operator_cost: 0.0025,
            effective_cache_size: 16384, // 128MB with 8KB pages
            parallel_config: ParallelCostConfig::default(),
        }
    }
}

impl Default for ParallelCostConfig {
    fn default() -> Self {
        Self {
            parallel_startup_cost: 1000.0,    // Base overhead for parallel setup
            parallel_worker_cost: 100.0,      // Cost per additional worker
            tuple_comm_cost: 0.01,            // Communication overhead per tuple
            max_parallel_workers: num_cpus::get(),
            min_parallel_table_size: 1000,    // Minimum rows to consider parallel
            min_parallel_efficiency: 0.25,    // Minimum 25% efficiency
            parallel_memory_per_worker: 16.0 * 1024.0, // 16MB per worker
        }
    }
}

/// Cost model for estimating query operation costs
#[derive(Debug, Clone)]
pub struct CostModel {
    config: CostModelConfig,
}

impl CostModel {
    /// Create a new cost model with default configuration
    pub fn new() -> Self {
        Self {
            config: CostModelConfig::default(),
        }
    }

    /// Create a new cost model with custom configuration
    pub fn with_config(config: CostModelConfig) -> Self {
        Self { config }
    }

    /// Estimate cost for sequential table scan
    pub fn estimate_seq_scan(&self, num_pages: usize, num_tuples: usize) -> CostEstimate {
        let io_cost = num_pages as f64 * self.config.seq_page_cost;
        let cpu_cost = num_tuples as f64 * self.config.cpu_tuple_cost;
        let memory_cost = 0.0; // Sequential scan uses minimal additional memory

        CostEstimate::new(io_cost, cpu_cost, memory_cost)
    }

    /// Estimate cost for index scan
    pub fn estimate_index_scan(
        &self,
        index_pages: usize,
        heap_pages: usize,
        index_tuples: usize,
        heap_tuples: usize,
    ) -> CostEstimate {
        // Cost to traverse index + fetch heap pages
        let io_cost = (index_pages as f64 * self.config.random_page_cost) +
                     (heap_pages as f64 * self.config.random_page_cost);
        let cpu_cost = (index_tuples as f64 * self.config.cpu_index_tuple_cost) +
                      (heap_tuples as f64 * self.config.cpu_tuple_cost);
        let memory_cost = 0.0;

        CostEstimate::new(io_cost, cpu_cost, memory_cost)
    }

    /// Estimate cost for index-only scan (covering index)
    pub fn estimate_index_only_scan(
        &self,
        index_pages: usize,
        index_tuples: usize,
    ) -> CostEstimate {
        let io_cost = index_pages as f64 * self.config.random_page_cost;
        let cpu_cost = index_tuples as f64 * self.config.cpu_index_tuple_cost;
        let memory_cost = 0.0;

        CostEstimate::new(io_cost, cpu_cost, memory_cost)
    }

    /// Estimate cost for nested loop join
    pub fn estimate_nested_loop_join(
        &self,
        outer_tuples: usize,
        inner_cost: CostEstimate,
        join_selectivity: f64,
    ) -> CostEstimate {
        let result_tuples = (outer_tuples as f64 * join_selectivity) as usize;
        let inner_repetitions = outer_tuples;

        // Repeat inner scan for each outer tuple
        let repeated_inner_cost = inner_cost.multiply(inner_repetitions as f64);
        let join_cpu_cost = result_tuples as f64 * self.config.cpu_tuple_cost;

        CostEstimate::new(
            repeated_inner_cost.io_cost,
            repeated_inner_cost.cpu_cost + join_cpu_cost,
            repeated_inner_cost.memory_cost,
        )
    }

    /// Estimate cost for hash join
    pub fn estimate_hash_join(
        &self,
        outer_tuples: usize,
        inner_tuples: usize,
        outer_cost: CostEstimate,
        inner_cost: CostEstimate,
        join_selectivity: f64,
    ) -> CostEstimate {
        let result_tuples = ((outer_tuples + inner_tuples) as f64 * join_selectivity) as usize;

        // Cost to build hash table (inner relation)
        let hash_build_cost = inner_cost.add(&CostEstimate::new(
            0.0,
            inner_tuples as f64 * self.config.cpu_tuple_cost, // Hash computation
            inner_tuples as f64 * 64.0, // Rough estimate: 64 bytes per hash entry
        ));

        // Cost to probe hash table (outer relation)
        let hash_probe_cost = outer_cost.add(&CostEstimate::new(
            0.0,
            outer_tuples as f64 * self.config.cpu_tuple_cost, // Hash lookup
            0.0,
        ));

        // Join result processing
        let result_cost = CostEstimate::new(
            0.0,
            result_tuples as f64 * self.config.cpu_tuple_cost,
            0.0,
        );

        let total_cost = hash_build_cost.add(&hash_probe_cost).add(&result_cost);
        total_cost
    }

    /// Estimate cost for merge join
    pub fn estimate_merge_join(
        &self,
        outer_tuples: usize,
        inner_tuples: usize,
        outer_cost: CostEstimate,
        inner_cost: CostEstimate,
        join_selectivity: f64,
    ) -> CostEstimate {
        let result_tuples = ((outer_tuples + inner_tuples) as f64 * join_selectivity) as usize;

        // Assume both inputs are already sorted (or we need to sort them)
        // For now, assume no sorting cost
        let merge_cpu_cost = (outer_tuples + inner_tuples) as f64 * self.config.cpu_tuple_cost;
        let result_cost = result_tuples as f64 * self.config.cpu_tuple_cost;

        CostEstimate::new(
            outer_cost.io_cost + inner_cost.io_cost,
            merge_cpu_cost + result_cost,
            0.0,
        )
    }

    /// Estimate cost for filter operation
    pub fn estimate_filter(&self, input_cost: CostEstimate, input_tuples: usize, condition_cost: f64) -> CostEstimate {
        let filter_cpu_cost = input_tuples as f64 * (self.config.cpu_tuple_cost + condition_cost);

        CostEstimate::new(
            input_cost.io_cost,
            input_cost.cpu_cost + filter_cpu_cost,
            input_cost.memory_cost,
        )
    }

    /// Estimate cost for projection operation
    pub fn estimate_projection(&self, input_cost: CostEstimate, input_tuples: usize, num_columns: usize) -> CostEstimate {
        let projection_cpu_cost = input_tuples as f64 * num_columns as f64 * self.config.cpu_tuple_cost;

        CostEstimate::new(
            input_cost.io_cost,
            input_cost.cpu_cost + projection_cpu_cost,
            input_cost.memory_cost,
        )
    }

    /// Estimate cost for sorting
    pub fn estimate_sort(&self, input_tuples: usize, input_pages: usize) -> CostEstimate {
        // Simplified cost model based on quicksort: O(n log n)
        let sort_cpu_cost = input_tuples as f64 * (input_tuples as f64).log2() * self.config.cpu_tuple_cost;

        // I/O cost for external sort if needed
        let io_cost = if input_pages > self.config.effective_cache_size {
            // Need external sort - multiple passes over data
            input_pages as f64 * self.config.seq_page_cost * 2.0 // Rough estimate: 2 passes
        } else {
            0.0 // Fits in memory
        };

        let memory_cost = input_tuples as f64 * 100.0; // Rough estimate: 100 bytes per sort entry

        CostEstimate::new(io_cost, sort_cpu_cost, memory_cost)
    }

    /// Estimate cost for aggregation (GROUP BY)
    pub fn estimate_aggregation(
        &self,
        input_cost: CostEstimate,
        input_tuples: usize,
        num_groups: usize,
        aggregate_functions: usize,
    ) -> CostEstimate {
        // Cost to group tuples
        let grouping_cost = input_tuples as f64 * self.config.cpu_tuple_cost;
        // Cost to evaluate aggregate functions
        let aggregation_cost = num_groups as f64 * aggregate_functions as f64 * self.config.cpu_operator_cost;

        let memory_cost = num_groups as f64 * 200.0; // Rough estimate: 200 bytes per group

        CostEstimate::new(
            input_cost.io_cost,
            input_cost.cpu_cost + grouping_cost + aggregation_cost,
            input_cost.memory_cost + memory_cost,
        )
    }

    /// Estimate selectivity of a predicate (simplified)
    pub fn estimate_selectivity(&self, predicate: &Expression) -> f64 {
        // This is a very simplified selectivity estimation
        // In a real database, this would use histograms and statistics
        match predicate {
            Expression::BinaryOp { op, .. } => {
                match op {
                    crate::sql::ast::BinaryOperator::Equals => 0.1,  // Equality predicates are usually selective
                    crate::sql::ast::BinaryOperator::NotEquals => 0.9,
                    crate::sql::ast::BinaryOperator::LessThan => 0.33,
                    crate::sql::ast::BinaryOperator::LessThanOrEquals => 0.33,
                    crate::sql::ast::BinaryOperator::GreaterThan => 0.33,
                    crate::sql::ast::BinaryOperator::GreaterThanOrEquals => 0.33,
                    crate::sql::ast::BinaryOperator::Like => 0.1, // LIKE with patterns can be selective
                    _ => 0.5, // Default selectivity
                }
            }
            Expression::UnaryOp { op, .. } => {
                match op {
                    crate::sql::ast::UnaryOperator::Not => 0.5, // NOT expression
                    _ => 0.5,
                }
            }
            _ => 0.5, // Default selectivity for complex predicates
        }
    }

    /// Estimate cost for parallel sequential scan
    pub fn estimate_parallel_seq_scan(&self, num_pages: usize, num_tuples: usize) -> ParallelCostEstimate {
        let sequential_cost = self.estimate_seq_scan(num_pages, num_tuples);

        // Don't use parallel for small tables
        if num_tuples < self.config.parallel_config.min_parallel_table_size {
            return ParallelCostEstimate {
                sequential_cost,
                parallel_cost: sequential_cost,
                speedup: 1.0,
                efficiency: 1.0,
                optimal_workers: 1,
            };
        }

        // Calculate optimal number of workers
        let optimal_workers = self.calculate_optimal_workers(num_tuples);
        let parallel_overhead = self.calculate_parallel_overhead(optimal_workers, num_tuples);

        sequential_cost.to_parallel(optimal_workers, parallel_overhead)
    }

    /// Estimate cost for parallel hash join
    pub fn estimate_parallel_hash_join(
        &self,
        outer_tuples: usize,
        inner_tuples: usize,
        outer_cost: CostEstimate,
        inner_cost: CostEstimate,
        join_selectivity: f64,
    ) -> ParallelCostEstimate {
        let sequential_cost = self.estimate_hash_join(
            outer_tuples, inner_tuples, outer_cost, inner_cost, join_selectivity
        );

        // Don't use parallel for small joins
        let total_tuples = outer_tuples + inner_tuples;
        if total_tuples < self.config.parallel_config.min_parallel_table_size {
            return ParallelCostEstimate {
                sequential_cost,
                parallel_cost: sequential_cost,
                speedup: 1.0,
                efficiency: 1.0,
                optimal_workers: 1,
            };
        }

        // Hash joins benefit significantly from parallelism
        let optimal_workers = self.calculate_optimal_workers(total_tuples);
        let parallel_overhead = self.calculate_parallel_overhead(optimal_workers, total_tuples);

        // Hash join has better parallel efficiency than most operations
        let mut parallel_estimate = sequential_cost.to_parallel(optimal_workers, parallel_overhead);
        parallel_estimate.efficiency *= 1.2; // 20% bonus for hash join parallelism
        parallel_estimate.efficiency = parallel_estimate.efficiency.min(1.0);

        parallel_estimate
    }

    /// Estimate cost for parallel aggregation
    pub fn estimate_parallel_aggregation(
        &self,
        input_cost: CostEstimate,
        input_tuples: usize,
        num_groups: usize,
        aggregate_functions: usize,
    ) -> ParallelCostEstimate {
        let sequential_cost = self.estimate_aggregation(
            input_cost, input_tuples, num_groups, aggregate_functions
        );

        // Don't use parallel for small aggregations
        if input_tuples < self.config.parallel_config.min_parallel_table_size {
            return ParallelCostEstimate {
                sequential_cost,
                parallel_cost: sequential_cost,
                speedup: 1.0,
                efficiency: 1.0,
                optimal_workers: 1,
            };
        }

        // Aggregations can benefit from parallelism if there are many groups
        let optimal_workers = self.calculate_optimal_workers_for_aggregation(input_tuples, num_groups);
        let parallel_overhead = self.calculate_parallel_overhead(optimal_workers, input_tuples);

        // Aggregation parallel efficiency depends on number of groups
        let mut parallel_estimate = sequential_cost.to_parallel(optimal_workers, parallel_overhead);
        let group_factor = (num_groups as f64 / 1000.0).min(1.0); // More groups = better parallelism
        parallel_estimate.efficiency *= 0.5 + group_factor * 0.5; // Scale efficiency based on groups
        parallel_estimate.efficiency = parallel_estimate.efficiency.min(1.0);

        parallel_estimate
    }

    /// Calculate optimal number of workers for a given data size
    fn calculate_optimal_workers(&self, num_tuples: usize) -> usize {
        let max_workers = self.config.parallel_config.max_parallel_workers;

        // Scale workers based on data size
        if num_tuples < 1000 {
            return 1;
        } else if num_tuples < 10000 {
            return 2.min(max_workers);
        } else if num_tuples < 100000 {
            return (num_cpus::get() / 2).min(max_workers);
        } else {
            return max_workers;
        }
    }

    /// Calculate optimal workers for aggregation (considers group count)
    fn calculate_optimal_workers_for_aggregation(&self, num_tuples: usize, num_groups: usize) -> usize {
        let base_workers = self.calculate_optimal_workers(num_tuples);

        // Reduce workers if few groups (less parallelism opportunity)
        if num_groups < 10 {
            return 1;
        } else if num_groups < 100 {
            return base_workers / 2;
        } else {
            return base_workers;
        }
    }

    /// Calculate parallel execution overhead
    fn calculate_parallel_overhead(&self, workers: usize, num_tuples: usize) -> f64 {
        let config = &self.config.parallel_config;

        // Startup cost + worker cost + communication cost
        let startup_cost = config.parallel_startup_cost;
        let worker_cost = (workers - 1) as f64 * config.parallel_worker_cost;
        let comm_cost = num_tuples as f64 * config.tuple_comm_cost * workers as f64;

        startup_cost + worker_cost + comm_cost
    }

    /// Check if parallel execution is beneficial
    pub fn should_use_parallel(&self, parallel_estimate: &ParallelCostEstimate) -> bool {
        parallel_estimate.efficiency >= self.config.parallel_config.min_parallel_efficiency
            && parallel_estimate.optimal_workers > 1
            && parallel_estimate.parallel_cost.total_cost < parallel_estimate.sequential_cost.total_cost
    }

    /// Get the parallel configuration
    pub fn parallel_config(&self) -> &ParallelCostConfig {
        &self.config.parallel_config
    }

    /// Get configuration reference
    pub fn config(&self) -> &CostModelConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config<F>(&mut self, updater: F)
    where
        F: FnOnce(&mut CostModelConfig),
    {
        updater(&mut self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_estimate() {
        let cost = CostEstimate::new(10.0, 5.0, 2.0);
        assert_eq!(cost.io_cost, 10.0);
        assert_eq!(cost.cpu_cost, 5.0);
        assert_eq!(cost.memory_cost, 2.0);
        assert!(cost.total_cost > 10.0); // Should include weighted CPU/memory costs

        let doubled = cost.multiply(2.0);
        assert_eq!(doubled.io_cost, 20.0);
        assert_eq!(doubled.cpu_cost, 10.0);
    }

    #[test]
    fn test_cost_estimate_with_parallel() {
        let cost = CostEstimate::with_parallel(10.0, 5.0, 2.0, 1.0, 4);
        assert_eq!(cost.io_cost, 10.0);
        assert_eq!(cost.cpu_cost, 5.0);
        assert_eq!(cost.memory_cost, 2.0);
        assert_eq!(cost.parallel_cost, 1.0);
        assert_eq!(cost.parallel_workers, 4);
    }

    #[test]
    fn test_parallel_speedup_estimation() {
        let cost = CostEstimate::new(100.0, 50.0, 10.0);

        // Test speedup for different worker counts
        let speedup_1 = cost.estimate_speedup(1);
        assert_eq!(speedup_1, 1.0);

        let speedup_2 = cost.estimate_speedup(2);
        assert!(speedup_2 > 1.0);
        assert!(speedup_2 <= 2.0);

        let speedup_8 = cost.estimate_speedup(8);
        assert!(speedup_8 > 1.0);
        assert!(speedup_8 <= 8.0);
    }

    #[test]
    fn test_parallel_conversion() {
        let cost = CostEstimate::new(100.0, 50.0, 10.0);
        let parallel_estimate = cost.to_parallel(4, 50.0);

        assert_eq!(parallel_estimate.optimal_workers, 4);
        assert!(parallel_estimate.speedup > 1.0);
        assert!(parallel_estimate.efficiency > 0.0);
        assert!(parallel_estimate.efficiency <= 1.0);
    }

    #[test]
    fn test_seq_scan_cost() {
        let model = CostModel::new();
        let cost = model.estimate_seq_scan(100, 1000);
        assert!(cost.io_cost > 0.0);
        assert!(cost.cpu_cost > 0.0);
    }

    #[test]
    fn test_parallel_seq_scan_cost() {
        let model = CostModel::new();
        let parallel_cost = model.estimate_parallel_seq_scan(100, 10000); // Large table

        assert_eq!(parallel_cost.sequential_cost, model.estimate_seq_scan(100, 10000));
        assert!(parallel_cost.optimal_workers > 1);
        assert!(parallel_cost.speedup > 1.0);

        // Small table should not use parallel
        let small_parallel_cost = model.estimate_parallel_seq_scan(10, 500);
        assert_eq!(small_parallel_cost.optimal_workers, 1);
        assert_eq!(small_parallel_cost.speedup, 1.0);
    }

    #[test]
    fn test_parallel_hash_join_cost() {
        let model = CostModel::new();
        let outer_cost = model.estimate_seq_scan(100, 5000);
        let inner_cost = model.estimate_seq_scan(50, 3000);

        let parallel_cost = model.estimate_parallel_hash_join(
            5000, 3000, outer_cost, inner_cost, 0.1
        );

        assert!(parallel_cost.optimal_workers > 1);
        assert!(parallel_cost.speedup > 1.0);
        assert!(parallel_cost.efficiency > 0.0);
    }

    #[test]
    fn test_parallel_aggregation_cost() {
        let model = CostModel::new();
        let input_cost = model.estimate_seq_scan(200, 20000);

        let parallel_cost = model.estimate_parallel_aggregation(
            input_cost, 20000, 1000, 3
        );

        assert!(parallel_cost.optimal_workers > 1);
        assert!(parallel_cost.speedup > 1.0);
    }

    #[test]
    fn test_should_use_parallel() {
        let model = CostModel::new();
        let cost = CostEstimate::new(100.0, 50.0, 10.0);
        let parallel_estimate = cost.to_parallel(4, 10.0);

        // Should use parallel if efficiency is high enough
        assert!(model.should_use_parallel(&parallel_estimate));
    }

    #[test]
    fn test_parallel_config_default() {
        let config = ParallelCostConfig::default();
        assert!(config.parallel_startup_cost > 0.0);
        assert!(config.max_parallel_workers > 0);
        assert!(config.min_parallel_efficiency > 0.0);
        assert!(config.min_parallel_efficiency <= 1.0);
    }

    #[test]
    fn test_index_scan_cost() {
        let model = CostModel::new();
        let cost = model.estimate_index_scan(10, 50, 100, 500);
        assert!(cost.io_cost > 0.0);
        assert!(cost.cpu_cost > 0.0);
    }

    #[test]
    fn test_nested_loop_join_cost() {
        let model = CostModel::new();
        let inner_cost = model.estimate_seq_scan(100, 1000);
        let join_cost = model.estimate_nested_loop_join(10, inner_cost, 0.1);
        assert!(join_cost.io_cost > inner_cost.io_cost); // Should be higher due to repetition
    }
}