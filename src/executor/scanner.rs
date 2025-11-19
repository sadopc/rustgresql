//! Table scanner for efficient data access
//!
//! Provides integration between the execution engine and storage layer
//! for scanning table data with proper column resolution.

use crate::{Result, RustgreSQLError};
use crate::catalog::{CatalogManager, TableDef, ColumnDef};
use crate::storage::{BufferPoolManager, BTree};
use crate::types::{Value, ValueKind, DataType};
use crate::executor::EvaluationContext;
use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};

/// Row data stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    pub values: Vec<Value>,
    pub row_id: Option<u64>, // Optional row identifier
}

impl RowData {
    pub fn new(values: Vec<Value>) -> Self {
        Self {
            values,
            row_id: None,
        }
    }

    pub fn with_id(values: Vec<Value>, row_id: u64) -> Self {
        Self {
            values,
            row_id: Some(row_id),
        }
    }
}

/// Table scanner for efficient data access
#[derive(Debug)]
pub struct TableScanner {
    catalog_manager: Arc<CatalogManager>,
    buffer_manager: Arc<BufferPoolManager>,
    table_def: TableDef,
    btree: Option<BTree>,
    column_map: HashMap<String, usize>, // Column name -> index in row data
}

impl TableScanner {
    /// Create a new table scanner
    pub fn new(
        catalog_manager: Arc<CatalogManager>,
        buffer_manager: Arc<BufferPoolManager>,
        table_name: &str,
    ) -> Result<Self> {
        // Get table definition
        let table_def = catalog_manager.get_table(table_name)?
            .ok_or_else(|| RustgreSQLError::NotFound(format!("Table '{}' not found", table_name)))?;

        // Build column map for efficient lookup
        let mut column_map = HashMap::new();
        for (index, column) in table_def.columns.iter().enumerate() {
            column_map.insert(column.name.clone(), index);
        }

        // Initialize B-Tree if table has data
        let btree = if let Some(root_page_id) = table_def.root_page_id {
            Some(BTree::load(root_page_id, buffer_manager.clone()))
        } else {
            None
        };

        Ok(Self {
            catalog_manager,
            buffer_manager,
            table_def,
            btree,
            column_map,
        })
    }

    /// Scan all rows in the table
    pub fn scan_all(&self) -> Result<SimpleRowIterator> {
        // Read rows from the catalog
        let rows = self.catalog_manager.table_manager.select(&self.table_def.name)?;

        // Convert Value vectors to RowData objects with row indices
        let row_data_list: Vec<RowData> = rows.into_iter()
            .enumerate()
            .map(|(index, values)| RowData::with_id(values, index as u64))
            .collect();

        SimpleRowIterator::from_rows(
            row_data_list,
            self.column_map.clone(),
            self.table_def.columns.clone(),
        )
    }

    /// Get a specific row by key
    pub fn get_row(&self, key: &[u8]) -> Result<Option<RowData>> {
        let btree = self.btree.as_ref()
            .ok_or_else(|| RustgreSQLError::Storage("Table has no data".to_string()))?;

        match btree.search(&key.to_vec())? {
            Some(page_id) => {
                // For now, we'll create dummy data since we don't have a way to get actual row data
                // In a real implementation, we'd fetch the page and deserialize the row data
                Ok(Some(RowData::new(vec![
                    Value { kind: ValueKind::Integer(1) },
                    Value { kind: ValueKind::String("test".to_string()) },
                ])))
            }
            None => Ok(None),
        }
    }

    /// Insert a new row into the table
    pub fn insert_row(&mut self, row_data: RowData) -> Result<()> {
        // Insert into table manager
        self.catalog_manager.table_manager.insert(&self.table_def.name, row_data.values.clone())?;

        let key = self.generate_row_key(&row_data)?;

        // Create B-Tree if it doesn't exist
        if self.btree.is_none() {
            let btree = BTree::new(self.buffer_manager.clone())?;
            let root_page_id = btree.root_page_id();
            self.btree = Some(btree);

            // Update table definition with root page ID
            self.catalog_manager.table_manager.update_table_root_page(&self.table_def.name, root_page_id)?;
        }

        if let Some(ref mut btree) = self.btree {
            // For now, insert with a dummy page value since we don't have actual row storage
            btree.insert(key, 1)?; // Page ID 1 as placeholder

            // Update table definition with current root page ID in case it changed
            self.catalog_manager.table_manager.update_table_root_page(&self.table_def.name, btree.root_page_id())?;
        }

        Ok(())
    }

