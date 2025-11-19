//! System tables and catalog management

use crate::{Result, PageId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

/// Table definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDef {
    pub table_id: u64,
    pub name: String,
    pub schema_id: u64,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<TableConstraint>,
    pub storage_engine: String,
    pub root_page_id: Option<PageId>,
    pub created_at: std::time::SystemTime,
    pub modified_at: std::time::SystemTime,
}

/// Column definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub column_id: u64,
    pub name: String,
    pub data_type: crate::types::DataType,
    pub nullable: bool,
    pub default_value: Option<crate::types::Value>,
    pub primary_key: bool,
    pub unique: bool,
    pub check_constraint: Option<String>,
}

/// Table constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TableConstraint {
    PrimaryKey { columns: Vec<String> },
    ForeignKey {
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>
    },
    Unique { columns: Vec<String> },
    Check { condition: String },
    NotNull { column: String },
}

/// System catalog table
#[derive(Debug, Clone)]
pub struct SystemTable {
    pub def: TableDef,
    pub data: Arc<Mutex<Vec<Vec<crate::types::Value>>>>,
    pub buffer_manager: Option<std::sync::Arc<crate::storage::BufferPoolManager>>,
}

/// System table manager
#[derive(Debug)]
pub struct SystemTableManager {
    tables: Arc<Mutex<HashMap<String, SystemTable>>>,
    next_table_id: Arc<Mutex<u64>>,
    next_column_id: Arc<Mutex<u64>>,
    buffer_manager: Arc<Mutex<Option<std::sync::Arc<crate::storage::BufferPoolManager>>>>,
}

impl SystemTableManager {
    pub fn new() -> Self {
        let manager = Self {
            tables: Arc::new(Mutex::new(HashMap::new())),
            next_table_id: Arc::new(Mutex::new(1)),
            next_column_id: Arc::new(Mutex::new(1)),
            buffer_manager: Arc::new(Mutex::new(None)),
        };

        // Initialize system tables
        manager.initialize_system_tables();
        manager
    }

    /// Set the buffer manager for persistence operations
    pub fn set_buffer_manager(&self, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) {
        *self.buffer_manager.lock().unwrap() = Some(buffer_manager);
    }

