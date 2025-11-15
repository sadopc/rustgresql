//! Join ordering optimization
//!
//! Provides algorithms for optimizing join order in queries.

use crate::{
    executor::planner::PlanNode,
    sql::ast::{Expression, BinaryOperator},
    optimizer::{cost_model::CostModel, statistics::StatisticsManager},
};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, BuildHasher};

/// Join tree representation
#[derive(Debug, Clone)]
pub struct JoinTree {
    pub node: JoinTreeNode,
    pub estimated_cost: f64,
    pub estimated_rows: f64,
}

/// Join tree node types
#[derive(Debug, Clone)]
pub enum JoinTreeNode {
    /// Base table scan
    Table {
        name: String,
        estimated_rows: f64,
    },
    /// Inner join
    Join {
        left: Box<JoinTreeNode>,
        right: Box<JoinTreeNode>,
        condition: Option<Expression>,
        join_type: JoinType,
        join_algorithm: JoinAlgorithm,
        estimated_rows: f64,
        estimated_cost: f64,
    },
}

/// Join types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Join algorithms
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinAlgorithm {
    NestedLoop,
    HashJoin,
    MergeJoin,
}

/// Join condition information
#[derive(Debug, Clone)]
pub struct JoinCondition {
    pub left_table: String,
    pub right_table: String,
    pub left_columns: Vec<String>,
    pub right_columns: Vec<String>,
    pub expression: Expression,
    pub selectivity: f64,
}

/// Join ordering strategy
#[derive(Debug, Clone, Copy)]
pub enum JoinOrderingStrategy {
    /// Dynamic programming - exact optimization (exponential time)
    DynamicProgramming,
    /// Greedy heuristic - linear time
    Greedy,
    /// Heuristic-based - rule-based
    Heuristic,
}

/// Join ordering optimizer
#[derive(Debug)]
pub struct JoinOrderOptimizer {
    cost_model: CostModel,
    stats_manager: StatisticsManager,
    strategy: JoinOrderingStrategy,
    max_tables_for_exhaustive: usize, // Threshold for using DP vs greedy
}

impl JoinOrderOptimizer {
    /// Create new join ordering optimizer
    pub fn new(cost_model: CostModel, stats_manager: StatisticsManager) -> Self {
        Self {
            cost_model,
            stats_manager,
            strategy: JoinOrderingStrategy::Greedy,
            max_tables_for_exhaustive: 6, // Use DP for up to 6 tables
        }
    }

    /// Create join ordering optimizer with custom strategy
    pub fn with_strategy(
        cost_model: CostModel,
        stats_manager: StatisticsManager,
        strategy: JoinOrderingStrategy,
        max_tables_for_exhaustive: usize,
    ) -> Self {
        Self {
            cost_model,
            stats_manager,
            strategy,
            max_tables_for_exhaustive,
        }
    }

    /// Optimize join order for multiple tables
    pub fn optimize_join_order(
        &self,
        tables: &[String],
        join_conditions: &[JoinCondition],
    ) -> JoinTree {
        if tables.is_empty() {
            return JoinTree {
                node: JoinTreeNode::Table {
                    name: String::new(),
                    estimated_rows: 0.0,
                },
                estimated_cost: 0.0,
                estimated_rows: 0.0,
            };
        }

        if tables.len() == 1 {
            let table_name = &tables[0];
            let estimated_rows = self.estimate_table_rows(table_name);
            return JoinTree {
                node: JoinTreeNode::Table {
                    name: table_name.clone(),
                    estimated_rows,
                },
                estimated_cost: self.cost_model.estimate_seq_scan(100, estimated_rows as usize).total_cost,
                estimated_rows,
            };
        }

        match self.strategy {
            JoinOrderingStrategy::DynamicProgramming | JoinOrderingStrategy::Greedy => {
                self.optimize_join_order_greedy(tables, join_conditions)
            }
            JoinOrderingStrategy::Heuristic => {
                self.optimize_join_order_heuristic(tables, join_conditions)
            }
        }
    }