    /// Delete a specific row
    pub fn delete_row(&mut self, key: &[u8]) -> Result<bool> {
        if let Some(ref mut btree) = self.btree {
            btree.delete(&key.to_vec()).map(|_| true)
        } else {
            Ok(false)
        }
    }

    /// Get table metadata
    pub fn get_table_def(&self) -> &TableDef {
        &self.table_def
    }

    /// Get catalog manager
    pub fn get_catalog_manager(&self) -> &Arc<CatalogManager> {
        &self.catalog_manager
    }

    /// Get column index by name
    pub fn get_column_index(&self, column_name: &str) -> Option<usize> {
        self.column_map.get(column_name).copied()
    }

    /// Create evaluation context for a row
    pub fn create_evaluation_context(&self, row_data: &RowData) -> EvaluationContext {
        let mut columns = HashMap::new();
        for (i, column_def) in self.table_def.columns.iter().enumerate() {
            if i < row_data.values.len() {
                columns.insert(column_def.name.clone(), row_data.values[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Serialize row data to bytes for storage
    fn serialize_row(&self, row_data: &RowData) -> Result<Vec<u8>> {
        bincode::serialize(row_data)
            .map_err(|e| RustgreSQLError::Serialization(format!("Failed to serialize row: {}", e)))
    }

    /// Deserialize row data from bytes
    fn deserialize_row(&self, data: &[u8]) -> Result<RowData> {
        bincode::deserialize(data)
            .map_err(|e| RustgreSQLError::Serialization(format!("Failed to deserialize row: {}", e)))
    }

    /// Generate a key for row storage
    fn generate_row_key(&self, row_data: &RowData) -> Result<Vec<u8>> {
        // Use primary key values for indexing
        let mut key_parts = Vec::new();

        // Find primary key columns
        let primary_key_columns: Vec<usize> = self.table_def.columns.iter()
            .enumerate()
            .filter(|(_, col)| col.primary_key)
            .map(|(idx, _)| idx)
            .collect();

        if !primary_key_columns.is_empty() {
            // Use primary key values
            for &col_idx in &primary_key_columns {
                if col_idx < row_data.values.len() {
                    // Serialize the primary key value
                    let key_bytes = bincode::serialize(&row_data.values[col_idx])
                        .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;
                    key_parts.extend(key_bytes);
                }
            }
        } else {
            // Fallback: use row_id if available, otherwise timestamp
            match row_data.row_id {
                Some(id) => {
                    key_parts.extend(id.to_le_bytes().to_vec());
                }
                None => {
                    // Generate a timestamp-based key as last resort
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    key_parts.extend(timestamp.to_le_bytes().to_vec());
                }
            }
        }

        Ok(key_parts)
    }

    /// Validate row data against table schema
    pub fn validate_row_data(&self, row_data: &RowData) -> Result<()> {
        // Check column count
        if row_data.values.len() != self.table_def.columns.len() {
            return Err(RustgreSQLError::Type(format!(
                "Column count mismatch: expected {}, got {}",
                self.table_def.columns.len(),
                row_data.values.len()
            )));
        }

        // Validate each column
        for (i, (column_def, value)) in self.table_def.columns.iter().zip(row_data.values.iter()).enumerate() {
            // Check NOT NULL constraints
            if !column_def.nullable && matches!(&value.kind, ValueKind::Null(_)) {
                return Err(RustgreSQLError::Type(format!(
                    "Column '{}' cannot be NULL", column_def.name
                )));
            }

            // Type validation (simplified)
            if !matches!(&value.kind, ValueKind::Null(_)) {
                self.validate_column_type(&column_def.data_type, value, i)?;
            }
        }

        Ok(())
    }

    /// Validate that a value matches the expected column type
    fn validate_column_type(&self, expected_type: &DataType, value: &Value, column_index: usize) -> Result<()> {
        match (&expected_type.kind, &value.kind) {
            (crate::types::DataTypeKind::Integer | crate::types::DataTypeKind::Serial | crate::types::DataTypeKind::BigInt | crate::types::DataTypeKind::BigSerial, ValueKind::Integer(_)) => Ok(()),
            (crate::types::DataTypeKind::Real | crate::types::DataTypeKind::DoublePrecision | crate::types::DataTypeKind::Numeric(_, _) | crate::types::DataTypeKind::Decimal(_, _), ValueKind::Float(_)) => Ok(()),
            (crate::types::DataTypeKind::Text | crate::types::DataTypeKind::Varchar(_), ValueKind::String(_)) => Ok(()),
            (crate::types::DataTypeKind::Timestamp | crate::types::DataTypeKind::TimestampWithTimeZone, ValueKind::String(_) | ValueKind::Timestamp(_)) => Ok(()),
            (crate::types::DataTypeKind::Date | crate::types::DataTypeKind::Time | crate::types::DataTypeKind::TimeWithTimeZone | crate::types::DataTypeKind::Interval, ValueKind::String(_)) => Ok(()),
            (crate::types::DataTypeKind::Boolean, ValueKind::Boolean(_)) => Ok(()),
            _ => Err(RustgreSQLError::Type(format!(
                "Type mismatch at column {}: expected {:?}, got {:?}",
                column_index, expected_type.kind, value.kind
            ))),
        }
    }
}

/// Simple iterator over table rows
pub struct SimpleRowIterator {
    btree: Option<BTree>,
    column_map: HashMap<String, usize>,
    column_defs: Vec<ColumnDef>,
    current_index: usize,
    sample_data: Vec<RowData>,
}

impl SimpleRowIterator {
    fn new(
        btree: BTree,
        column_map: HashMap<String, usize>,
        column_defs: Vec<ColumnDef>,
    ) -> Self {
        // Create some sample data for demonstration
        let sample_data = vec![
            RowData::new(vec![
                Value { kind: ValueKind::Integer(1) },
                Value { kind: ValueKind::String("Alice".to_string()) },
                Value { kind: ValueKind::Integer(25) },
            ]),
            RowData::new(vec![
                Value { kind: ValueKind::Integer(2) },
                Value { kind: ValueKind::String("Bob".to_string()) },
                Value { kind: ValueKind::Integer(30) },
            ]),
            RowData::new(vec![
                Value { kind: ValueKind::Integer(3) },
                Value { kind: ValueKind::String("Charlie".to_string()) },
                Value { kind: ValueKind::Integer(35) },
            ]),
        ];

        Self {
            btree: Some(btree),
            column_map,
            column_defs,
            current_index: 0,
            sample_data,
        }
    }

    /// Create an iterator from actual row data
    pub fn from_rows(
        row_data: Vec<RowData>,
        column_map: HashMap<String, usize>,
        column_defs: Vec<ColumnDef>,
    ) -> Result<Self> {
        // No need for BTree when using actual row data
        Ok(Self {
            btree: None,
            column_map,
            column_defs,
            current_index: 0,
            sample_data: row_data,
        })
    }

    /// Get the next row as RowData
    pub fn next_row(&mut self) -> Result<Option<RowData>> {
        if self.current_index < self.sample_data.len() {
            let row_data = self.sample_data[self.current_index].clone();
            self.current_index += 1;
            Ok(Some(row_data))
        } else {
            Ok(None)
        }
    }

    /// Get the next row as EvaluationContext
    pub fn next_context(&mut self) -> Result<Option<EvaluationContext>> {
        if let Some(row_data) = self.next_row()? {
            let mut columns = HashMap::new();
            for (i, column_def) in self.column_defs.iter().enumerate() {
                if i < row_data.values.len() {
                    columns.insert(column_def.name.clone(), row_data.values[i].clone());
                }
            }
            Ok(Some(EvaluationContext::with_columns(columns)))
        } else {
            Ok(None)
        }
    }

    /// Get column definitions for this iterator
    pub fn get_column_defs(&self) -> &[ColumnDef] {
        &self.column_defs
    }

    /// Get column names
    pub fn get_column_names(&self) -> Vec<String> {
        self.column_defs.iter().map(|col| col.name.clone()).collect()
    }
}

/// Multiple table scanner for handling joins (simplified implementation)
#[derive(Debug)]
pub struct MultiTableScanner {
    scanners: Vec<TableScanner>,
}

impl MultiTableScanner {
    pub fn new(scanners: Vec<TableScanner>) -> Self {
        Self {
            scanners,
        }
    }

    /// Get table definitions
    pub fn get_table_defs(&self) -> Vec<&TableDef> {
        self.scanners.iter().map(|scanner| scanner.get_table_def()).collect()
    }
}