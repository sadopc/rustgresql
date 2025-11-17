//! System catalog module
//!
//! Manages database metadata and system tables

pub mod table;
pub mod schema;
pub mod index;
pub mod view;
pub mod constraint_mapping;

pub use table::{TableDef, ColumnDef, TableConstraint, SystemTable, SystemTableManager};
pub use schema::{SchemaDef, SchemaManager};
pub use index::{IndexType, IndexDef, IndexInfo, IndexStats, IndexManager};
pub use view::{ViewDef, ViewManager, ViewStats, RefreshType, DependencyType, DataType as ViewDataType};
pub use constraint_mapping::*;

use crate::Result;
use std::sync::Arc;

/// Unified catalog manager that coordinates all catalog components
#[derive(Debug)]
pub struct CatalogManager {
    pub table_manager: Arc<SystemTableManager>,
    pub schema_manager: Arc<SchemaManager>,
    pub index_manager: Arc<IndexManager>,
    pub view_manager: Arc<ViewManager>,
}

impl CatalogManager {
    /// Create a new catalog manager
    pub fn new() -> Self {
        Self {
            table_manager: Arc::new(SystemTableManager::new()),
            schema_manager: Arc::new(SchemaManager::new()),
            index_manager: Arc::new(IndexManager::new()),
            view_manager: Arc::new(ViewManager::new()),
        }
    }

    /// Initialize the catalog system
    pub fn initialize(&self) -> Result<()> {
        // Create system tables in the catalog if they don't exist
        // This is already handled by the individual managers during initialization

        // Initialize view manager
        self.view_manager.initialize()?;

        Ok(())
    }

    /// Create a table with automatic index creation for primary keys
    pub fn create_table(&self, name: &str, columns: Vec<ColumnDef>) -> Result<u64> {
        let table_id = self.table_manager.create_table(name, columns.clone())?;

        // Create indexes for primary key and unique constraints
        for column in &columns {
            if column.primary_key {
                let index_columns = vec![column.name.clone()];
                self.index_manager.create_primary_key_index(
                    table_id,
                    name,
                    index_columns
                )?;
            } else if column.unique {
                let index_columns = vec![column.name.clone()];
                self.index_manager.create_index(
                    &format!("unique_{}_{}", name, column.name),
                    table_id,
                    index_columns,
                    crate::catalog::IndexType::BTree,
                    true
                )?;
            }
        }

        Ok(table_id)
    }

    /// Drop a table and all its indexes
    pub fn drop_table(&self, name: &str) -> Result<()> {
        // Get table ID before dropping
        if let Some(table_def) = self.table_manager.get_table(name)? {
            // Drop all indexes for this table
            let indexes = self.index_manager.list_table_indexes(table_def.table_id)?;
            for index in indexes {
                self.index_manager.drop_index(&index.def.name)?;
            }

            // Drop the table
            self.table_manager.drop_table(name)?;
        }

        Ok(())
    }

    /// Get table definition with resolved schema information
    pub fn get_table(&self, name: &str) -> Result<Option<TableDef>> {
        self.table_manager.get_table(name)
    }

    /// List tables in a specific schema
    pub fn list_tables_in_schema(&self, schema_name: &str) -> Result<Vec<TableDef>> {
        let schema_def = self.schema_manager.get_schema(schema_name)?;
        if schema_def.is_none() {
            return Err(crate::error::RustgreSQLError::NotFound(format!("Schema '{}' not found", schema_name)));
        }

        let schema_id = schema_def.unwrap().schema_id;
        let all_tables = self.table_manager.list_tables()?;
        Ok(all_tables.into_iter()
            .filter(|table| table.schema_id == schema_id)
            .collect())
    }

    /// Get table with all its indexes
    pub fn get_table_with_indexes(&self, name: &str) -> Result<Option<(TableDef, Vec<IndexInfo>)>> {
        if let Some(table_def) = self.table_manager.get_table(name)? {
            let indexes = self.index_manager.list_table_indexes(table_def.table_id)?;
            Ok(Some((table_def, indexes)))
        } else {
            Ok(None)
        }
    }

    /// Get schema with all its tables and indexes
    pub fn get_schema_with_tables(&self, name: &str) -> Result<Option<(SchemaDef, Vec<TableDef>)>> {
        if let Some(schema_def) = self.schema_manager.get_schema(name)? {
            let tables = self.list_tables_in_schema(name)?;
            Ok(Some((schema_def, tables)))
        } else {
            Ok(None)
        }
    }

