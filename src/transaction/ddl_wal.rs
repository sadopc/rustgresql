//! DDL WAL integration
//!
//! Provides integration between DDL operations and the Write-Ahead Logging system,
//! ensuring durability and recoverability for schema changes.

use crate::{Result, TransactionId};
use crate::transaction::wal::{WALManager, WALRecord, WALRecordType, LSN};
use crate::storage::schema_evolution::{SchemaEvolutionManager, TableSchema, ColumnSchema, ConstraintSchema, IndexSchema};
use std::sync::{Arc, Mutex};

/// DDL WAL manager for handling DDL operation logging
#[derive(Debug)]
pub struct DdlWALManager {
    /// Underlying WAL manager
    wal_manager: Arc<Mutex<WALManager>>,
    /// Schema evolution manager
    schema_manager: Arc<Mutex<SchemaEvolutionManager>>,
    /// Active DDL transactions
    active_transactions: Arc<Mutex<std::collections::HashMap<TransactionId, DdlTransactionState>>>,
}

/// DDL transaction state
#[derive(Debug, Clone)]
pub struct DdlTransactionState {
    /// Transaction ID
    pub transaction_id: TransactionId,
    /// Transaction start LSN
    pub start_lsn: LSN,
    /// Current LSN in this transaction
    pub current_lsn: LSN,
    /// Objects modified in this transaction
    pub modified_objects: Vec<String>,
    /// Previous LSNs for rollback
    pub prev_lsns: Vec<LSN>,
}

