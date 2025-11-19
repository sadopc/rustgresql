//! Schema evolution support for DDL operations
//!
//! This module provides functionality for managing schema changes over time,
//! including version tracking, migration support, and compatibility checks.

use crate::{Result, PageId};
// use crate::storage::PageType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema version information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Unique version identifier
    pub version_id: u64,
    /// Version timestamp
    pub timestamp: u64,
    /// Description of changes in this version
    pub description: String,
    /// Migration script from previous version (if any)
    pub migration_script: Option<String>,
    /// Rollback script for this version (if any)
    pub rollback_script: Option<String>,
}

impl SchemaVersion {
    /// Create a new schema version
    pub fn new(version_id: u64, description: String) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            version_id,
            timestamp,
            description,
            migration_script: None,
            rollback_script: None,
        }
    }

    /// Set migration script
    pub fn with_migration_script(mut self, script: String) -> Self {
        self.migration_script = Some(script);
        self
    }

    /// Set rollback script
    pub fn with_rollback_script(mut self, script: String) -> Self {
        self.rollback_script = Some(script);
        self
    }
}

/// Table schema metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableSchema {
    /// Table name
    pub table_name: String,
    /// Schema name (e.g., "public")
    pub schema_name: String,
    /// Current schema version
    pub current_version: u64,
    /// Column definitions
    pub columns: Vec<ColumnSchema>,
    /// Table constraints
    pub constraints: Vec<ConstraintSchema>,
    /// Indexes on this table
    pub indexes: Vec<IndexSchema>,
    /// Table statistics
    pub statistics: TableStatistics,
    /// Creation timestamp
    pub created_at: u64,
    /// Last modification timestamp
    pub modified_at: u64,
}

impl TableSchema {
    /// Create a new table schema
    pub fn new(table_name: String, schema_name: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            table_name,
            schema_name,
            current_version: 1,
            columns: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            statistics: TableStatistics::default(),
            created_at: now,
            modified_at: now,
        }
    }

    /// Add a column to the table schema
    pub fn add_column(&mut self, column: ColumnSchema) -> Result<()> {
        // Check for duplicate column names
        if self.columns.iter().any(|c| c.name == column.name) {
            return Err(crate::error::RustgreSQLError::AlreadyExists(
                format!("Column '{}' already exists in table '{}'", column.name, self.table_name)
            ));
        }

        self.columns.push(column);
        self.update_modified_time();
        Ok(())
    }

    /// Remove a column from the table schema
    pub fn remove_column(&mut self, column_name: &str) -> Result<()> {
        let original_len = self.columns.len();
        self.columns.retain(|c| c.name != column_name);

        if self.columns.len() == original_len {
            return Err(crate::error::RustgreSQLError::NotFound(
                format!("Column '{}' not found in table '{}'", column_name, self.table_name)
            ));
        }

        self.update_modified_time();
        Ok(())
    }

    /// Add a constraint to the table schema
    pub fn add_constraint(&mut self, constraint: ConstraintSchema) -> Result<()> {
        // Check for duplicate constraint names
        if self.constraints.iter().any(|c| c.name == constraint.name) {
            return Err(crate::error::RustgreSQLError::AlreadyExists(
                format!("Constraint '{}' already exists in table '{}'", constraint.name, self.table_name)
            ));
        }

        self.constraints.push(constraint);
        self.update_modified_time();
        Ok(())
    }

    /// Remove a constraint from the table schema
    pub fn remove_constraint(&mut self, constraint_name: &str) -> Result<()> {
        let original_len = self.constraints.len();
        self.constraints.retain(|c| c.name != constraint_name);

        if self.constraints.len() == original_len {
            return Err(crate::error::RustgreSQLError::NotFound(
                format!("Constraint '{}' not found in table '{}'", constraint_name, self.table_name)
            ));
        }

        self.update_modified_time();
        Ok(())
    }

    /// Add an index to the table schema
    pub fn add_index(&mut self, index: IndexSchema) -> Result<()> {
        // Check for duplicate index names
        if self.indexes.iter().any(|i| i.name == index.name) {
            return Err(crate::error::RustgreSQLError::AlreadyExists(
                format!("Index '{}' already exists in table '{}'", index.name, self.table_name)
            ));
        }

        self.indexes.push(index);
        self.update_modified_time();
        Ok(())
    }

    /// Remove an index from the table schema
    pub fn remove_index(&mut self, index_name: &str) -> Result<()> {
        let original_len = self.indexes.len();
        self.indexes.retain(|i| i.name != index_name);

        if self.indexes.len() == original_len {
            return Err(crate::error::RustgreSQLError::NotFound(
                format!("Index '{}' not found in table '{}'", index_name, self.table_name)
            ));
        }

        self.update_modified_time();
        Ok(())
    }

    /// Increment schema version
    pub fn increment_version(&mut self) {
        self.current_version += 1;
        self.update_modified_time();
    }

    /// Update modification timestamp
    fn update_modified_time(&mut self) {
        self.modified_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Get column by name
    pub fn get_column(&self, name: &str) -> Option<&ColumnSchema> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Get mutable column by name
    pub fn get_column_mut(&mut self, name: &str) -> Option<&mut ColumnSchema> {
        self.columns.iter_mut().find(|c| c.name == name)
    }

    /// Get constraint by name
    pub fn get_constraint(&self, name: &str) -> Option<&ConstraintSchema> {
        self.constraints.iter().find(|c| c.name == name)
    }

    /// Get index by name
    pub fn get_index(&self, name: &str) -> Option<&IndexSchema> {
        self.indexes.iter().find(|i| i.name == name)
    }
}