    /// Dynamic programming join ordering (exhaustive search)
    fn optimize_join_order_dp(
        &self,
        tables: &[String],
        join_conditions: &[JoinCondition],
    ) -> JoinTree {
        let n = tables.len();
        let mut best_cost: HashMap<HashSet<String>, (f64, f64, JoinTreeNode)> = HashMap::new();

        // Initialize with single tables
        for table in tables {
            let mut table_set = HashSet::new();
            table_set.insert(table.clone());
            let estimated_rows = self.estimate_table_rows(table);
            let cost = self.cost_model.estimate_seq_scan(100, estimated_rows as usize).total_cost;

            best_cost.insert(
                table_set,
                (cost, estimated_rows, JoinTreeNode::Table {
                    name: table.clone(),
                    estimated_rows,
                }),
            );
        }

        // DP over subsets of increasing size
        for subset_size in 2..=n {
            let all_tables: HashSet<String> = tables.iter().cloned().collect();
            let subsets = self.generate_subsets(&all_tables, subset_size);

            for subset in subsets {
                let (best_subset_cost, best_subset_rows, best_tree) = self.find_best_partition(
                    &subset,
                    &best_cost,
                    join_conditions,
                );
                best_cost.insert(subset, (best_subset_cost, best_subset_rows, best_tree));
            }
        }

        let all_tables: HashSet<String> = tables.iter().cloned().collect();
        let (final_cost, final_rows, final_tree) = best_cost.get(&all_tables).unwrap();

        JoinTree {
            node: final_tree.clone(),
            estimated_cost: *final_cost,
            estimated_rows: *final_rows,
        }
    }

    /// Greedy join ordering
    fn optimize_join_order_greedy(
        &self,
        tables: &[String],
        join_conditions: &[JoinCondition],
    ) -> JoinTree {
        if tables.is_empty() {
            return JoinTree {
                node: JoinTreeNode::Table {
                    name: String::new(),
                    estimated_rows: 0.0,
                },
                estimated_cost: 0.0,
                estimated_rows: 0.0,
            };
        }

        // Start with the table having the smallest estimated rows
        let mut remaining_tables: HashSet<String> = tables.iter().cloned().collect();
        let mut best_table = self.find_smallest_table(&remaining_tables);
        remaining_tables.remove(&best_table);

        let best_rows = self.estimate_table_rows(&best_table);
        let mut current_tree = JoinTreeNode::Table {
            name: best_table.clone(),
            estimated_rows: best_rows,
        };
        let mut current_cost = self.cost_model.estimate_seq_scan(100, best_rows as usize).total_cost;
        let mut current_rows = best_rows;

        // Greedily add tables
        while !remaining_tables.is_empty() {
            let (next_table, next_tree, next_cost, next_rows) = self.find_best_next_join(
                &current_tree,
                current_cost,
                current_rows,
                &remaining_tables,
                join_conditions,
            );

            current_tree = next_tree;
            current_cost = next_cost;
            current_rows = next_rows;
            remaining_tables.remove(&next_table);
        }

        JoinTree {
            node: current_tree,
            estimated_cost: current_cost,
            estimated_rows: current_rows,
        }
    }

    /// Heuristic-based join ordering
    fn optimize_join_order_heuristic(
        &self,
        tables: &[String],
        join_conditions: &[JoinCondition],
    ) -> JoinTree {
        // Simple heuristic: prioritize tables with selective joins
        let mut table_selectivity: HashMap<String, f64> = HashMap::new();

        for table in tables {
            let mut min_selectivity = 1.0;
            for condition in join_conditions {
                if condition.left_table == *table || condition.right_table == *table {
                    min_selectivity = min_selectivity.min(condition.selectivity);
                }
            }
            table_selectivity.insert(table.clone(), min_selectivity);
        }

        // Sort tables by selectivity (most selective first)
        let mut sorted_tables = tables.to_vec();
        sorted_tables.sort_by(|a, b| {
            table_selectivity[a].partial_cmp(&table_selectivity[b]).unwrap_or(std::cmp::Ordering::Equal)
        });

        self.optimize_join_order_greedy(&sorted_tables, join_conditions)
    }

    /// Find the table with smallest estimated row count
    fn find_smallest_table(&self, tables: &HashSet<String>) -> String {
        tables.iter()
            .min_by_key(|table| self.estimate_table_rows(table) as u64)
            .unwrap()
            .clone()
    }

