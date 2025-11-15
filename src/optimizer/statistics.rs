//! Statistics collection and cardinality estimation
//!
//! Provides table and column statistics for cost-based optimization.
//! This includes row counts, null fractions, most common values, and histograms.

use crate::{Result, catalog::TableDef, types::DataTypeKind};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Statistics for a single column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStatistics {
    /// Column name
    pub name: String,
    /// Number of distinct values
    pub distinct_count: Option<f64>,
    /// Fraction of null values (0.0 to 1.0)
    pub null_fraction: f64,
    /// Average width in bytes
    pub avg_width: f64,
    /// Most common values with their frequencies
    pub mcv: Vec<(String, f64)>, // value, frequency
    /// Histogram bounds for range queries
    pub histogram: Vec<String>,
    /// Minimum value
    pub min_value: Option<String>,
    /// Maximum value
    pub max_value: Option<String>,
    /// Correlation with physical row order (for clustering)
    pub correlation: Option<f64>,
}

impl ColumnStatistics {
    /// Create new column statistics with default values
    pub fn new(name: String, data_type: DataTypeKind) -> Self {
        Self {
            name,
            distinct_count: None,
            null_fraction: 0.0,
            avg_width: estimate_average_width(&data_type),
            mcv: Vec::new(),
            histogram: Vec::new(),
            min_value: None,
            max_value: None,
            correlation: None,
        }
    }

    /// Estimate selectivity for equality predicate
    pub fn estimate_eq_selectivity(&self, value: &str) -> f64 {
        // Check if value is in most common values
        for (mcv_val, freq) in &self.mcv {
            if mcv_val == value {
                return *freq;
            }
        }

        // If not in MCV, use uniform distribution assumption
        if let Some(distinct) = self.distinct_count {
            (1.0 - self.null_fraction) / distinct
        } else {
            0.1 // Default selectivity
        }
    }

    /// Estimate selectivity for inequality predicate
    pub fn estimate_range_selectivity(&self, min_val: Option<&str>, max_val: Option<&str>) -> f64 {
        if self.histogram.is_empty() {
            return 0.33; // Default range selectivity
        }

        let mut lower_bound = 0;
        let mut upper_bound = self.histogram.len();

        // Find position in histogram
        if let Some(min) = min_val {
            for (i, bound) in self.histogram.iter().enumerate() {
                if bound.as_str() >= min {
                    lower_bound = i;
                    break;
                }
            }
        }

        if let Some(max) = max_val {
            for (i, bound) in self.histogram.iter().enumerate() {
                if bound.as_str() > max {
                    upper_bound = i;
                    break;
                }
            }
        }

        let selectivity = (upper_bound - lower_bound) as f64 / self.histogram.len() as f64;
        selectivity * (1.0 - self.null_fraction)
    }

    /// Estimate selectivity for LIKE predicate (simplified)
    pub fn estimate_like_selectivity(&self, pattern: &str) -> f64 {
        // Very simplified heuristic
        if pattern.contains('%') {
            if pattern.starts_with('%') && pattern.ends_with('%') {
                0.3 // Contains pattern
            } else if pattern.starts_with('%') {
                0.4 // Ends with pattern
            } else {
                0.2 // Starts with pattern
            }
        } else {
            0.1 // Exact match (similar to =)
        }
    }
}

/// Statistics for a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStatistics {
    /// Table name
    pub name: String,
    /// Number of rows in the table
    pub row_count: f64,
    /// Number of pages occupied by the table
    pub page_count: usize,
    /// Average row width in bytes
    pub avg_row_width: f64,
    /// Statistics for each column
    pub column_stats: HashMap<String, ColumnStatistics>,
    /// Last time statistics were updated
    pub last_analyzed: Option<chrono::DateTime<chrono::Utc>>,
}

impl TableStatistics {
    /// Create new table statistics
    pub fn new(name: String, row_count: f64, page_count: usize) -> Self {
        Self {
            name,
            row_count,
            page_count,
            avg_row_width: 100.0, // Default estimate
            column_stats: HashMap::new(),
            last_analyzed: None,
        }
    }

    /// Add column statistics
    pub fn add_column_stats(&mut self, column_stats: ColumnStatistics) {
        self.column_stats.insert(column_stats.name.clone(), column_stats);
    }

    /// Get column statistics
    pub fn get_column_stats(&self, column_name: &str) -> Option<&ColumnStatistics> {
        self.column_stats.get(column_name)
    }