/// Column schema information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnSchema {
    /// Column name
    pub name: String,
    /// Column data type
    pub data_type: String,
    /// Whether column is nullable
    pub nullable: bool,
    /// Default value (if any)
    pub default_value: Option<String>,
    /// Column position in table
    pub position: usize,
    /// Whether column is part of primary key
    pub is_primary_key: bool,
    /// Whether column is unique
    pub is_unique: bool,
    /// Foreign key reference (if any)
    pub foreign_key: Option<ForeignKeyReference>,
}

/// Foreign key reference information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForeignKeyReference {
    /// Referenced table name
    pub referenced_table: String,
    /// Referenced column name
    pub referenced_column: String,
    /// Referencing column name
    pub referencing_column: String,
    /// ON DELETE action
    pub on_delete: ReferentialAction,
    /// ON UPDATE action
    pub on_update: ReferentialAction,
}

/// Referential actions for foreign keys
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ReferentialAction {
    /// No action
    NoAction,
    /// Cascade changes
    Cascade,
    /// Set to NULL
    SetNull,
    /// Set to default value
    SetDefault,
    /// Restrict operation
    Restrict,
}

/// Constraint schema information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintSchema {
    /// Constraint name
    pub name: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Columns involved in the constraint
    pub columns: Vec<String>,
    /// Constraint definition (for check constraints)
    pub definition: Option<String>,
    /// Whether constraint is deferrable
    pub deferrable: bool,
    /// Constraint deferral mode
    pub initially_deferred: bool,
}

/// Constraint types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConstraintType {
    /// Primary key constraint
    PrimaryKey,
    /// Foreign key constraint
    ForeignKey,
    /// Unique constraint
    Unique,
    /// Check constraint
    Check,
    /// Not NULL constraint
    NotNull,
}

/// Index schema information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexSchema {
    /// Index name
    pub name: String,
    /// Indexed columns
    pub columns: Vec<String>,
    /// Whether index is unique
    pub unique: bool,
    /// Index type
    pub index_type: IndexType,
    /// Index storage pages
    pub pages: Vec<PageId>,
    /// Index statistics
    pub statistics: IndexStatistics,
}

/// Index types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IndexType {
    /// B-Tree index
    BTree,
    /// Hash index
    Hash,
    /// GiST index
    GiST,
    /// GIN index
    GIN,
    /// BRIN index
    BRIN,
}

/// Table statistics
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableStatistics {
    /// Number of rows in table
    pub row_count: u64,
    /// Number of pages used by table
    pub page_count: u64,
    /// Average row size in bytes
    pub avg_row_size: usize,
    /// Last analyzed timestamp
    pub last_analyzed: Option<u64>,
}

/// Index statistics
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexStatistics {
    /// Number of index entries
    pub entry_count: u64,
    /// Number of pages used by index
    pub page_count: u64,
    /// Index depth (for B-Tree)
    pub depth: Option<u32>,
    /// Index selectivity estimate
    pub selectivity: Option<f64>,
    /// Last analyzed timestamp
    pub last_analyzed: Option<u64>,
}

/// Schema evolution manager
#[derive(Debug)]
pub struct SchemaEvolutionManager {
    /// Current schema versions for all objects
    schema_versions: HashMap<String, u64>,
    /// Version history for all objects
    version_history: HashMap<String, Vec<SchemaVersion>>,
    /// Migration queue
    migration_queue: Vec<MigrationTask>,
}

impl SchemaEvolutionManager {
    /// Create a new schema evolution manager
    pub fn new() -> Self {
        Self {
            schema_versions: HashMap::new(),
            version_history: HashMap::new(),
            migration_queue: Vec::new(),
        }
    }

    /// Get current version for an object
    pub fn get_current_version(&self, object_name: &str) -> Option<u64> {
        self.schema_versions.get(object_name).copied()
    }

    /// Set current version for an object
    pub fn set_current_version(&mut self, object_name: String, version: u64) {
        self.schema_versions.insert(object_name.clone(), version);
    }

    /// Add a new version to the history
    pub fn add_version(&mut self, object_name: &str, version: SchemaVersion) {
        let history = self.version_history.entry(object_name.to_string())
            .or_insert_with(Vec::new);
        history.push(version);
    }

