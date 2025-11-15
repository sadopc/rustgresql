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
        }
    }

    /// Create a zero cost estimate
    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Add another cost estimate to this one
    pub fn add(&self, other: &CostEstimate) -> CostEstimate {
        CostEstimate::new(
            self.io_cost + other.io_cost,
            self.cpu_cost + other.cpu_cost,
            self.memory_cost + other.memory_cost,
        )
    }

    /// Multiply this cost estimate by a factor
    pub fn multiply(&self, factor: f64) -> CostEstimate {
        CostEstimate::new(
            self.io_cost * factor,
            self.cpu_cost * factor,
            self.memory_cost * factor,
        )
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
    fn test_seq_scan_cost() {
        let model = CostModel::new();
        let cost = model.estimate_seq_scan(100, 1000);
        assert!(cost.io_cost > 0.0);
        assert!(cost.cpu_cost > 0.0);
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