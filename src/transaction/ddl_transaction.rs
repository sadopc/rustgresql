//! DDL transaction integration with MVCC system
//!
//! This module provides transaction safety for DDL operations by integrating
//! with the existing MVCC system and providing schema change isolation.

use crate::{Result, TransactionId};
use crate::executor::ddl_error::{DdlError, DdlOperation};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

/// DDL operation types for transaction tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DdlOperationType {
    CreateTable { table_name: String, schema_name: String },
    DropTable { table_name: String, schema_name: String },
    AlterTable {
        table_name: String,
        schema_name: String,
        operation: String, // Description of the specific ALTER operation
    },
    CreateIndex {
        index_name: String,
        table_name: String,
        schema_name: String
    },
    DropIndex { index_name: String, schema_name: String },
    CreateSchema { schema_name: String },
    DropSchema { schema_name: String },
}

/// DDL operation status within a transaction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DdlOperationStatus {
    /// Operation is planned but not yet executed
    Planned,
    /// Operation is currently executing
    Executing,
    /// Operation completed successfully
    Completed,
    /// Operation failed and needs rollback
    Failed(String),
    /// Operation has been rolled back
    RolledBack,
}

/// A single DDL operation within a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlTransactionOperation {
    /// Unique identifier for this operation within the transaction
    pub operation_id: u64,
    /// The type of DDL operation
    pub operation_type: DdlOperationType,
    /// Current status of the operation
    pub status: DdlOperationStatus,
    /// Dependencies on other operations (operation IDs)
    pub dependencies: Vec<u64>,
    /// Rollback information for undoing this operation
    pub rollback_info: Option<RollbackInfo>,
    /// Timestamp when operation was created
    pub created_at: u64,
    /// Timestamp when operation was completed
    pub completed_at: Option<u64>,
}

/// Information needed to rollback a DDL operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackInfo {
    /// Rollback CREATE TABLE by dropping the table
    DropTable { table_name: String, schema_name: String },
    /// Rollback DROP TABLE by restoring from backup
    RestoreTable {
        table_name: String,
        schema_name: String,
        backup_data: Vec<u8>, // Serialized table definition and data
    },
    /// Rollback ALTER TABLE by reversing the operation
    ReverseAlter {
        table_name: String,
        schema_name: String,
        reverse_operation: String, // SQL to reverse the ALTER
        original_schema: Vec<u8>, // Serialized original column definitions
    },
    /// Rollback CREATE INDEX by dropping the index
    DropIndex {
        index_name: String,
        table_name: String,
        schema_name: String
    },
    /// Rollback DROP INDEX by recreating the index
    RecreateIndex {
        index_name: String,
        table_name: String,
        schema_name: String,
        index_definition: Vec<u8>, // Serialized index definition
    },
    /// Rollback CREATE SCHEMA by dropping the schema
    DropSchema { schema_name: String },
    /// Rollback DROP SCHEMA by restoring from backup
    RestoreSchema {
        schema_name: String,
        backup_data: Vec<u8>, // Serialized schema definition and contents
    },
}

/// Schema change isolation level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchemaChangeIsolation {
    /// Changes are visible only to the transaction that made them
    TransactionLocal,
    /// Changes become visible immediately after commit
    Immediate,
    /// Changes become visible after a delay (for distributed systems)
    Delayed { delay_ms: u64 },
}

/// DDL transaction context for managing schema changes
#[derive(Debug)]
pub struct DdlTransactionContext {
    /// Transaction ID
    pub transaction_id: TransactionId,
    /// All DDL operations in this transaction
    pub operations: Arc<Mutex<HashMap<u64, DdlTransactionOperation>>>,
    /// Next operation ID to assign
    pub next_operation_id: Arc<Mutex<u64>>,
    /// Set of objects locked by this transaction
    pub locked_objects: Arc<Mutex<HashSet<String>>>,
    /// Schema change isolation level
    pub isolation_level: SchemaChangeIsolation,
    /// Snapshot timestamp for visibility checking
    pub snapshot_ts: u64,
    /// Objects that have been modified (for dependency tracking)
    pub modified_objects: Arc<Mutex<HashSet<String>>>,
}

