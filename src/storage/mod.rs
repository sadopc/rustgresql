//! Storage engine module
//!
//! Provides page-based storage with B-Tree indexing,
//! buffer pool management, and schema evolution support.

pub mod page;
pub mod buffer;
pub mod btree;
pub mod file_manager;
pub mod schema_evolution;
#[cfg(test)]
pub mod test_utils;

pub use page::{Page, PageType};
pub use buffer::{BufferPool, BufferPoolManager};
pub use btree::{BTree, BTreeIterator};
pub use file_manager::FileManager;
pub use schema_evolution::{
    SchemaEvolutionManager, TableSchema, ColumnSchema, ConstraintSchema, IndexSchema,
    SchemaVersion, MigrationTask, ForeignKeyReference, ConstraintType, IndexType,
    ReferentialAction, TableStatistics, IndexStatistics, MigrationStep, MigrationPriority,
};