    /// Validate that a table exists in the specified schema
    pub fn validate_table_in_schema(&self, table_name: &str, schema_name: &str) -> Result<bool> {
        if let Some(schema_def) = self.schema_manager.get_schema(schema_name)? {
            if let Some(table_def) = self.table_manager.get_table(table_name)? {
                Ok(table_def.schema_id == schema_def.schema_id)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Resolve table reference (schema.table or just table)
    pub fn resolve_table_reference(&self, table_ref: &str) -> Result<(String, String)> {
        if table_ref.contains('.') {
            let parts: Vec<&str> = table_ref.split('.').collect();
            if parts.len() == 2 {
                Ok((parts[0].to_string(), parts[1].to_string()))
            } else {
                Err(crate::error::RustgreSQLError::Parse(format!("Invalid table reference: {}", table_ref)))
            }
        } else {
            // Default to public schema
            Ok(("public".to_string(), table_ref.to_string()))
        }
    }

    /// Get catalog statistics
    pub fn get_catalog_stats(&self) -> Result<CatalogStats> {
        let table_count = self.table_manager.list_tables()?.len();
        let schema_count = self.schema_manager.list_schemas()?.len();
        let index_count = self.index_manager.list_indexes()?.len();
        let view_count = self.view_manager.list_views()?.len();

        Ok(CatalogStats {
            table_count,
            schema_count,
            index_count,
            view_count,
        })
    }

    /// Create schema if it doesn't exist
    pub fn ensure_schema(&self, name: &str, owner_id: u64) -> Result<u64> {
        if self.schema_manager.schema_exists(name) {
            if let Some(schema_def) = self.schema_manager.get_schema(name)? {
                Ok(schema_def.schema_id)
            } else {
                Err(crate::error::RustgreSQLError::Internal("Schema exists but not found".to_string()))
            }
        } else {
            self.schema_manager.create_schema(name, owner_id)
        }
    }
}

/// Catalog statistics
#[derive(Debug, Clone)]
pub struct CatalogStats {
    pub table_count: usize,
    pub schema_count: usize,
    pub index_count: usize,
    pub view_count: usize,
}

impl CatalogManager {
    /// Create a view
    pub fn create_view(
        &self,
        name: &str,
        schema_name: &str,
        columns: Vec<(String, crate::catalog::view::DataType)>,
        query: String,
        materialized: bool,
    ) -> Result<u64> {
        // Resolve schema
        let schema_def = self.schema_manager.get_schema(schema_name)?;
        if schema_def.is_none() {
            return Err(crate::error::RustgreSQLError::NotFound(format!("Schema '{}' not found", schema_name)));
        }

        let schema_id = schema_def.unwrap().schema_id;

        // Create the view
        self.view_manager.create_view(name, schema_id, columns, query, materialized)
    }

    /// Drop a view
    pub fn drop_view(&self, name: &str, cascade: bool) -> Result<()> {
        self.view_manager.drop_view(name, cascade)
    }

    /// Get view definition
    pub fn get_view(&self, name: &str) -> Result<Option<ViewDef>> {
        self.view_manager.get_view(name)
    }

    /// List all views
    pub fn list_views(&self) -> Result<Vec<ViewDef>> {
        self.view_manager.list_views()
    }

    /// List views in a specific schema
    pub fn list_views_in_schema(&self, schema_name: &str) -> Result<Vec<ViewDef>> {
        let schema_def = self.schema_manager.get_schema(schema_name)?;
        if schema_def.is_none() {
            return Err(crate::error::RustgreSQLError::NotFound(format!("Schema '{}' not found", schema_name)));
        }

        let schema_id = schema_def.unwrap().schema_id;
        self.view_manager.list_views_in_schema(schema_id)
    }

    /// Refresh a materialized view
    pub fn refresh_materialized_view(&self, name: &str, with_data: bool) -> Result<()> {
        self.view_manager.refresh_materialized_view(name, with_data)
    }

    /// Check if a view exists
    pub fn view_exists(&self, name: &str) -> bool {
        self.view_manager.view_exists(name)
    }

    /// Get views that depend on a specific table
    pub fn get_dependent_views(&self, table_name: &str) -> Result<Vec<ViewDef>> {
        self.view_manager.get_dependent_views(table_name)
    }
}

/// Global catalog instance
lazy_static::lazy_static! {
    pub static ref GLOBAL_CATALOG: Arc<CatalogManager> = Arc::new(CatalogManager::new());
}

/// Get the global catalog instance
pub fn get_catalog() -> Arc<CatalogManager> {
    GLOBAL_CATALOG.clone()
}