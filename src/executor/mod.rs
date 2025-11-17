//! Query execution engine module

pub mod planner;
pub mod engine;
pub mod operators;
pub mod expression;
pub mod scanner;
pub mod ddl_error;
pub mod query_rewrite;
pub mod procedure;

#[cfg(feature = "parallel")]
pub mod parallel;

#[cfg(test)]
mod ddl_tests;

#[cfg(test)]
mod tests;

pub use planner::{QueryPlanner, ExecutionPlan, PlanNode};
pub use engine::{ExecutionEngine, Executor, ExecutionStats};
pub use operators::{QueryResult, ExecutionContext, ScanOperator, FilterOperator, ProjectOperator, JoinOperator, HashJoinOperator, MergeJoinOperator, InsertOperator, UpdateOperator, DeleteOperator, IndexScanOperator, IndexOnlyScanOperator, AggregateOperator, CTEOperator};
pub use expression::{ExpressionEvaluator, EvaluationContext, ThreeValuedLogic, AggregateState};
pub use scanner::{TableScanner, SimpleRowIterator, RowData, MultiTableScanner};
pub use ddl_error::*;
pub use query_rewrite::{QueryRewriter, RewriteResult, ViewFreshness, RewriterStats};
pub use procedure::{ProcedureExecutor, ProcedureContext, ExecutionFrame, ProcedureDef, FunctionDef};

#[cfg(feature = "parallel")]
pub use parallel::{ParallelExecutor, ParallelExecutorConfig, ParallelExecutionResult};