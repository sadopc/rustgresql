//! Query execution engine module

pub mod planner;
pub mod engine;
pub mod operators;
pub mod expression;
pub mod scanner;
pub mod ddl_error;

#[cfg(test)]
mod ddl_tests;

pub use planner::{QueryPlanner, ExecutionPlan, PlanNode};
pub use engine::{ExecutionEngine, Executor, ExecutionStats};
pub use operators::{QueryResult, ExecutionContext, ScanOperator, FilterOperator, ProjectOperator, JoinOperator, HashJoinOperator, MergeJoinOperator, InsertOperator, UpdateOperator, DeleteOperator, IndexScanOperator, IndexOnlyScanOperator, AggregateOperator};
pub use expression::{ExpressionEvaluator, EvaluationContext, ThreeValuedLogic, AggregateState};
pub use scanner::{TableScanner, SimpleRowIterator, RowData, MultiTableScanner};
pub use ddl_error::*;