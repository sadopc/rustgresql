//! Index selection algorithms
//!
//! Provides algorithms for selecting the best index for query execution.

use crate::{Result, sql::ast::{Expression, BinaryOperator}, catalog::{IndexDef, IndexType}, optimizer::{cost_model::CostModel, statistics::{StatisticsManager, PredicateType}}};
use std::collections::HashMap;

/// Index access path information
#[derive(Debug, Clone)]
pub struct IndexAccessPath {
    pub index_name: String,
    pub index_type: IndexType,
    pub access_type: IndexAccessType,
    pub selectivity: f64,
    pub cost: f64,
    pub index_condition: Option<Expression>,
    pub residual_conditions: Vec<Expression>, // Conditions that can't be handled by index
}

/// Types of index access
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndexAccessType {
    /// Full index scan
    FullScan,
    /// Range scan using index bounds
    RangeScan,
    /// Point lookup using equality
    PointLookup,
    /// Bitmap index scan (multiple conditions)
    BitmapScan,
    /// Index-only scan (covering index)
    IndexOnlyScan,
}

/// Index condition classification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndexConditionType {
    /// Equality condition (col = const)
    Equality,
    /// Range condition (col > const, col < const, etc.)
    Range,
    /// IN condition with list of values
    InList,
    /// LIKE condition with prefix pattern
    LikePrefix,
    /// Non-indexable condition
    NonIndexable,
}

/// Index selector for choosing optimal indexes
#[derive(Debug)]
pub struct IndexSelector {
    cost_model: CostModel,
    stats_manager: StatisticsManager,
}

impl IndexSelector {
    /// Create new index selector
    pub fn new(cost_model: CostModel, stats_manager: StatisticsManager) -> Self {
        Self {
            cost_model,
            stats_manager,
        }
    }

    /// Select best index for given conditions on a table
    pub fn select_best_index(
        &self,
        table_name: &str,
        indexes: &[IndexDef],
        conditions: &[Expression],
        required_columns: &[String],
    ) -> Option<IndexAccessPath> {
        let mut best_path: Option<IndexAccessPath> = None;
        let mut best_cost = f64::INFINITY;

        for index_def in indexes {
            if let Some(path) = self.evaluate_index(table_name, index_def, conditions, required_columns) {
                if path.cost < best_cost {
                    best_cost = path.cost;
                    best_path = Some(path);
                }
            }
        }

        best_path
    }

    /// Evaluate if an index can be used for the given conditions
    pub fn evaluate_index(
        &self,
        table_name: &str,
        index_def: &IndexDef,
        conditions: &[Expression],
        required_columns: &[String],
    ) -> Option<IndexAccessPath> {
        let index_columns = &index_def.columns;
        if index_columns.is_empty() {
            return None;
        }

        // Find which conditions can use the index
        let (usable_conditions, residual_conditions) = self.classify_conditions(conditions, index_columns);

        if usable_conditions.is_empty() {
            // No conditions can use this index
            return None;
        }

        // Determine access type and selectivity
        let (access_type, selectivity) = self.determine_access_type(&usable_conditions, index_def);

        // Check if this can be an index-only scan
        let is_covering = self.is_covering_index(index_def, required_columns);
        let final_access_type = if is_covering && access_type != IndexAccessType::FullScan {
            IndexAccessType::IndexOnlyScan
        } else {
            access_type
        };

        // Estimate cost
        let cost = self.estimate_index_cost(
            table_name,
            index_def,
            &final_access_type,
            selectivity,
            &usable_conditions,
        );

        Some(IndexAccessPath {
            index_name: index_def.name.clone(),
            index_type: index_def.index_type.clone(),
            access_type: final_access_type,
            selectivity,
            cost,
            index_condition: self.combine_index_conditions(&usable_conditions),
            residual_conditions,
        })
    }

    /// Classify conditions into indexable and residual conditions
    fn classify_conditions(&self, conditions: &[Expression], index_columns: &[String]) -> (Vec<Expression>, Vec<Expression>) {
        let mut usable_conditions = Vec::new();
        let mut residual_conditions = Vec::new();

        for condition in conditions {
            if self.is_condition_indexable(condition, index_columns) {
                usable_conditions.push(condition.clone());
            } else {
                residual_conditions.push(condition.clone());
            }
        }

        (usable_conditions, residual_conditions)
    }

