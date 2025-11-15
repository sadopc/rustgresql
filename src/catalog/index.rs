//! Index management

use crate::{Result, PageId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};

/// Index type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndexType {
    BTree,
    Hash,
    GIN,   // Generalized Inverted Index
    GIST,  // Generalized Search Tree
}

/// Index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDef {
    pub index_id: u64,
    pub name: String,
    pub table_id: u64,
    pub columns: Vec<String>,
    pub index_type: IndexType,
    pub unique: bool,
    pub primary_key: bool,
    pub is_system_generated: bool, // True for indexes created automatically (PK, constraints)
    pub root_page_id: Option<PageId>,
    pub created_at: std::time::SystemTime,
    pub modified_at: std::time::SystemTime,
}

/// Index info with metadata
#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub def: IndexDef,
    pub stats: IndexStats,
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub pages_used: u64,
    pub entries: u64,
    pub height: u32,
    pub last_analyzed: Option<std::time::SystemTime>,
}

/// Index manager
#[derive(Debug)]
pub struct IndexManager {
    indexes: Arc<Mutex<HashMap<String, IndexInfo>>>,
    next_index_id: Arc<Mutex<u64>>,
    table_indexes: Arc<Mutex<HashMap<u64, Vec<String>>>>, // table_id -> index_names
}

impl IndexManager {
    pub fn new() -> Self {
        Self {
            indexes: Arc::new(Mutex::new(HashMap::new())),
            next_index_id: Arc::new(Mutex::new(1)),
            table_indexes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new index
    pub fn create_index(&self, name: &str, table_id: u64, columns: Vec<String>, index_type: IndexType, unique: bool) -> Result<u64> {
        let index_id = {
            let mut next_id = self.next_index_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let index_def = IndexDef {
            index_id,
            name: name.to_string(),
            table_id,
            columns: columns.clone(),
            index_type,
            unique,
            primary_key: false,
            is_system_generated: false, // User-created index
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        let index_info = IndexInfo {
            def: index_def.clone(),
            stats: IndexStats {
                pages_used: 0,
                entries: 0,
                height: 0,
                last_analyzed: None,
            },
        };

        let mut indexes = self.indexes.lock().unwrap();
        if indexes.contains_key(name) {
            return Err(crate::error::RustgreSQLError::AlreadyExists(format!("Index '{}' already exists", name)));
        }

        indexes.insert(name.to_string(), index_info);

        // Update table_indexes mapping
        let mut table_indexes = self.table_indexes.lock().unwrap();
        table_indexes.entry(table_id).or_insert_with(Vec::new).push(name.to_string());

        Ok(index_id)
    }

    /// Create primary key index
    pub fn create_primary_key_index(&self, table_id: u64, table_name: &str, columns: Vec<String>) -> Result<u64> {
        let index_name = format!("pk_{}", table_name);
        let index_id = {
            let mut next_id = self.next_index_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let index_def = IndexDef {
            index_id,
            name: index_name.clone(),
            table_id,
            columns: columns.clone(),
            index_type: IndexType::BTree,
            unique: true,
            primary_key: true,
            is_system_generated: true, // System-generated index for primary key
            root_page_id: None,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        let index_info = IndexInfo {
            def: index_def.clone(),
            stats: IndexStats {
                pages_used: 0,
                entries: 0,
                height: 0,
                last_analyzed: None,
            },
        };

        let mut indexes = self.indexes.lock().unwrap();
        indexes.insert(index_name, index_info);

        // Update table_indexes mapping
        let mut table_indexes = self.table_indexes.lock().unwrap();
        table_indexes.entry(table_id).or_insert_with(Vec::new).push(format!("pk_{}", table_name));

        Ok(index_id)
    }

    /// Drop an index
    pub fn drop_index(&self, name: &str) -> Result<()> {
        let mut indexes = self.indexes.lock().unwrap();

        if let Some(index_info) = indexes.remove(name) {
            // Update table_indexes mapping
            let mut table_indexes = self.table_indexes.lock().unwrap();
            if let Some(index_list) = table_indexes.get_mut(&index_info.def.table_id) {
                index_list.retain(|index_name| index_name != name);
                if index_list.is_empty() {
                    table_indexes.remove(&index_info.def.table_id);
                }
            }
        }

        Ok(())
    }

    /// Get index by name
    pub fn get_index(&self, name: &str) -> Result<Option<IndexInfo>> {
        let indexes = self.indexes.lock().unwrap();
        Ok(indexes.get(name).cloned())
    }

    /// Get index by ID
    pub fn get_index_by_id(&self, index_id: u64) -> Result<Option<IndexInfo>> {
        let indexes = self.indexes.lock().unwrap();
        for index_info in indexes.values() {
            if index_info.def.index_id == index_id {
                return Ok(Some(index_info.clone()));
            }
        }
        Ok(None)
    }

    /// List all indexes
    pub fn list_indexes(&self) -> Result<Vec<IndexInfo>> {
        let indexes = self.indexes.lock().unwrap();
        Ok(indexes.values().cloned().collect())
    }

    /// List indexes for a table
    pub fn list_table_indexes(&self, table_id: u64) -> Result<Vec<IndexInfo>> {
        let table_indexes = self.table_indexes.lock().unwrap();
        let indexes = self.indexes.lock().unwrap();

        if let Some(index_names) = table_indexes.get(&table_id) {
            let mut result = Vec::new();
            for index_name in index_names {
                if let Some(index_info) = indexes.get(index_name) {
                    result.push(index_info.clone());
                }
            }
            Ok(result)
        } else {
            Ok(vec![])
        }
    }

    /// Get primary key index for a table
    pub fn get_primary_key_index(&self, table_id: u64) -> Result<Option<IndexInfo>> {
        let table_indexes = self.table_indexes.lock().unwrap();
        let indexes = self.indexes.lock().unwrap();

        if let Some(index_names) = table_indexes.get(&table_id) {
            for index_name in index_names {
                if let Some(index_info) = indexes.get(index_name) {
                    if index_info.def.primary_key {
                        return Ok(Some(index_info.clone()));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Check if index exists
    pub fn index_exists(&self, name: &str) -> bool {
        let indexes = self.indexes.lock().unwrap();
        indexes.contains_key(name)
    }

    /// Update index statistics
    pub fn update_stats(&self, index_name: &str, stats: IndexStats) -> Result<()> {
        let mut indexes = self.indexes.lock().unwrap();
        if let Some(index_info) = indexes.get_mut(index_name) {
            index_info.stats = stats;
        }
        Ok(())
    }

    /// Get index statistics
    pub fn get_stats(&self, index_name: &str) -> Result<Option<IndexStats>> {
        let indexes = self.indexes.lock().unwrap();
        Ok(indexes.get(index_name).map(|info| info.stats.clone()))
    }
}
