//! Schema management

use crate::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Schema definition
#[derive(Debug, Clone)]
pub struct SchemaDef {
    pub schema_id: u64,
    pub name: String,
    pub owner_id: u64,
    pub created_at: std::time::SystemTime,
    pub modified_at: std::time::SystemTime,
}

/// Schema manager
#[derive(Debug)]
pub struct SchemaManager {
    schemas: Arc<Mutex<HashMap<String, SchemaDef>>>,
    next_schema_id: Arc<Mutex<u64>>,
}

impl SchemaManager {
    pub fn new() -> Self {
        let manager = Self {
            schemas: Arc::new(Mutex::new(HashMap::new())),
            next_schema_id: Arc::new(Mutex::new(1)),
        };

        // Initialize default schemas
        manager.initialize_schemas();
        manager
    }

    /// Initialize default system schemas
    fn initialize_schemas(&self) {
        let mut schemas = self.schemas.lock().unwrap();

        // Create system schema (pg_catalog)
        let system_schema = SchemaDef {
            schema_id: 0,
            name: "pg_catalog".to_string(),
            owner_id: 0,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };
        schemas.insert("pg_catalog".to_string(), system_schema);

        // Create public schema
        let public_schema = SchemaDef {
            schema_id: 1,
            name: "public".to_string(),
            owner_id: 1,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };
        schemas.insert("public".to_string(), public_schema);
    }

    /// Create a new schema
    pub fn create_schema(&self, name: &str, owner_id: u64) -> Result<u64> {
        let schema_id = {
            let mut next_id = self.next_schema_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let schema_def = SchemaDef {
            schema_id,
            name: name.to_string(),
            owner_id,
            created_at: std::time::SystemTime::now(),
            modified_at: std::time::SystemTime::now(),
        };

        let mut schemas = self.schemas.lock().unwrap();
        if schemas.contains_key(name) {
            return Err(crate::error::RustgreSQLError::AlreadyExists(format!("Schema '{}' already exists", name)));
        }

        schemas.insert(name.to_string(), schema_def);
        Ok(schema_id)
    }

    /// Drop a schema
    pub fn drop_schema(&self, name: &str, cascade: bool) -> Result<()> {
        let mut schemas = self.schemas.lock().unwrap();

        // Don't allow dropping system schemas
        if name == "pg_catalog" || (name == "public" && !cascade) {
            return Err(crate::error::RustgreSQLError::InvalidOperation(format!("Cannot drop system schema '{}'", name)));
        }

        schemas.remove(name);
        Ok(())
    }

    /// Get schema by name
    pub fn get_schema(&self, name: &str) -> Result<Option<SchemaDef>> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas.get(name).cloned())
    }

    /// Get schema by ID
    pub fn get_schema_by_id(&self, schema_id: u64) -> Result<Option<SchemaDef>> {
        let schemas = self.schemas.lock().unwrap();
        for schema in schemas.values() {
            if schema.schema_id == schema_id {
                return Ok(Some(schema.clone()));
            }
        }
        Ok(None)
    }

    /// List all schemas
    pub fn list_schemas(&self) -> Result<Vec<SchemaDef>> {
        let schemas = self.schemas.lock().unwrap();
        Ok(schemas.values().cloned().collect())
    }

    /// Get public schema ID
    pub fn get_public_schema_id(&self) -> u64 {
        1 // Hardcoded for now
    }

    /// Get system schema ID
    pub fn get_system_schema_id(&self) -> u64 {
        0 // Hardcoded for now
    }

    /// Check if schema exists
    pub fn schema_exists(&self, name: &str) -> bool {
        let schemas = self.schemas.lock().unwrap();
        schemas.contains_key(name)
    }
}
