//! Parallel query execution framework
//!
//! This module provides support for parallel execution of SQL queries,
//! enabling multi-threaded processing for improved performance on multi-core systems.

pub mod executor;
pub mod scheduler;
pub mod resource_manager;
pub mod context;
pub mod metrics;
pub mod concurrent_buffer;
pub mod scanner;
pub mod operators;

pub use executor::{ParallelExecutor, ParallelExecutorConfig, ParallelExecutionResult};
pub use scheduler::{TaskScheduler, ParallelTask, TaskId, TaskType};
pub use resource_manager::{ResourceManager, ResourceConstraints, ResourceType};
pub use context::{ParallelExecutionContext, SharedExecutionState};
pub use metrics::{ParallelExecutionMetrics, MetricCollector};
pub use concurrent_buffer::{
    ConcurrentBufferPool, ConcurrentBufferPoolConfig, ConcurrentBufferPoolStats,
    BackoffStrategy, WorkerHandle, LoadBalanceInfo
};
pub use scanner::{
    ParallelScanner, DefaultParallelScanner, ParallelScannerConfig, ParallelScanIterator,
    TablePartition, PartitionBoundary, PartitionStrategy, LoadBalanceStrategy,
    PartitionStats, ParallelScanStats, PartitionScanResult
};
pub use operators::{
    ParallelOperator, ParallelScanOperator, ParallelFilterOperator, ParallelHashJoinOperator,
    ParallelAggregateOperator, ParallelOperatorConfig, ParallelOperatorStats, ResourceRequirement,
    AggregateFunction, AggregateFunctionType, AggregateState
};