    /// Find best next table to join with greedy algorithm
    fn find_best_next_join(
        &self,
        current_tree: &JoinTreeNode,
        current_cost: f64,
        current_rows: f64,
        remaining_tables: &HashSet<String>,
        join_conditions: &[JoinCondition],
    ) -> (String, JoinTreeNode, f64, f64) {
        let mut best_table = String::new();
        let mut best_cost = f64::INFINITY;
        let mut best_rows = 0.0;
        let mut best_tree = current_tree.clone();

        for table in remaining_tables {
            let table_rows = self.estimate_table_rows(table);
            let condition = self.find_join_condition(current_tree, table, join_conditions);
            let selectivity = condition.as_ref().map(|c| c.selectivity).unwrap_or(0.1);

            // Choose best join algorithm
            let join_algorithm = self.choose_join_algorithm(current_rows, table_rows, selectivity);
            let estimated_rows = current_rows * table_rows * selectivity;
            let estimated_cost = self.estimate_join_cost(current_cost, current_rows, table_rows, selectivity, join_algorithm);

            if estimated_cost < best_cost {
                best_table = table.clone();
                best_cost = estimated_cost;
                best_rows = estimated_rows;
                best_tree = JoinTreeNode::Join {
                    left: Box::new(current_tree.clone()),
                    right: Box::new(JoinTreeNode::Table {
                        name: table.clone(),
                        estimated_rows: table_rows,
                    }),
                    condition: condition.map(|c| c.expression.clone()),
                    join_type: JoinType::Inner,
                    join_algorithm,
                    estimated_rows,
                    estimated_cost,
                };
            }
        }

        (best_table, best_tree, best_cost, best_rows)
    }

    /// Choose best join algorithm based on input sizes and selectivity
    fn choose_join_algorithm(&self, left_rows: f64, right_rows: f64, selectivity: f64) -> JoinAlgorithm {
        let smaller_input = left_rows.min(right_rows);
        let larger_input = left_rows.max(right_rows);
        let result_rows = left_rows * right_rows * selectivity;

        // Heuristic rules for join algorithm selection
        if smaller_input < 100.0 && result_rows < 1000.0 {
            JoinAlgorithm::NestedLoop  // Small inputs, use nested loop
        } else if selectivity < 0.01 && smaller_input < 10000.0 {
            JoinAlgorithm::HashJoin     // Highly selective, use hash join
        } else if selectivity > 0.1 {
            JoinAlgorithm::MergeJoin    // Low selectivity, use merge join
        } else {
            JoinAlgorithm::HashJoin     // Default to hash join
        }
    }

    /// Estimate cost of join operation
    fn estimate_join_cost(
        &self,
        current_cost: f64,
        left_rows: f64,
        right_rows: f64,
        selectivity: f64,
        algorithm: JoinAlgorithm,
    ) -> f64 {
        let join_cost = match algorithm {
            JoinAlgorithm::NestedLoop => {
                self.cost_model.estimate_nested_loop_join(
                    left_rows as usize,
                    self.cost_model.estimate_seq_scan(100, right_rows as usize),
                    selectivity,
                ).total_cost
            }
            JoinAlgorithm::HashJoin => {
                self.cost_model.estimate_hash_join(
                    left_rows as usize,
                    right_rows as usize,
                    self.cost_model.estimate_seq_scan(100, left_rows as usize),
                    self.cost_model.estimate_seq_scan(100, right_rows as usize),
                    selectivity,
                ).total_cost
            }
            JoinAlgorithm::MergeJoin => {
                self.cost_model.estimate_merge_join(
                    left_rows as usize,
                    right_rows as usize,
                    self.cost_model.estimate_seq_scan(100, left_rows as usize),
                    self.cost_model.estimate_seq_scan(100, right_rows as usize),
                    selectivity,
                ).total_cost
            }
        };

        current_cost + join_cost
    }

    /// Estimate table row count
    fn estimate_table_rows(&self, table_name: &str) -> f64 {
        if let Some(table_stats) = self.stats_manager.get_table_stats(table_name) {
            table_stats.row_count
        } else {
            1000.0 // Default estimate
        }
    }

    /// Find join condition between two tables
    fn find_join_condition(
        &self,
        left_tree: &JoinTreeNode,
        right_table: &str,
        join_conditions: &[JoinCondition],
    ) -> Option<&JoinCondition> {
        let left_tables = self.extract_table_names(left_tree);

        join_conditions.iter().find(|condition| {
            (left_tables.contains(&condition.left_table) && condition.right_table == right_table) ||
            (left_tables.contains(&condition.right_table) && condition.left_table == right_table)
        })
    }

    /// Extract table names from join tree
    fn extract_table_names(&self, node: &JoinTreeNode) -> HashSet<String> {
        match node {
            JoinTreeNode::Table { name, .. } => {
                let mut set = HashSet::new();
                set.insert(name.clone());
                set
            }
            JoinTreeNode::Join { left, right, .. } => {
                let mut set = self.extract_table_names(left);
                set.extend(self.extract_table_names(right));
                set
            }
        }
    }