    /// Check if a condition can use an index
    fn is_condition_indexable(&self, condition: &Expression, index_columns: &[String]) -> bool {
        match condition {
            Expression::BinaryOp { left, op, right } => {
                // Check if left side references an indexed column
                if let Expression::Column { name, .. } = &**left {
                    if index_columns.contains(name) {
                        // Check if right side is a constant or parameter
                        match &**right {
                            Expression::Value(_) => true,
                            Expression::Parameter(_) => true,
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Expression::BinaryOp { left, op: BinaryOperator::Like, right } => {
                // Check if left side references an indexed column
                if let Expression::Column { name, .. } = &**left {
                    if index_columns.contains(name) {
                        // Check if right side is a prefix pattern (starts with literal, ends with %)
                        match &**right {
                            Expression::Value(crate::types::Value { kind: crate::types::ValueKind::String(p) }) => !p.starts_with('%'),
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            Expression::BinaryOp { left, op: BinaryOperator::In, right } => {
                // Check if left side references an indexed column
                if let Expression::Column { name, .. } = &**left {
                    index_columns.contains(name)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Determine the best access type for the given conditions
    fn determine_access_type(&self, conditions: &[Expression], index_def: &IndexDef) -> (IndexAccessType, f64) {
        let mut selectivity = 1.0;
        let mut access_type = IndexAccessType::FullScan;

        for condition in conditions {
            let cond_type = self.classify_index_condition(condition);
            let cond_selectivity = self.estimate_condition_selectivity(condition, cond_type);

            selectivity *= cond_selectivity;

            match cond_type {
                IndexConditionType::Equality => {
                    if access_type == IndexAccessType::FullScan {
                        access_type = IndexAccessType::PointLookup;
                    }
                }
                IndexConditionType::Range => {
                    if access_type == IndexAccessType::PointLookup {
                        access_type = IndexAccessType::RangeScan;
                    } else if access_type == IndexAccessType::FullScan {
                        access_type = IndexAccessType::RangeScan;
                    }
                }
                IndexConditionType::InList => {
                    access_type = IndexAccessType::BitmapScan;
                }
                IndexConditionType::LikePrefix => {
                    access_type = IndexAccessType::RangeScan;
                }
                IndexConditionType::NonIndexable => {
                    access_type = IndexAccessType::FullScan;
                }
            }
        }

        (access_type, selectivity)
    }

    /// Classify an index condition
    fn classify_index_condition(&self, condition: &Expression) -> IndexConditionType {
        match condition {
            Expression::BinaryOp { op, .. } => {
                match op {
                    BinaryOperator::Equals => IndexConditionType::Equality,
                    BinaryOperator::NotEquals => IndexConditionType::Range, // Treat as range
                    BinaryOperator::LessThan |
                    BinaryOperator::LessThanOrEquals |
                    BinaryOperator::GreaterThan |
                    BinaryOperator::GreaterThanOrEquals => IndexConditionType::Range,
                    BinaryOperator::Like => IndexConditionType::LikePrefix,
                    BinaryOperator::In => IndexConditionType::InList,
                    _ => IndexConditionType::NonIndexable,
                }
            }
            _ => IndexConditionType::NonIndexable,
        }
    }

    /// Estimate selectivity for a condition
    fn estimate_condition_selectivity(&self, condition: &Expression, cond_type: IndexConditionType) -> f64 {
        match cond_type {
            IndexConditionType::Equality => 0.01,  // Equality is very selective
            IndexConditionType::Range => 0.33,      // Range queries are moderately selective
            IndexConditionType::InList => 0.1,      // IN lists are fairly selective
            IndexConditionType::LikePrefix => 0.05, // Prefix patterns are selective
            IndexConditionType::NonIndexable => 1.0, // Non-indexable conditions don't help
        }
    }

    /// Check if an index covers all required columns
    fn is_covering_index(&self, index_def: &IndexDef, required_columns: &[String]) -> bool {
        let index_columns = &index_def.columns;
        required_columns.iter().all(|col| index_columns.contains(col))
    }

    /// Estimate cost of using an index
    fn estimate_index_cost(
        &self,
        table_name: &str,
        index_def: &IndexDef,
        access_type: &IndexAccessType,
        selectivity: f64,
        conditions: &[Expression],
    ) -> f64 {
        // Get table statistics
        let table_stats = self.stats_manager.get_table_stats(table_name);
        let row_count = table_stats.map(|s| s.row_count).unwrap_or(1000.0) as usize;
        let page_count = table_stats.map(|s| s.page_count).unwrap_or(100);

        let result_rows = (row_count as f64 * selectivity) as usize;

        match access_type {
            IndexAccessType::PointLookup => {
                // Assume 2-3 index page accesses + 1 heap page access per result
                let index_pages = 3;
                let heap_pages = result_rows;
                let cost = self.cost_model.estimate_index_scan(index_pages, heap_pages, 1, result_rows);
                cost.total_cost
            }
            IndexAccessType::RangeScan => {
                // Assume range scan over 10% of index
                let index_pages = (page_count / 10).max(1);
                let heap_pages = result_rows;
                let cost = self.cost_model.estimate_index_scan(index_pages, heap_pages, result_rows / 10, result_rows);
                cost.total_cost
            }
            IndexAccessType::FullScan => {
                // Full index scan (less efficient than table scan)
                let cost = self.cost_model.estimate_seq_scan(page_count, row_count);
                cost.total_cost * 1.5 // Index scans are typically 50% slower than seq scans
            }
            IndexAccessType::BitmapScan => {
                // Bitmap index scan + heap fetches
                let index_cost = page_count as f64 * self.cost_model.config().seq_page_cost;
                let heap_cost = result_rows as f64 * self.cost_model.config().random_page_cost;
                index_cost + heap_cost
            }
            IndexAccessType::IndexOnlyScan => {
                // Index-only scan (no heap access)
                let index_pages = match access_type {
                    IndexAccessType::PointLookup => 3,
                    IndexAccessType::RangeScan => (page_count / 10).max(1),
                    _ => page_count,
                };
                let cost = self.cost_model.estimate_index_only_scan(index_pages, result_rows);
                cost.total_cost
            }
        }
    }

    /// Combine multiple index conditions into a single condition
    fn combine_index_conditions(&self, conditions: &[Expression]) -> Option<Expression> {
        if conditions.is_empty() {
            return None;
        }

        if conditions.len() == 1 {
            Some(conditions[0].clone())
        } else {
            // Combine conditions with AND
            let mut combined = conditions[0].clone();
            for condition in &conditions[1..] {
                combined = Expression::BinaryOp {
                    left: Box::new(combined),
                    op: crate::sql::ast::BinaryOperator::And,
                    right: Box::new(condition.clone()),
                };
            }
            Some(combined)
        }
    }

    /// Estimate index selectivity for a condition
    pub fn estimate_index_selectivity(&self, index_name: &str, condition: &Expression) -> f64 {
        // Use statistics manager if available, otherwise fall back to heuristics
        if let Expression::BinaryOp { left, op, .. } = condition {
            if let Expression::Column { name, .. } = &**left {
                let predicate_type = match op {
                    crate::sql::ast::BinaryOperator::Equals => PredicateType::Equals,
                    crate::sql::ast::BinaryOperator::NotEquals => PredicateType::NotEquals,
                    crate::sql::ast::BinaryOperator::LessThan => PredicateType::Less,
                    crate::sql::ast::BinaryOperator::LessThanOrEquals => PredicateType::LessOrEqual,
                    crate::sql::ast::BinaryOperator::GreaterThan => PredicateType::Greater,
                    crate::sql::ast::BinaryOperator::GreaterThanOrEquals => PredicateType::GreaterOrEqual,
                    _ => PredicateType::Equals, // Default
                };

                // Extract table name from index name (simplified)
                let table_name = index_name.split('_').next().unwrap_or("unknown");
                return self.stats_manager.estimate_cardinality(
                    table_name,
                    &[(name.clone(), predicate_type, None)],
                ) / 1000.0; // Normalize by assumed row count
            }
        }

        0.1 // Default selectivity
    }

    /// Check if an index can be used for a condition
    pub fn is_index_usable(&self, index_def: &IndexDef, condition: &Expression) -> bool {
        self.is_condition_indexable(condition, &index_def.columns)
    }

    /// Get cost model reference
    pub fn cost_model(&self) -> &CostModel {
        &self.cost_model
    }

    /// Get statistics manager reference
    pub fn stats_manager(&self) -> &StatisticsManager {
        &self.stats_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::IndexDef;
    use crate::sql::ast::*;
    use crate::types::Value;

    #[test]
    fn test_condition_indexability() {
        let selector = IndexSelector::new(
            CostModel::new(),
            StatisticsManager::new(),
        );

        let index_columns = vec!["id".to_string(), "name".to_string()];

        // Test indexable equality condition
        let eq_condition = Expression::BinaryOp {
            left: Box::new(Expression::Column { name: "id".to_string(), table: None }),
            op: BinaryOperator::Equals,
            right: Box::new(Expression::Literal(Value::integer(42))),
        };
        assert!(selector.is_condition_indexable(&eq_condition, &index_columns));

        // Test non-indexable condition
        let non_indexable = Expression::BinaryOp {
            left: Box::new(Expression::Column { name: "created_at".to_string(), table: None }),
            op: BinaryOperator::Equals,
            right: Box::new(Expression::Literal(Value::integer(42))),
        };
        assert!(!selector.is_condition_indexable(&non_indexable, &index_columns));
    }

    #[test]
    fn test_access_type_determination() {
        let selector = IndexSelector::new(
            CostModel::new(),
            StatisticsManager::new(),
        );

        let index_def = IndexDef {
            index_id: 1,
            name: "test_idx".to_string(),
            table_id: 1,
            columns: vec!["id".to_string()],
            index_type: IndexType::BTree,
            unique: false,
            primary_key: false,
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
            is_system_generated: false,
        };

        let eq_condition = Expression::BinaryOp {
            left: Box::new(Expression::Column { name: "id".to_string(), table: None }),
            op: BinaryOperator::Equals,
            right: Box::new(Expression::Literal(Value::integer(42))),
        };

        let (access_type, selectivity) = selector.determine_access_type(&[eq_condition], &index_def);
        assert_eq!(access_type, IndexAccessType::PointLookup);
        assert!(selectivity < 1.0);
    }
}