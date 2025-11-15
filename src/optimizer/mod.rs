//! Query optimizer module
//!
//! This module provides cost-based query optimization for RustgreSQL.
//! It includes components for cost estimation, statistics collection,
//! index selection, plan caching, and optimization rules.

pub mod cost_model;
pub mod statistics;
pub mod index_selection;
pub mod plan_cache;
// pub mod join_ordering;  // Temporarily disabled due to HashMap trait issues
pub mod rules;  // Re-enabled with aggregation pushdown optimization
pub mod query_optimizer;

pub use cost_model::{CostModel, CostEstimate};
pub use statistics::{TableStatistics, ColumnStatistics, StatisticsManager};
pub use index_selection::{IndexSelector, IndexAccessPath, IndexAccessType, IndexConditionType};
pub use plan_cache::{PlanCache, CachedPlan, PlanCacheStats, PlanCacheConfig};
// pub use join_ordering::{JoinOrderOptimizer, JoinTree, JoinTreeNode};  // Temporarily disabled
pub use rules::{OptimizerRule, RuleEngine, PredicatePushdownRule, ProjectionPushdownRule, ConstantFoldingRule, AggregationPushdownRule};
pub use query_optimizer::OptimizedQueryPlanner;