    /// Generate all subsets of given size
    fn generate_subsets(&self, full_set: &HashSet<String>, size: usize) -> Vec<HashSet<String>> {
        let items: Vec<String> = full_set.iter().cloned().collect();
        let mut subsets = Vec::new();

        for combination in items.iter().combinations(size) {
            let subset: HashSet<String> = combination.into_iter().collect();
            subsets.push(subset);
        }

        subsets
    }

    /// Find best partition for DP algorithm
    fn find_best_partition(
        &self,
        subset: &HashSet<String>,
        best_cost: &HashMap<HashSet<String>, (f64, f64, JoinTreeNode)>,
        join_conditions: &[JoinCondition],
    ) -> (f64, f64, JoinTreeNode) {
        let mut best_total_cost = f64::INFINITY;
        let mut best_total_rows = 0.0;
        let mut best_tree = JoinTreeNode::Table {
            name: String::new(),
            estimated_rows: 0.0,
        };

        // Try all possible partitions
        for k in 1..subset.len() {
            for left_subset in self.generate_subsets(subset, k) {
                let mut right_subset = subset.clone();
                for table in &left_subset {
                    right_subset.remove(table);
                }

                if let (Some((left_cost, left_rows, left_tree)), Some((right_cost, right_rows, right_tree))) =
                    (best_cost.get(&left_subset), best_cost.get(&right_subset))
                {
                    // Estimate join cost
                    let condition = self.find_join_condition_between_sets(&left_subset, &right_subset, join_conditions);
                    let selectivity = condition.as_ref().map(|c| c.selectivity).unwrap_or(0.1);
                    let join_algorithm = self.choose_join_algorithm(left_rows, right_rows, selectivity);
                    let join_cost = self.estimate_join_cost(left_cost + right_cost, left_rows, right_rows, selectivity, join_algorithm);
                    let result_rows = left_rows * right_rows * selectivity;

                    if join_cost < best_total_cost {
                        best_total_cost = join_cost;
                        best_total_rows = result_rows;
                        best_tree = JoinTreeNode::Join {
                            left: Box::new(left_tree.clone()),
                            right: Box::new(right_tree.clone()),
                            condition: condition.map(|c| c.expression.clone()),
                            join_type: JoinType::Inner,
                            join_algorithm,
                            estimated_rows: result_rows,
                            estimated_cost: join_cost,
                        };
                    }
                }
            }
        }

        (best_total_cost, best_total_rows, best_tree)
    }

    /// Find join condition between two sets of tables
    fn find_join_condition_between_sets<'a>(
        &self,
        left_set: &HashSet<String>,
        right_set: &HashSet<String>,
        join_conditions: &'a [JoinCondition],
    ) -> Option<&'a JoinCondition> {
        join_conditions.iter().find(|condition| {
            (left_set.contains(&condition.left_table) && right_set.contains(&condition.right_table)) ||
            (left_set.contains(&condition.right_table) && right_set.contains(&condition.left_table))
        })
    }

    /// Estimate cost of join tree
    pub fn get_join_tree_cost(&self, join_tree: &JoinTree) -> f64 {
        join_tree.estimated_cost
    }

    /// Get strategy
    pub fn strategy(&self) -> JoinOrderingStrategy {
        self.strategy
    }

    /// Update strategy
    pub fn set_strategy(&mut self, strategy: JoinOrderingStrategy) {
        self.strategy = strategy;
    }
}

// Extension for itertools-like combinations
trait CombinationsExt<T: Clone> {
    fn combinations(&self, k: usize) -> Vec<Vec<T>>;
}

impl<T: Clone> CombinationsExt<T> for &[T] {
    fn combinations(&self, k: usize) -> Vec<Vec<T>> {
        if k == 0 {
            return vec![vec![]];
        }
        if k > self.len() {
            return vec![];
        }
        if k == self.len() {
            return vec![self.to_vec()];
        }

        let mut result = Vec::new();
        for i in 0..=self.len() - k {
            let head = &self[i];
            let tail_combinations = self[i + 1..].combinations(k - 1);
            for mut combo in tail_combinations {
                combo.insert(0, head.clone());
                result.push(combo);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_ordering_optimizer() {
        let cost_model = CostModel::new();
        let stats_manager = StatisticsManager::new();
        let optimizer = JoinOrderOptimizer::new(cost_model, stats_manager);

        let tables = vec!["users".to_string(), "orders".to_string(), "products".to_string()];
        let join_conditions = vec![];

        let join_tree = optimizer.optimize_join_order(&tables, &join_conditions);
        assert!(join_tree.estimated_cost > 0.0);
        assert!(join_tree.estimated_rows > 0.0);
    }
}