impl DdlWALManager {
    /// Create a new DDL WAL manager
    pub fn new(wal_manager: WALManager) -> Self {
        Self {
            wal_manager: Arc::new(Mutex::new(wal_manager)),
            schema_manager: Arc::new(Mutex::new(SchemaEvolutionManager::new())),
            active_transactions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Begin a DDL transaction
    pub fn begin_ddl_transaction(&self, transaction_id: TransactionId) -> Result<LSN> {
        let mut wal = self.wal_manager.lock().unwrap();
        let start_lsn = wal.get_next_lsn();

        // Log transaction begin
        let begin_record = WALRecord::begin(transaction_id, start_lsn);
        let logged_lsn = wal.append_record(begin_record)?;

        // Track transaction state
        let mut active_txs = self.active_transactions.lock().unwrap();
        active_txs.insert(transaction_id, DdlTransactionState {
            transaction_id,
            start_lsn: logged_lsn,
            current_lsn: logged_lsn,
            modified_objects: Vec::new(),
            prev_lsns: Vec::new(),
        });

        Ok(logged_lsn)
    }

    /// Commit a DDL transaction
    pub fn commit_ddl_transaction(&self, transaction_id: TransactionId) -> Result<LSN> {
        let mut active_txs = self.active_transactions.lock().unwrap();
        let tx_state = active_txs.remove(&transaction_id)
            .ok_or_else(|| crate::error::RustgreSQLError::Transaction(
                format!("DDL transaction {} not found", transaction_id)
            ))?;

        let prev_lsn = Some(tx_state.current_lsn);
        drop(active_txs);

        let mut wal = self.wal_manager.lock().unwrap();
        let commit_lsn = wal.get_next_lsn();

        // Log transaction commit
        let commit_record = WALRecord::commit(transaction_id, commit_lsn, prev_lsn);
        let logged_lsn = wal.append_record(commit_record)?;

        // Force flush for DDL transactions
        wal.flush_buffer()?;

        Ok(logged_lsn)
    }

    /// Rollback a DDL transaction
    pub fn rollback_ddl_transaction(&self, transaction_id: TransactionId) -> Result<LSN> {
        let mut active_txs = self.active_transactions.lock().unwrap();
        let tx_state = active_txs.remove(&transaction_id)
            .ok_or_else(|| crate::error::RustgreSQLError::Transaction(
                format!("DDL transaction {} not found", transaction_id)
            ))?;

        let prev_lsn = Some(tx_state.current_lsn);
        drop(active_txs);

        let mut wal = self.wal_manager.lock().unwrap();
        let rollback_lsn = wal.get_next_lsn();

        // Log transaction rollback
        let rollback_record = WALRecord::abort(transaction_id, rollback_lsn, prev_lsn);
        let logged_lsn = wal.append_record(rollback_record)?;

        // Force flush for rollback
        wal.flush_buffer()?;

        Ok(logged_lsn)
    }

    /// Log CREATE TABLE operation
    pub fn log_create_table(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        table_schema: &TableSchema,
    ) -> Result<LSN> {
        let table_definition = bincode::serialize(table_schema)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log CREATE TABLE
        let create_record = WALRecord::create_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            table_name,
            schema_name,
            table_definition,
            dependents,
        );

        let logged_lsn = wal.append_record(create_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log DROP TABLE operation
    pub fn log_drop_table(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        previous_schema: &TableSchema,
    ) -> Result<LSN> {
        let previous_definition = bincode::serialize(previous_schema)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log DROP TABLE
        let drop_record = WALRecord::drop_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            table_name,
            schema_name,
            previous_definition,
            dependents,
        );

        let logged_lsn = wal.append_record(drop_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log ALTER TABLE ADD COLUMN operation
    pub fn log_add_column(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        column: &ColumnSchema,
    ) -> Result<LSN> {
        let column_definition = bincode::serialize(column)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log ALTER TABLE ADD COLUMN
        let alter_record = WALRecord::alter_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::AlterTableAddColumn,
            table_name,
            schema_name,
            "ADD_COLUMN",
            &column.name,
            None,
            Some(column_definition),
            dependents,
        );

        let logged_lsn = wal.append_record(alter_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log ALTER TABLE DROP COLUMN operation
    pub fn log_drop_column(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        column_name: &str,
        previous_definition: &ColumnSchema,
    ) -> Result<LSN> {
        let column_definition = bincode::serialize(previous_definition)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log ALTER TABLE DROP COLUMN
        let alter_record = WALRecord::alter_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::AlterTableDropColumn,
            table_name,
            schema_name,
            "DROP_COLUMN",
            column_name,
            Some(column_definition),
            None,
            dependents,
        );

        let logged_lsn = wal.append_record(alter_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log ALTER TABLE ADD CONSTRAINT operation
    pub fn log_add_constraint(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        constraint: &ConstraintSchema,
    ) -> Result<LSN> {
        let constraint_definition = bincode::serialize(constraint)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log ALTER TABLE ADD CONSTRAINT
        let alter_record = WALRecord::alter_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::AlterTableAddConstraint,
            table_name,
            schema_name,
            "ADD_CONSTRAINT",
            &constraint.name,
            None,
            Some(constraint_definition),
            dependents,
        );

        let logged_lsn = wal.append_record(alter_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log ALTER TABLE DROP CONSTRAINT operation
    pub fn log_drop_constraint(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        schema_name: Option<String>,
        constraint_name: &str,
        previous_definition: &ConstraintSchema,
    ) -> Result<LSN> {
        let constraint_definition = bincode::serialize(previous_definition)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log ALTER TABLE DROP CONSTRAINT
        let alter_record = WALRecord::alter_table(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::AlterTableDropConstraint,
            table_name,
            schema_name,
            "DROP_CONSTRAINT",
            constraint_name,
            Some(constraint_definition),
            None,
            dependents,
        );

        let logged_lsn = wal.append_record(alter_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, table_name.to_string())?;

        Ok(logged_lsn)
    }

    /// Log CREATE INDEX operation
    pub fn log_create_index(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        index_name: &str,
        index: &IndexSchema,
    ) -> Result<LSN> {
        let index_definition = bincode::serialize(index)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log CREATE INDEX (using generic DDL record)
        let ddl_record = WALRecord::ddl(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::CreateIndex,
            "CREATE_INDEX".to_string(),
            index_name.to_string(),
            "index".to_string(),
            None, // schema_name
            Some(index_definition),
            None,
            {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("table_name".to_string(), table_name.to_string());
                metadata
            },
            dependents,
        );

        let logged_lsn = wal.append_record(ddl_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, format!("{}.{}", table_name, index_name))?;

        Ok(logged_lsn)
    }

    /// Log DROP INDEX operation
    pub fn log_drop_index(
        &self,
        transaction_id: TransactionId,
        table_name: &str,
        index_name: &str,
        previous_definition: &IndexSchema,
    ) -> Result<LSN> {
        let index_definition = bincode::serialize(previous_definition)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let dependents = self.get_table_dependents(table_name);

        let prev_lsn = self.get_transaction_prev_lsn(transaction_id)?;

        let mut wal = self.wal_manager.lock().unwrap();
        let record_lsn = wal.get_next_lsn();

        // Log DROP INDEX (using generic DDL record)
        let ddl_record = WALRecord::ddl(
            transaction_id,
            record_lsn,
            prev_lsn,
            WALRecordType::DropIndex,
            "DROP_INDEX".to_string(),
            index_name.to_string(),
            "index".to_string(),
            None, // schema_name
            None,
            Some(index_definition),
            {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("table_name".to_string(), table_name.to_string());
                metadata
            },
            dependents,
        );

        let logged_lsn = wal.append_record(ddl_record)?;
        self.update_transaction_state(transaction_id, logged_lsn, format!("{}.{}", table_name, index_name))?;

        Ok(logged_lsn)
    }

    /// Force flush WAL for durability
    pub fn flush_wal(&self) -> Result<()> {
        let mut wal = self.wal_manager.lock().unwrap();
        wal.flush_buffer()
    }

    /// Get schema evolution manager
    pub fn get_schema_manager(&self) -> Arc<Mutex<SchemaEvolutionManager>> {
        Arc::clone(&self.schema_manager)
    }

    /// Check if transaction is active
    pub fn is_transaction_active(&self, transaction_id: TransactionId) -> bool {
        let active_txs = self.active_transactions.lock().unwrap();
        active_txs.contains_key(&transaction_id)
    }

    /// Get transaction state
    pub fn get_transaction_state(&self, transaction_id: TransactionId) -> Option<DdlTransactionState> {
        let active_txs = self.active_transactions.lock().unwrap();
        active_txs.get(&transaction_id).cloned()
    }

    /// Get previous LSN for transaction
    fn get_transaction_prev_lsn(&self, transaction_id: TransactionId) -> Result<Option<LSN>> {
        let active_txs = self.active_transactions.lock().unwrap();
        let prev_lsn = active_txs.get(&transaction_id)
            .map(|state| Some(state.current_lsn))
            .unwrap_or(None);
        Ok(prev_lsn)
    }

    /// Update transaction state with new LSN
    fn update_transaction_state(&self, transaction_id: TransactionId, lsn: LSN, object_name: String) -> Result<()> {
        let mut active_txs = self.active_transactions.lock().unwrap();
        if let Some(state) = active_txs.get_mut(&transaction_id) {
            state.prev_lsns.push(state.current_lsn);
            state.current_lsn = lsn;
            state.modified_objects.push(object_name);
        }
        Ok(())
    }

    /// Get dependent objects for a table
    fn get_table_dependents(&self, table_name: &str) -> Vec<String> {
        // In a real implementation, this would query the catalog
        // For now, return empty vector
        Vec::new()
    }
}

/// Global DDL WAL manager instance
static DDL_WAL_MANAGER: std::sync::OnceLock<std::sync::Arc<DdlWALManager>> = std::sync::OnceLock::new();

/// Initialize global DDL WAL manager
pub fn init_ddl_wal_manager(wal_manager: WALManager) -> Result<()> {
    DDL_WAL_MANAGER.get_or_init(|| {
        std::sync::Arc::new(DdlWALManager::new(wal_manager))
    });
    Ok(())
}

/// Get global DDL WAL manager
pub fn get_ddl_wal_manager() -> Option<std::sync::Arc<DdlWALManager>> {
    DDL_WAL_MANAGER.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::transaction::wal::WALManager;

    #[test]
    fn test_ddl_transaction_lifecycle() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test_ddl.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        let ddl_manager = DdlWALManager::new(wal_manager);

        // Begin transaction
        let tx_id = 12345;
        let start_lsn = ddl_manager.begin_ddl_transaction(tx_id)?;
        assert!(ddl_manager.is_transaction_active(tx_id));

        // Commit transaction
        let commit_lsn = ddl_manager.commit_ddl_transaction(tx_id)?;
        assert!(!ddl_manager.is_transaction_active(tx_id));
        assert!(commit_lsn > start_lsn);

        Ok(())
    }

    #[test]
    fn test_ddl_rollback() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test_rollback.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        let ddl_manager = DdlWALManager::new(wal_manager);

        let tx_id = 54321;
        let start_lsn = ddl_manager.begin_ddl_transaction(tx_id)?;
        assert!(ddl_manager.is_transaction_active(tx_id));

        let rollback_lsn = ddl_manager.rollback_ddl_transaction(tx_id)?;
        assert!(!ddl_manager.is_transaction_active(tx_id));
        assert!(rollback_lsn > start_lsn);

        Ok(())
    }

    #[test]
    fn test_schema_manager_integration() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test_schema.wal");

        let wal_manager = WALManager::create(&wal_path, 8192).unwrap();
        let ddl_manager = DdlWALManager::new(wal_manager);

        let schema_manager = ddl_manager.get_schema_manager();
        let mut schema = schema_manager.lock().unwrap();

        // Test schema evolution
        schema.set_current_version("test_table".to_string(), 1);
        assert_eq!(schema.get_current_version("test_table"), Some(1));

        Ok(())
    }
}