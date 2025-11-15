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
}

/// System table manager
#[derive(Debug)]
pub struct SystemTableManager {
    tables: Arc<Mutex<HashMap<String, SystemTable>>>,
    next_table_id: Arc<Mutex<u64>>,
    next_column_id: Arc<Mutex<u64>>,
}

impl SystemTableManager {
    pub fn new() -> Self {
        let manager = Self {
            tables: Arc::new(Mutex::new(HashMap::new())),
            next_table_id: Arc::new(Mutex::new(1)),
            next_column_id: Arc::new(Mutex::new(1)),
        };

        // Initialize system tables
        manager.initialize_system_tables();
        manager
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
            .filter(|t| t.def.table_id > 1) // Skip system tables
            .map(|t| t.def.clone())
            .collect();
        Ok(table_list)
    }

    /// Insert data into a table
    pub fn insert(&self, table_name: &str, row: Vec<crate::types::Value>) -> Result<()> {
        let tables = self.tables.lock().unwrap();
        if let Some(table) = tables.get(table_name) {
            let mut data = table.data.lock().unwrap();
            data.push(row);
        }
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