    /// Estimate result size for a predicate on a single column
    pub fn estimate_predicate_selectivity(&self, column_name: &str, predicate_type: PredicateType, value: Option<&str>) -> f64 {
        if let Some(col_stats) = self.get_column_stats(column_name) {
            match predicate_type {
                PredicateType::Equals => {
                    if let Some(v) = value {
                        col_stats.estimate_eq_selectivity(v)
                    } else {
                        0.1
                    }
                }
                PredicateType::NotEquals => {
                    if let Some(v) = value {
                        1.0 - col_stats.estimate_eq_selectivity(v)
                    } else {
                        0.9
                    }
                }
                PredicateType::Less | PredicateType::LessOrEqual => {
                    col_stats.estimate_range_selectivity(None, value.as_deref())
                }
                PredicateType::Greater | PredicateType::GreaterOrEqual => {
                    col_stats.estimate_range_selectivity(value.as_deref(), None)
                }
                PredicateType::Like => {
                    if let Some(v) = value {
                        col_stats.estimate_like_selectivity(v)
                    } else {
                        0.1
                    }
                }
                PredicateType::IsNull => col_stats.null_fraction,
                PredicateType::IsNotNull => 1.0 - col_stats.null_fraction,
            }
        } else {
            // Default selectivity when no statistics available
            match predicate_type {
                PredicateType::Equals | PredicateType::Like => 0.1,
                PredicateType::NotEquals => 0.9,
                PredicateType::Less | PredicateType::LessOrEqual |
                PredicateType::Greater | PredicateType::GreaterOrEqual => 0.33,
                PredicateType::IsNull => 0.05,
                PredicateType::IsNotNull => 0.95,
            }
        }
    }

    /// Estimate result cardinality for a predicate
    pub fn estimate_predicate_cardinality(&self, column_name: &str, predicate_type: PredicateType, value: Option<&str>) -> f64 {
        let selectivity = self.estimate_predicate_selectivity(column_name, predicate_type, value);
        self.row_count * selectivity
    }
}

/// Types of predicates for selectivity estimation
#[derive(Debug, Clone, Copy)]
pub enum PredicateType {
    Equals,
    NotEquals,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Like,
    IsNull,
    IsNotNull,
}

/// Statistics manager for collecting and storing table/column statistics
#[derive(Debug, Clone)]
pub struct StatisticsManager {
    /// Table statistics cache
    table_stats: HashMap<String, TableStatistics>,
    /// Configuration for statistics collection
    config: StatisticsConfig,
}

/// Configuration for statistics collection
#[derive(Debug, Clone)]
pub struct StatisticsConfig {
    /// Target sample size for ANALYZE
    pub sample_size: usize,
    /// Maximum number of most common values to track
    pub max_mcv: usize,
    /// Number of histogram buckets
    pub histogram_buckets: usize,
    /// Auto-analyze threshold (percentage of table changes)
    pub auto_analyze_threshold: f64,
}

impl Default for StatisticsConfig {
    fn default() -> Self {
        Self {
            sample_size: 30000,    // PostgreSQL default
            max_mcv: 100,
            histogram_buckets: 100,
            auto_analyze_threshold: 0.1, // 10% of table
        }
    }
}

impl StatisticsManager {
    /// Create new statistics manager
    pub fn new() -> Self {
        Self {
            table_stats: HashMap::new(),
            config: StatisticsConfig::default(),
        }
    }

    /// Create statistics manager with custom configuration
    pub fn with_config(config: StatisticsConfig) -> Self {
        Self {
            table_stats: HashMap::new(),
            config,
        }
    }

    /// Store table statistics
    pub fn store_table_stats(&mut self, stats: TableStatistics) {
        self.table_stats.insert(stats.name.clone(), stats);
    }

    /// Get table statistics
    pub fn get_table_stats(&self, table_name: &str) -> Option<&TableStatistics> {
        self.table_stats.get(table_name)
    }

    /// Estimate cardinality for a table with filters
    pub fn estimate_cardinality(&self, table_name: &str, predicates: &[(String, PredicateType, Option<String>)]) -> f64 {
        if let Some(table_stats) = self.get_table_stats(table_name) {
            let mut result_cardinality = table_stats.row_count;

            // Apply selectivity for each predicate (simplified - assumes independence)
            for (column_name, predicate_type, value) in predicates {
                let selectivity = table_stats.estimate_predicate_selectivity(
                    column_name,
                    *predicate_type,
                    value.as_deref(),
                );
                result_cardinality *= selectivity;
            }

            // Ensure we don't estimate less than 1 row
            result_cardinality.max(1.0)
        } else {
            // Default estimate when no statistics available
            1000.0 * predicates.iter().map(|(_, pt, _)| {
                match pt {
                    PredicateType::Equals | PredicateType::Like => 0.1,
                    PredicateType::NotEquals => 0.9,
                    PredicateType::Less | PredicateType::LessOrEqual |
                    PredicateType::Greater | PredicateType::GreaterOrEqual => 0.33,
                    PredicateType::IsNull => 0.05,
                    PredicateType::IsNotNull => 0.95,
                }
            }).product::<f64>()
        }
    }