    /// Initialize core system catalog tables
    fn initialize_system_tables(&self) {
        let mut tables = self.tables.lock().unwrap();

        // Create pg_table catalog table
        let pg_table_def = TableDef {
            table_id: 0,
            name: "pg_table".to_string(),
            schema_id: 0, // system schema
            columns: vec![
                ColumnDef {
                    column_id: 0,
                    name: "table_id".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Integer).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                },
                ColumnDef {
                    column_id: 1,
                    name: "table_name".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Text).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: true,
                    check_constraint: None,
                },
                ColumnDef {
                    column_id: 2,
                    name: "schema_id".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Integer).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: false,
                    check_constraint: None,
                },
            ],
            constraints: vec![],
            storage_engine: "btree".to_string(),
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        tables.insert("pg_table".to_string(), SystemTable {
            def: pg_table_def,
            data: Arc::new(Mutex::new(vec![])),
            buffer_manager: None,
        });

        // Create pg_column catalog table
        let pg_column_def = TableDef {
            table_id: 1,
            name: "pg_column".to_string(),
            schema_id: 0,
            columns: vec![
                ColumnDef {
                    column_id: 0,
                    name: "column_id".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Integer).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: true,
                    unique: false,
                    check_constraint: None,
                },
                ColumnDef {
                    column_id: 1,
                    name: "table_id".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Integer).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: false,
                    check_constraint: None,
                },
                ColumnDef {
                    column_id: 2,
                    name: "column_name".to_string(),
                    data_type: crate::types::DataType::new(crate::types::DataTypeKind::Text).nullable(false),
                    nullable: false,
                    default_value: None,
                    primary_key: false,
                    unique: false,
                    check_constraint: None,
                },
            ],
            constraints: vec![],
            storage_engine: "btree".to_string(),
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        tables.insert("pg_column".to_string(), SystemTable {
            def: pg_column_def,
            data: Arc::new(Mutex::new(vec![])),
            buffer_manager: None,
        });
    }

    /// Create a new table
    pub fn create_table(&self, name: &str, columns: Vec<ColumnDef>) -> Result<u64> {
        let table_id = {
            let mut next_id = self.next_table_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let table_def = TableDef {
            table_id,
            name: name.to_string(),
            schema_id: 1, // public schema
            columns,
            constraints: vec![],
            storage_engine: "btree".to_string(),
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        let mut tables = self.tables.lock().unwrap();
        tables.insert(name.to_string(), SystemTable {
            def: table_def.clone(),
            data: Arc::new(Mutex::new(vec![])),
            buffer_manager: None,
        });

        // Update pg_table catalog
        if let Some(pg_table) = tables.get_mut("pg_table") {
            let mut data = pg_table.data.lock().unwrap();
            data.push(vec![
                crate::types::Value { kind: crate::types::ValueKind::Integer(table_id as i64) },
                crate::types::Value { kind: crate::types::ValueKind::String(name.to_string()) },
                crate::types::Value { kind: crate::types::ValueKind::Integer(1) }, // public schema
            ]);
        }

        // Update pg_column catalog
        if let Some(pg_column) = tables.get_mut("pg_column") {
            let mut data = pg_column.data.lock().unwrap();
            let mut column_id_counter = 0;
            for column in &table_def.columns {
                data.push(vec![
                    crate::types::Value { kind: crate::types::ValueKind::Integer(column_id_counter) },
                    crate::types::Value { kind: crate::types::ValueKind::Integer(table_id as i64) },
                    crate::types::Value { kind: crate::types::ValueKind::String(column.name.clone()) },
                ]);
                column_id_counter += 1;
            }
        }

        Ok(table_id)
    }

    /// Drop a table
    pub fn drop_table(&self, name: &str) -> Result<()> {
        let mut tables = self.tables.lock().unwrap();
        tables.remove(name);
        Ok(())
    }

    /// Get table definition by name
    pub fn get_table(&self, name: &str) -> Result<Option<TableDef>> {
        let tables = self.tables.lock().unwrap();
        Ok(tables.get(name).map(|t| t.def.clone()))
    }

    /// Get table definition by ID
    pub fn get_table_by_id(&self, table_id: u64) -> Result<Option<TableDef>> {
        let tables = self.tables.lock().unwrap();
        for table in tables.values() {
            if table.def.table_id == table_id {
                return Ok(Some(table.def.clone()));
            }
        }
        Ok(None)
    }

    /// List all tables
    pub fn list_tables(&self) -> Result<Vec<TableDef>> {
        let tables = self.tables.lock().unwrap();
        let table_list = tables.values()
            .map(|t| t.def.clone())
            .collect();
        Ok(table_list)
    }

    /// Insert data into a table
    pub fn insert(&self, table_name: &str, row: Vec<crate::types::Value>) -> Result<()> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            let mut data = table.data.lock().unwrap();
            data.push(row.clone());

            // If we have a buffer manager, persist the data to disk
            if let Some(ref buffer_manager) = *self.buffer_manager.lock().unwrap() {
                self.persist_row_to_disk(buffer_manager, table, &row)?;
            }
        }
        Ok(())
    }

    /// Update a row in a table by row index
    pub fn update_row(&self, table_name: &str, row_index: usize, new_row: Vec<crate::types::Value>) -> Result<()> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            let mut data = table.data.lock().unwrap();

            // Check if row index is valid
            if row_index >= data.len() {
                return Err(crate::error::RustgreSQLError::NotFound(
                    format!("Row index {} not found in table '{}'", row_index, table_name)
                ));
            }

            // Update the row in memory
            data[row_index] = new_row.clone();

            // If we have a buffer manager, persist the changes to disk
            if let Some(ref buffer_manager) = *self.buffer_manager.lock().unwrap() {
                // For simplicity, we persist the entire row again
                // In a real implementation, this would update the specific page
                self.persist_row_to_disk(buffer_manager, table, &new_row)?;
            }
        } else {
            return Err(crate::error::RustgreSQLError::NotFound(
                format!("Table '{}' not found", table_name)
            ));
        }
        Ok(())
    }

    /// Delete a row from a table by row index
    pub fn delete_row(&self, table_name: &str, row_index: usize) -> Result<()> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            let mut data = table.data.lock().unwrap();

            // Check if row index is valid
            if row_index >= data.len() {
                return Err(crate::error::RustgreSQLError::NotFound(
                    format!("Row index {} not found in table '{}'", row_index, table_name)
                ));
            }

            // Remove the row from memory
            data.remove(row_index);

            // If we have a buffer manager, mark pages as dirty to persist the deletion
            if let Some(ref buffer_manager) = *self.buffer_manager.lock().unwrap() {
                // In a real implementation, this would mark the specific pages as dirty
                // and potentially compact the page storage
                // For now, we rely on the eventual flush of all pages
                buffer_manager.flush_all_pages()?;
            }
        } else {
            return Err(crate::error::RustgreSQLError::NotFound(
                format!("Table '{}' not found", table_name)
            ));
        }
        Ok(())
    }

    /// Persist a row to disk pages
    fn persist_row_to_disk(&self, buffer_manager: &std::sync::Arc<crate::storage::BufferPoolManager>,
                          table: &SystemTable, row: &[crate::types::Value]) -> Result<()> {
        // For now, serialize the row and store it in a data page
        // In a full implementation, this would use proper page management
        use bincode;

        let row_bytes = bincode::serialize(row)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        // Allocate a new page for this row (simplified - should reuse pages)
        let page_id = buffer_manager.new_page(crate::storage::PageType::Data)?;
        let page = buffer_manager.fetch_page(page_id)?;

        {
            let mut page_guard = page.lock().unwrap();

            // Ensure the row fits in the page
            if row_bytes.len() > page_guard.data.len() {
                return Err(crate::error::RustgreSQLError::Storage(
                    format!("Row too large for page: {} bytes", row_bytes.len())
                ));
            }

            // Store the row data
            page_guard.data[..row_bytes.len()].copy_from_slice(&row_bytes);
            page_guard.header.free_bytes = page_guard.data.len() - row_bytes.len();
            page_guard.update_checksum(buffer_manager.page_size());
        }

        // Mark page as dirty and unpin
        buffer_manager.unpin_page(page_id, true)?;

        Ok(())
    }

    /// Select data from a table
    pub fn select(&self, table_name: &str) -> Result<Vec<Vec<crate::types::Value>>> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            let data = table.data.lock().unwrap();
            Ok(data.clone())
        } else {
            Err(crate::error::RustgreSQLError::NotFound(format!("Table '{}' not found", table_name)))
        }
    }

    /// Update the root page ID for a table
    pub fn update_table_root_page(&self, table_name: &str, root_page_id: PageId) -> Result<()> {
        let mut tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get_mut(table_name) {
            table.def.root_page_id = Some(root_page_id);
            Ok(())
        } else {
            Err(crate::error::RustgreSQLError::NotFound(format!("Table '{}' not found", table_name)))
        }
    }

    /// Update the entire table definition (for ALTER TABLE operations)
    pub fn update_table_definition(&self, table_name: &str, new_def: TableDef) -> Result<()> {
        let mut tables = self.tables.lock().unwrap();

        // First, check if the table exists and get its ID
        let table_id = if let Some(table) = tables.get(table_name) {
            table.def.table_id
        } else {
            return Err(crate::error::RustgreSQLError::NotFound(format!("Table '{}' not found", table_name)));
        };

        // Update the table definition
        if let Some(table) = tables.get_mut(table_name) {
            let root_page_id = table.def.root_page_id;
            table.def = new_def;
            table.def.table_id = table_id; // Preserve table_id
            table.def.root_page_id = root_page_id; // Preserve root_page_id
            table.def.modified_at = std::time::SystemTime::now();
        }

        // Collect the column information before releasing the table borrow
        let columns_to_add: Vec<(usize, String)> = if let Some(table) = tables.get(table_name) {
            table.def.columns.iter().enumerate()
                .map(|(idx, col)| (idx, col.name.clone()))
                .collect()
        } else {
            vec![]
        };

        // Update pg_column catalog table to reflect the new columns
        if let Some(pg_column) = tables.get_mut("pg_column") {
            let mut data = pg_column.data.lock().unwrap();

            // Remove all existing column entries for this table
            data.retain(|row| {
                if let crate::types::ValueKind::Integer(tid) = &row[1].kind {
                    *tid != table_id as i64
                } else {
                    true
                }
            });

            // Add new column entries
            for (idx, column_name) in columns_to_add {
                data.push(vec![
                    crate::types::Value { kind: crate::types::ValueKind::Integer(idx as i64) },
                    crate::types::Value { kind: crate::types::ValueKind::Integer(table_id as i64) },
                    crate::types::Value { kind: crate::types::ValueKind::String(column_name) },
                ]);
            }
        }

        Ok(())
    }

    /// Get column names for a table
    pub fn get_column_names(&self, table_name: &str) -> Result<Vec<String>> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            Ok(table.def.columns.iter().map(|c| c.name.clone()).collect())
        } else {
            Err(crate::error::RustgreSQLError::NotFound(format!("Table '{}' not found", table_name)))
        }
    }
}
