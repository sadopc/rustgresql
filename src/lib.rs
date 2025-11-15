//! RustgreSQL - A PostgreSQL-like database implemented in Rust
//!
//! This project is an educational implementation of a relational database
//! that aims to provide basic SQL functionality with ACID compliance.

pub mod storage;
pub mod transaction;
pub mod sql;
pub mod executor;
pub mod catalog;
pub mod types;
pub mod error;
pub mod optimizer;

// Re-export commonly used types and functions
pub use error::{RustgreSQLError, Result};
pub use storage::{Page, BufferPoolManager, BTree};
pub use transaction::{TransactionManager, Transaction};
pub use transaction::manager::IsolationLevel;
pub use sql::Statement;
pub use executor::{Executor, ExecutionEngine, QueryResult, QueryPlanner};
pub use catalog::{CatalogManager, TableDef, ColumnDef, IndexDef, get_catalog};
pub use types::{DataType, DataTypeKind, Value, ValueKind};
pub use optimizer::{CostModel, StatisticsManager, IndexSelector, OptimizedQueryPlanner as QueryOptimizer};

// Type aliases for convenience
pub type PageId = u64;
pub type TransactionId = u64;

/// Database configuration options
#[derive(Debug, Clone)]
pub struct Config {
    pub page_size: usize,
    pub buffer_pool_size: usize,
    pub wal_enabled: bool,
    pub wal_file_path: Option<String>,
    pub data_file_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            page_size: 8192, // 8KB pages
            buffer_pool_size: 1000,
            wal_enabled: true,
            wal_file_path: Some("rustgresql.wal".to_string()),
            data_file_path: "rustgresql.db".to_string(),
        }
    }
}

/// Main database entry point
#[derive(Debug)]
pub struct Database {
    pub config: Config,
    buffer_manager: std::sync::Arc<storage::BufferPoolManager>,
    catalog_manager: std::sync::Arc<catalog::CatalogManager>,
}

impl Database {
    /// Create a new database instance with the given configuration
    pub fn new(config: Config) -> Result<Self> {
        log::info!("Creating new RustgreSQL database instance");

        // Initialize file manager first (create new database file)
        let file_manager = std::sync::Arc::new(
            std::sync::Mutex::new(storage::file_manager::DefaultFileManager::create(&config.data_file_path, config.page_size as u32)?)
        );

        // Initialize buffer manager
        let buffer_manager = std::sync::Arc::new(
            storage::BufferPoolManager::new(config.buffer_pool_size, file_manager)
        );

        // Initialize catalog manager
        let catalog_manager = get_catalog();
        catalog_manager.initialize()?;

        Ok(Self {
            config,
            buffer_manager,
            catalog_manager,
        })
    }

    /// Initialize the database, creating necessary files and structures
    pub fn initialize(&self) -> Result<()> {
        log::info!("Initializing RustgreSQL database");

        // Initialize catalog
        self.catalog_manager.initialize()?;

        Ok(())
    }

    /// Open an existing database
    pub fn open(config: Config) -> Result<Self> {
        log::info!("Opening RustgreSQL database");

        // Initialize file manager first (file should already exist)
        let file_manager = std::sync::Arc::new(
            std::sync::Mutex::new(storage::file_manager::DefaultFileManager::open(&config.data_file_path)?)
        );

        // Initialize buffer manager
        let buffer_manager = std::sync::Arc::new(
            storage::BufferPoolManager::new(config.buffer_pool_size, file_manager)
        );

        // Initialize catalog manager
        let catalog_manager = get_catalog();
        catalog_manager.initialize()?;

        Ok(Self {
            config,
            buffer_manager,
            catalog_manager,
        })
    }

    /// Begin a new transaction
    pub fn begin_transaction(&self) -> Result<Transaction> {
        let tx_id = 1; // Simplified transaction ID generation
        let start_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok(Transaction::new(tx_id, IsolationLevel::ReadCommitted, start_ts))
    }

    /// Get the catalog manager
    pub fn get_catalog(&self) -> std::sync::Arc<catalog::CatalogManager> {
        self.catalog_manager.clone()
    }

    /// Get the buffer manager
    pub fn get_buffer_manager(&self) -> std::sync::Arc<storage::BufferPoolManager> {
        self.buffer_manager.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.page_size, 8192);
        assert_eq!(config.buffer_pool_size, 1000);
        assert!(config.wal_enabled);
    }

    #[test]
    fn test_database_creation() {
        let config = Config::default();
        let db = Database::new(config);
        assert!(db.expect("Failed to create database").initialize().is_ok());
    }
}