    /// Analyze a table and collect statistics
    pub fn analyze_table(&mut self, table_name: &str, table_def: &TableDef, row_count: f64, page_count: usize) -> Result<()> {
        let mut table_stats = TableStatistics::new(table_name.to_string(), row_count, page_count);
        table_stats.last_analyzed = Some(chrono::Utc::now());

        // Collect column statistics (simplified - in a real implementation, this would sample data)
        for column in &table_def.columns {
            let col_stats = ColumnStatistics::new(column.name.clone(), column.data_type.kind.clone());
            table_stats.add_column_stats(col_stats);
        }

        self.store_table_stats(table_stats);
        Ok(())
    }

    /// Check if table needs re-analysis
    pub fn needs_analysis(&self, table_name: &str, rows_changed: usize) -> bool {
        if let Some(table_stats) = self.get_table_stats(table_name) {
            if let Some(last_analyzed) = table_stats.last_analyzed {
                // Check if enough time has passed or enough rows have changed
                let time_since_analyze = chrono::Utc::now() - last_analyzed;
                let change_ratio = rows_changed as f64 / table_stats.row_count.max(1.0);

                time_since_analyze.num_hours() > 24 || change_ratio > self.config.auto_analyze_threshold
            } else {
                true // Never analyzed
            }
        } else {
            true // No statistics exist
        }
    }

    /// Get all stored table statistics
    pub fn get_all_table_stats(&self) -> impl Iterator<Item = &TableStatistics> {
        self.table_stats.values()
    }

    /// Clear all statistics
    pub fn clear_all_stats(&mut self) {
        self.table_stats.clear();
    }

    /// Get configuration
    pub fn config(&self) -> &StatisticsConfig {
        &self.config
    }
}

/// Estimate average width for a data type
fn estimate_average_width(data_type: &DataTypeKind) -> f64 {
    match data_type {
        DataTypeKind::Integer => 4.0,
        DataTypeKind::BigInt => 8.0,
        DataTypeKind::SmallInt => 2.0,
        DataTypeKind::Real => 4.0,
        DataTypeKind::DoublePrecision => 8.0,
        DataTypeKind::Numeric { .. } => 16.0,
        DataTypeKind::Decimal { .. } => 16.0,
        DataTypeKind::Char(_) => 10.0,
        DataTypeKind::Varchar(_) => 25.0,
        DataTypeKind::Text => 50.0,    // Average string length
        DataTypeKind::Boolean => 1.0,
        DataTypeKind::Date => 4.0,
        DataTypeKind::Time => 8.0,
        DataTypeKind::Timestamp => 8.0,
        DataTypeKind::Bytea => 32.0,
        _ => 64.0, // Default for other types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ColumnDef;

    #[test]
    fn test_column_statistics() {
        let mut col_stats = ColumnStatistics::new("test_col".to_string(), DataTypeKind::Integer);
        col_stats.distinct_count = Some(100.0);
        col_stats.null_fraction = 0.1;

        // Test equality selectivity
        let selectivity = col_stats.estimate_eq_selectivity("42");
        assert_eq!(selectivity, (1.0 - 0.1) / 100.0);

        // Test MCV
        col_stats.mcv.push(("42".to_string(), 0.2));
        let selectivity = col_stats.estimate_eq_selectivity("42");
        assert_eq!(selectivity, 0.2);
    }

    #[test]
    fn test_table_statistics() {
        let mut table_stats = TableStatistics::new("test_table".to_string(), 1000.0, 100);
        let col_stats = ColumnStatistics::new("test_col".to_string(), DataTypeKind::Integer);
        table_stats.add_column_stats(col_stats);

        // Test predicate cardinality estimation
        let cardinality = table_stats.estimate_predicate_cardinality(
            "test_col",
            PredicateType::Equals,
            Some("42"),
        );
        assert_eq!(cardinality, 100.0); // 1000 * 0.1 (default)
    }

    #[test]
    fn test_statistics_manager() {
        let mut stats_manager = StatisticsManager::new();

        // Test cardinality estimation without statistics
        let cardinality = stats_manager.estimate_cardinality(
            "nonexistent_table",
            &[("col1".to_string(), PredicateType::Equals, Some("42".to_string()))],
        );
        assert_eq!(cardinality, 100.0); // 1000 * 0.1

        // Add table statistics
        let mut table_stats = TableStatistics::new("test_table".to_string(), 1000.0, 100);
        let mut col_stats = ColumnStatistics::new("test_col".to_string(), DataTypeKind::Integer);
        col_stats.distinct_count = Some(100.0);
        table_stats.add_column_stats(col_stats);
        stats_manager.store_table_stats(table_stats);

        // Test cardinality estimation with statistics
        let cardinality = stats_manager.estimate_cardinality(
            "test_table",
            &[("test_col".to_string(), PredicateType::Equals, Some("42".to_string()))],
        );
        assert_eq!(cardinality, 10.0); // 1000 * 0.01
    }
}