    /// Get version history for an object
    pub fn get_version_history(&self, object_name: &str) -> &[SchemaVersion] {
        self.version_history.get(object_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Queue a migration task
    pub fn queue_migration(&mut self, task: MigrationTask) {
        self.migration_queue.push(task);
    }

    /// Get next migration task
    pub fn next_migration(&mut self) -> Option<MigrationTask> {
        self.migration_queue.pop()
    }

    /// Check if schema migration is needed
    pub fn needs_migration(&self, target_version: u64, current_version: u64) -> bool {
        target_version > current_version
    }

    /// Generate migration plan from current to target version
    pub fn generate_migration_plan(
        &self,
        object_name: &str,
        target_version: u64
    ) -> Result<Vec<SchemaVersion>> {
        let current_version = self.get_current_version(object_name)
            .ok_or_else(|| crate::error::RustgreSQLError::NotFound(
                format!("Object '{}' not found", object_name)
            ))?;

        let history = self.get_version_history(object_name);
        let mut migration_plan = Vec::new();

        for version in history {
            if version.version_id > current_version && version.version_id <= target_version {
                migration_plan.push(version.clone());
            }
        }

        Ok(migration_plan)
    }
}

/// Migration task
#[derive(Debug, Clone)]
pub struct MigrationTask {
    /// Object name to migrate
    pub object_name: String,
    /// Source version
    pub source_version: u64,
    /// Target version
    pub target_version: u64,
    /// Migration steps
    pub steps: Vec<MigrationStep>,
    /// Priority
    pub priority: MigrationPriority,
}

/// Migration step
#[derive(Debug, Clone)]
pub enum MigrationStep {
    /// Add column
    AddColumn {
        table_name: String,
        column: ColumnSchema,
    },
    /// Drop column
    DropColumn {
        table_name: String,
        column_name: String,
    },
    /// Modify column
    ModifyColumn {
        table_name: String,
        column_name: String,
        new_definition: ColumnSchema,
    },
    /// Add constraint
    AddConstraint {
        table_name: String,
        constraint: ConstraintSchema,
    },
    /// Drop constraint
    DropConstraint {
        table_name: String,
        constraint_name: String,
    },
    /// Create index
    CreateIndex {
        table_name: String,
        index: IndexSchema,
    },
    /// Drop index
    DropIndex {
        table_name: String,
        index_name: String,
    },
    /// Custom SQL migration
    CustomSql {
        description: String,
        sql: String,
    },
}

/// Migration priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MigrationPriority {
    /// Low priority (background migration)
    Low = 0,
    /// Normal priority (scheduled migration)
    Normal = 1,
    /// High priority (urgent migration)
    High = 2,
    /// Critical priority (blocking migration)
    Critical = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_creation() {
        let version = SchemaVersion::new(1, "Initial version".to_string())
            .with_migration_script("ALTER TABLE users ADD COLUMN email VARCHAR(255);".to_string());

        assert_eq!(version.version_id, 1);
        assert_eq!(version.description, "Initial version");
        assert!(version.migration_script.is_some());
        assert!(version.rollback_script.is_none());
    }

    #[test]
    fn test_table_schema_operations() {
        let mut schema = TableSchema::new("users".to_string(), "public".to_string());

        // Add column
        let column = ColumnSchema {
            name: "id".to_string(),
            data_type: "INTEGER".to_string(),
            nullable: false,
            default_value: None,
            position: 0,
            is_primary_key: true,
            is_unique: true,
            foreign_key: None,
        };

        schema.add_column(column).unwrap();
        assert_eq!(schema.columns.len(), 1);
        assert_eq!(schema.current_version, 1);

        // Check duplicate prevention
        let duplicate_column = ColumnSchema {
            name: "id".to_string(),
            data_type: "BIGINT".to_string(),
            nullable: false,
            default_value: None,
            position: 1,
            is_primary_key: false,
            is_unique: false,
            foreign_key: None,
        };

        assert!(schema.add_column(duplicate_column).is_err());
    }

    #[test]
    fn test_schema_evolution_manager() {
        let mut manager = SchemaEvolutionManager::new();

        // Test empty manager
        assert!(manager.get_current_version("test_table").is_none());

        // Set version
        manager.set_current_version("test_table".to_string(), 1);
        assert_eq!(manager.get_current_version("test_table"), Some(1));

        // Add version history
        let version1 = SchemaVersion::new(1, "Initial version".to_string());
        let version2 = SchemaVersion::new(2, "Add email column".to_string());

        manager.add_version("test_table", version1);
        manager.add_version("test_table", version2);

        let history = manager.get_version_history("test_table");
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_foreign_key_reference() {
        let fk_ref = ForeignKeyReference {
            referenced_table: "orders".to_string(),
            referenced_column: "id".to_string(),
            referencing_column: "order_id".to_string(),
            on_delete: ReferentialAction::Cascade,
            on_update: ReferentialAction::Restrict,
        };

        assert_eq!(fk_ref.referenced_table, "orders");
        assert_eq!(fk_ref.on_delete, ReferentialAction::Cascade);
    }
}