impl DdlTransactionContext {
    /// Create a new DDL transaction context
    pub fn new(
        transaction_id: TransactionId,
        isolation_level: SchemaChangeIsolation,
        snapshot_ts: u64,
    ) -> Self {
        Self {
            transaction_id,
            operations: Arc::new(Mutex::new(HashMap::new())),
            next_operation_id: Arc::new(Mutex::new(1)),
            locked_objects: Arc::new(Mutex::new(HashSet::new())),
            isolation_level,
            snapshot_ts,
            modified_objects: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Add a new DDL operation to the transaction
    pub fn add_operation(
        &self,
        operation_type: DdlOperationType,
        dependencies: Vec<u64>,
        rollback_info: Option<RollbackInfo>,
    ) -> Result<u64> {
        let operation_id = {
            let mut next_id = self.next_operation_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let operation = DdlTransactionOperation {
            operation_id,
            operation_type: operation_type.clone(),
            status: DdlOperationStatus::Planned,
            dependencies,
            rollback_info,
            created_at: self.get_current_timestamp(),
            completed_at: None,
        };

        let mut operations = self.operations.lock().unwrap();
        operations.insert(operation_id, operation);

        // Track modified objects for dependency checking
        let object_name = self.extract_object_name(&operation_type);
        if let Some(name) = object_name {
            let mut modified = self.modified_objects.lock().unwrap();
            modified.insert(name);
        }

        Ok(operation_id)
    }

    /// Mark an operation as executing
    pub fn start_operation(&self, operation_id: u64) -> Result<()> {
        let mut operations = self.operations.lock().unwrap();
        if let Some(operation) = operations.get_mut(&operation_id) {
            operation.status = DdlOperationStatus::Executing;
            Ok(())
        } else {
            Err(DdlError::concurrent_ddl_conflict(
                &format!("Operation {}", operation_id),
                DdlOperation::Create, // Generic operation
            ).into())
        }
    }

    /// Mark an operation as completed
    pub fn complete_operation(&self, operation_id: u64) -> Result<()> {
        let mut operations = self.operations.lock().unwrap();
        if let Some(operation) = operations.get_mut(&operation_id) {
            operation.status = DdlOperationStatus::Completed;
            operation.completed_at = Some(self.get_current_timestamp());
            Ok(())
        } else {
            Err(DdlError::concurrent_ddl_conflict(
                &format!("Operation {}", operation_id),
                DdlOperation::Create, // Generic operation
            ).into())
        }
    }

    /// Mark an operation as failed
    pub fn fail_operation(&self, operation_id: u64, error_message: String) -> Result<()> {
        let mut operations = self.operations.lock().unwrap();
        if let Some(operation) = operations.get_mut(&operation_id) {
            operation.status = DdlOperationStatus::Failed(error_message);
            Ok(())
        } else {
            Err(DdlError::concurrent_ddl_conflict(
                &format!("Operation {}", operation_id),
                DdlOperation::Create, // Generic operation
            ).into())
        }
    }

    /// Check if an object is locked by this transaction
    pub fn is_object_locked(&self, object_name: &str) -> bool {
        let locked = self.locked_objects.lock().unwrap();
        locked.contains(object_name)
    }

    /// Lock an object for this transaction
    pub fn lock_object(&self, object_name: String) -> Result<()> {
        let mut locked = self.locked_objects.lock().unwrap();

        // Check if object is already locked by another transaction
        // This would typically involve a global lock manager
        // For now, we'll just check if it's locked by this transaction

        locked.insert(object_name);
        Ok(())
    }

    /// Release all locks held by this transaction
    pub fn release_all_locks(&self) {
        let mut locked = self.locked_objects.lock().unwrap();
        locked.clear();
    }

    /// Get operations that need to be executed (with dependencies resolved)
    pub fn get_execution_plan(&self) -> Result<Vec<u64>> {
        let operations = self.operations.lock().unwrap();
        let mut plan = Vec::new();
        let mut visited = HashSet::new();

        for (&operation_id, operation) in operations.iter() {
            if operation.status == DdlOperationStatus::Planned {
                self.topological_sort(operation_id, &operations, &mut visited, &mut plan)?;
            }
        }

        Ok(plan)
    }

    /// Check for conflicting operations with other transactions
    pub fn check_conflicts(&self, other_operations: &[DdlOperationType]) -> Result<()> {
        let modified = self.modified_objects.lock().unwrap();

        for other_op in other_operations {
            let other_object = self.extract_object_name(other_op);
            if let Some(obj_name) = other_object {
                if modified.contains(&obj_name) {
                    return Err(DdlError::concurrent_ddl_conflict(
                        &obj_name,
                        DdlOperation::Alter, // Generic operation
                    ).into());
                }
            }
        }

        Ok(())
    }

    /// Rollback all operations in this transaction
    pub fn rollback(&self) -> Result<Vec<RollbackInfo>> {
        let operations = self.operations.lock().unwrap();
        let mut rollback_operations = Vec::new();

        // Get operations in reverse order of completion
        let mut completed_ops: Vec<_> = operations
            .values()
            .filter(|op| op.status == DdlOperationStatus::Completed)
            .collect();

        // Sort by completion time (newest first)
        completed_ops.sort_by(|a, b| {
            match (a.completed_at, b.completed_at) {
                (Some(a_time), Some(b_time)) => b_time.cmp(&a_time),
                _ => std::cmp::Ordering::Equal,
            }
        });

        for operation in completed_ops {
            if let Some(rollback_info) = &operation.rollback_info {
                rollback_operations.push(rollback_info.clone());
            }
        }

        // Mark all operations as rolled back
        drop(operations); // Release the lock
        let mut operations = self.operations.lock().unwrap();
        for operation in operations.values_mut() {
            operation.status = DdlOperationStatus::RolledBack;
        }

        self.release_all_locks();
        Ok(rollback_operations)
    }

    /// Extract the object name from a DDL operation type
    fn extract_object_name(&self, operation: &DdlOperationType) -> Option<String> {
        match operation {
            DdlOperationType::CreateTable { table_name, schema_name } => {
                Some(format!("{}.{}", schema_name, table_name))
            }
            DdlOperationType::DropTable { table_name, schema_name } => {
                Some(format!("{}.{}", schema_name, table_name))
            }
            DdlOperationType::AlterTable { table_name, schema_name, .. } => {
                Some(format!("{}.{}", schema_name, table_name))
            }
            DdlOperationType::CreateIndex { index_name, table_name, schema_name } => {
                Some(format!("{}.{}.{}", schema_name, table_name, index_name))
            }
            DdlOperationType::DropIndex { index_name, schema_name } => {
                Some(format!("{}.{}", schema_name, index_name))
            }
            DdlOperationType::CreateSchema { schema_name } => {
                Some(schema_name.clone())
            }
            DdlOperationType::DropSchema { schema_name } => {
                Some(schema_name.clone())
            }
        }
    }

    /// Topological sort for dependency resolution
    fn topological_sort(
        &self,
        operation_id: u64,
        operations: &HashMap<u64, DdlTransactionOperation>,
        visited: &mut HashSet<u64>,
        plan: &mut Vec<u64>,
    ) -> Result<()> {
        if visited.contains(&operation_id) {
            return Ok(());
        }

        let operation = operations.get(&operation_id)
            .ok_or_else(|| DdlError::concurrent_ddl_conflict(
                &format!("Operation {}", operation_id),
                DdlOperation::Create,
            ))?;

        // Visit dependencies first
        for &dep_id in &operation.dependencies {
            self.topological_sort(dep_id, operations, visited, plan)?;
        }

        visited.insert(operation_id);
        plan.push(operation_id);
        Ok(())
    }

    /// Get current timestamp (simplified implementation)
    fn get_current_timestamp(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Check if this transaction can see schema changes
    pub fn can_see_schema_changes(&self, object_name: &str, change_ts: u64) -> bool {
        match self.isolation_level {
            SchemaChangeIsolation::TransactionLocal => {
                // Only see changes made by this transaction
                self.is_object_locked(object_name)
            }
            SchemaChangeIsolation::Immediate => {
                // See all committed changes
                change_ts <= self.snapshot_ts
            }
            SchemaChangeIsolation::Delayed { delay_ms } => {
                // See changes after delay period
                change_ts <= self.snapshot_ts.saturating_sub(delay_ms)
            }
        }
    }
}

/// Global DDL transaction manager
#[derive(Debug)]
pub struct DdlTransactionManager {
    /// Active DDL transactions
    active_transactions: Arc<Mutex<HashMap<TransactionId, Arc<DdlTransactionContext>>>>,
    /// Global object locks
    global_locks: Arc<Mutex<HashMap<String, TransactionId>>>,
}

impl DdlTransactionManager {
    /// Create a new DDL transaction manager
    pub fn new() -> Self {
        Self {
            active_transactions: Arc::new(Mutex::new(HashMap::new())),
            global_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Begin a DDL transaction
    pub fn begin_transaction(
        &self,
        transaction_id: TransactionId,
        isolation_level: SchemaChangeIsolation,
        snapshot_ts: u64,
    ) -> Result<Arc<DdlTransactionContext>> {
        let context = Arc::new(DdlTransactionContext::new(
            transaction_id,
            isolation_level,
            snapshot_ts,
        ));

        let mut active = self.active_transactions.lock().unwrap();
        active.insert(transaction_id, context.clone());

        Ok(context)
    }

    /// Commit a DDL transaction
    pub fn commit_transaction(&self, transaction_id: TransactionId) -> Result<()> {
        let mut active = self.active_transactions.lock().unwrap();
        if let Some(context) = active.remove(&transaction_id) {
            // Release all locks
            context.release_all_locks();

            // Remove from global locks
            let mut global_locks = self.global_locks.lock().unwrap();
            global_locks.retain(|_, lock_owner| *lock_owner != transaction_id);

            Ok(())
        } else {
            Err(DdlError::ddl_transaction("Transaction not found".to_string()).into())
        }
    }

    /// Rollback a DDL transaction
    pub fn rollback_transaction(&self, transaction_id: TransactionId) -> Result<Vec<RollbackInfo>> {
        let mut active = self.active_transactions.lock().unwrap();
        if let Some(context) = active.remove(&transaction_id) {
            let rollback_ops = context.rollback()?;

            // Remove from global locks
            let mut global_locks = self.global_locks.lock().unwrap();
            global_locks.retain(|_, lock_owner| *lock_owner != transaction_id);

            Ok(rollback_ops)
        } else {
            Err(DdlError::ddl_transaction("Transaction not found".to_string()).into())
        }
    }

    /// Acquire a global object lock
    pub fn acquire_global_lock(
        &self,
        object_name: &str,
        transaction_id: TransactionId,
    ) -> Result<()> {
        let mut global_locks = self.global_locks.lock().unwrap();

        if let Some(&owner_id) = global_locks.get(object_name) {
            if owner_id != transaction_id {
                return Err(DdlError::concurrent_ddl_conflict(
                    object_name,
                    DdlOperation::Alter,
                ).into());
            }
        }

        global_locks.insert(object_name.to_string(), transaction_id);
        Ok(())
    }

    /// Release a global object lock
    pub fn release_global_lock(&self, object_name: &str, transaction_id: TransactionId) -> Result<()> {
        let mut global_locks = self.global_locks.lock().unwrap();

        if let Some(&owner_id) = global_locks.get(object_name) {
            if owner_id == transaction_id {
                global_locks.remove(object_name);
            }
        }

        Ok(())
    }
}

/// Global DDL transaction manager instance
lazy_static::lazy_static! {
    pub static ref GLOBAL_DDL_TRANSACTION_MANAGER: Arc<DdlTransactionManager> =
        Arc::new(DdlTransactionManager::new());
}

/// Get the global DDL transaction manager
pub fn get_ddl_transaction_manager() -> Arc<DdlTransactionManager> {
    GLOBAL_DDL_TRANSACTION_MANAGER.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ddl_transaction_context_creation() {
        let context = DdlTransactionContext::new(
            1,
            SchemaChangeIsolation::TransactionLocal,
            1000,
        );

        assert_eq!(context.transaction_id, 1);
        assert_eq!(context.snapshot_ts, 1000);
        assert!(context.isolation_level == SchemaChangeIsolation::TransactionLocal);
    }

    #[test]
    fn test_add_operation() {
        let context = DdlTransactionContext::new(
            1,
            SchemaChangeIsolation::TransactionLocal,
            1000,
        );

        let operation_type = DdlOperationType::CreateTable {
            table_name: "users".to_string(),
            schema_name: "public".to_string(),
        };

        let operation_id = context.add_operation(operation_type, Vec::new(), None).unwrap();
        assert_eq!(operation_id, 1);
    }

    #[test]
    fn test_object_locking() {
        let context = DdlTransactionContext::new(
            1,
            SchemaChangeIsolation::TransactionLocal,
            1000,
        );

        assert!(!context.is_object_locked("public.users"));
        context.lock_object("public.users".to_string()).unwrap();
        assert!(context.is_object_locked("public.users"));

        context.release_all_locks();
        assert!(!context.is_object_locked("public.users"));
    }
}