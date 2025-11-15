//! Comprehensive unit tests for DDL execution operations
//!
//! Tests the complete DDL execution pipeline including:
//! - CREATE TABLE execution with all constraint types
//! - DROP TABLE execution with dependency checking
//! - ALTER TABLE operations (ADD/DROP columns and constraints)
//! - CREATE INDEX and DROP INDEX operations
//! - DDL WAL logging and schema evolution integration
//! - Error handling and edge cases

#[cfg(test)]
mod tests {
    use crate::executor::ExecutionEngine;
    use crate::sql::parser::parse_sql;
    use crate::storage::{SchemaEvolutionManager, TableSchema, ColumnSchema, ConstraintSchema, IndexSchema, ConstraintType, IndexType, ForeignKeyReference, ReferentialAction};
    use crate::transaction::ddl_wal::{init_ddl_wal_manager, get_ddl_wal_manager};
    use crate::transaction::wal::WALManager;
    use tempfile::tempdir;

    /// Create a test execution engine with temporary storage
    fn create_test_engine() -> ExecutionEngine {
        // Initialize WAL manager for DDL operations
        let temp_dir = tempdir().unwrap();
        let wal_path = temp_dir.path().join("test_wal");
        let wal_manager = WALManager::create(&wal_path, 8192).unwrap();
        init_ddl_wal_manager(wal_manager).unwrap();

        // Create execution engine
        ExecutionEngine::new()
    }

    #[test]
    fn test_create_table_basic() {
        let engine = create_test_engine();

        // Parse CREATE TABLE statement
        let sql = "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255) UNIQUE
        )";
        let statements = parse_sql(sql).unwrap();
        assert_eq!(statements.len(), 1);

        // Execute CREATE TABLE
        let result = engine.execute_query(&statements[0]);
        assert!(result.is_ok(), "CREATE TABLE execution failed: {:?}", result);

        // Verify we can parse and execute another CREATE TABLE
        let sql2 = "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name VARCHAR(200) NOT NULL,
            price INTEGER
        )";
        let statements2 = parse_sql(sql2).unwrap();
        let result2 = engine.execute_query(&statements2[0]);
        assert!(result2.is_ok(), "Second CREATE TABLE execution failed: {:?}", result2);
    }

    #[test]
    fn test_create_table_with_foreign_key() {
        let engine = create_test_engine();

        // First create users table
        let users_sql = "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL
        )";
        let users_statements = parse_sql(users_sql).unwrap();
        engine.execute_query(&users_statements[0]).unwrap();

        // Then create orders table with foreign key
        let orders_sql = "CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            total INTEGER,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )";
        let orders_statements = parse_sql(orders_sql).unwrap();
        let result = engine.execute_query(&orders_statements[0]);
        assert!(result.is_ok(), "CREATE TABLE with foreign key failed: {:?}", result);
    }

    #[test]
    fn test_create_table_error_foreign_key_to_nonexistent_table() {
        let engine = create_test_engine();

        // Try to create table with foreign key to nonexistent table
        let sql = "CREATE TABLE bad_fk (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            FOREIGN KEY (user_id) REFERENCES nonexistent_table(id)
        )";
        let statements = parse_sql(sql).unwrap();
        let result = engine.execute_query(&statements[0]);
        assert!(result.is_err(), "Expected error for foreign key to nonexistent table");

        match result.unwrap_err() {
            crate::error::RustgreSQLError::NotFound(msg) | crate::error::RustgreSQLError::InvalidOperation(msg) => {
                assert!(msg.contains("nonexistent_table") || msg.contains("user_id"));
            }
            _ => panic!("Expected NotFound or InvalidOperation error"),
        }
    }

    #[test]
    fn test_drop_table_basic() {
        let engine = create_test_engine();

        // First create a table
        let create_sql = "CREATE TABLE test_drop (id INTEGER PRIMARY KEY, name VARCHAR(100))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Drop the table
        let drop_sql = "DROP TABLE test_drop";
        let drop_statements = parse_sql(drop_sql).unwrap();
        let result = engine.execute_query(&drop_statements[0]);
        assert!(result.is_ok(), "DROP TABLE execution failed: {:?}", result);

        // Should be able to create it again after dropping
        let recreate_result = engine.execute_query(&create_statements[0]);
        assert!(recreate_result.is_ok(), "Should be able to recreate dropped table");
    }

    #[test]
    fn test_drop_table_if_exists() {
        let engine = create_test_engine();

        // Drop table that doesn't exist with IF EXISTS
        let drop_sql = "DROP TABLE IF EXISTS nonexistent_table";
        let drop_statements = parse_sql(drop_sql).unwrap();
        let result = engine.execute_query(&drop_statements[0]);
        assert!(result.is_ok(), "DROP TABLE IF EXISTS should succeed for nonexistent table");
    }

    #[test]
    fn test_drop_table_error_foreign_key_dependency() {
        let engine = create_test_engine();

        // Create users table
        let users_sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100))";
        let users_statements = parse_sql(users_sql).unwrap();
        engine.execute_query(&users_statements[0]).unwrap();

        // Create orders table with foreign key to users
        let orders_sql = "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, FOREIGN KEY (user_id) REFERENCES users(id))";
        let orders_statements = parse_sql(orders_sql).unwrap();
        engine.execute_query(&orders_statements[0]).unwrap();

        // Try to drop users table (should fail due to foreign key dependency)
        let drop_sql = "DROP TABLE users";
        let drop_statements = parse_sql(drop_sql).unwrap();
        let result = engine.execute_query(&drop_statements[0]);
        assert!(result.is_err(), "Expected error when dropping table with foreign key dependencies");
    }

    #[test]
    fn test_create_index_basic() {
        let engine = create_test_engine();

        // First create a table
        let create_sql = "CREATE TABLE test_index (id INTEGER PRIMARY KEY, email VARCHAR(255), name VARCHAR(100))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Create an index
        let index_sql = "CREATE INDEX idx_test_email ON test_index (email)";
        let index_statements = parse_sql(index_sql).unwrap();
        let result = engine.execute_query(&index_statements[0]);
        assert!(result.is_ok(), "CREATE INDEX execution failed: {:?}", result);

        // Should be able to drop the index
        let drop_sql = "DROP INDEX idx_test_email";
        let drop_statements = parse_sql(drop_sql).unwrap();
        let drop_result = engine.execute_query(&drop_statements[0]);
        assert!(drop_result.is_ok(), "DROP INDEX should succeed");
    }

    #[test]
    fn test_create_unique_index() {
        let engine = create_test_engine();

        // Create table
        let create_sql = "CREATE TABLE test_unique (id INTEGER PRIMARY KEY, email VARCHAR(255))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Create unique index
        let index_sql = "CREATE UNIQUE INDEX idx_test_email_unique ON test_unique (email)";
        let index_statements = parse_sql(index_sql).unwrap();
        let result = engine.execute_query(&index_statements[0]);
        assert!(result.is_ok(), "CREATE UNIQUE INDEX execution failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_add_column() {
        let engine = create_test_engine();

        // Create initial table
        let create_sql = "CREATE TABLE alter_test (id INTEGER PRIMARY KEY, name VARCHAR(100))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Add column
        let alter_sql = "ALTER TABLE alter_test ADD COLUMN email VARCHAR(255) UNIQUE";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE ADD COLUMN execution failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_add_column_with_default() {
        let engine = create_test_engine();

        // Create initial table
        let create_sql = "CREATE TABLE alter_default (id INTEGER PRIMARY KEY, name VARCHAR(100))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Add column with default value
        let alter_sql = "ALTER TABLE alter_default ADD COLUMN status VARCHAR(50) DEFAULT 'active'";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE ADD COLUMN with DEFAULT failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_drop_column() {
        let engine = create_test_engine();

        // Create table with multiple columns
        let create_sql = "CREATE TABLE drop_column_test (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100),
            email VARCHAR(255),
            phone VARCHAR(20)
        )";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Drop column
        let alter_sql = "ALTER TABLE drop_column_test DROP COLUMN phone";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE DROP COLUMN execution failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_add_constraint_unique() {
        let engine = create_test_engine();

        // Create table
        let create_sql = "CREATE TABLE constraint_test (id INTEGER PRIMARY KEY, email VARCHAR(255))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Add unique constraint
        let alter_sql = "ALTER TABLE constraint_test ADD CONSTRAINT uk_email UNIQUE (email)";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE ADD UNIQUE constraint failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_add_constraint_check() {
        let engine = create_test_engine();

        // Create table
        let create_sql = "CREATE TABLE check_test (id INTEGER PRIMARY KEY, age INTEGER, name VARCHAR(100))";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Add check constraint
        let alter_sql = "ALTER TABLE check_test ADD CONSTRAINT chk_age_positive CHECK (age > 0)";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE ADD CHECK constraint failed: {:?}", result);
    }

    #[test]
    fn test_alter_table_drop_constraint() {
        let engine = create_test_engine();

        // Create table with constraint
        let create_sql = "CREATE TABLE drop_constraint_test (
            id INTEGER PRIMARY KEY,
            email VARCHAR(255),
            CONSTRAINT uk_email UNIQUE (email)
        )";
        let create_statements = parse_sql(create_sql).unwrap();
        engine.execute_query(&create_statements[0]).unwrap();

        // Drop constraint
        let alter_sql = "ALTER TABLE drop_constraint_test DROP CONSTRAINT uk_email";
        let alter_statements = parse_sql(alter_sql).unwrap();
        let result = engine.execute_query(&alter_statements[0]);
        assert!(result.is_ok(), "ALTER TABLE DROP CONSTRAINT execution failed: {:?}", result);
    }

    #[test]
    fn test_ddl_wal_logging() {
        let engine = create_test_engine();

        // Execute CREATE TABLE
        let sql = "CREATE TABLE wal_test (id INTEGER PRIMARY KEY, name VARCHAR(100))";
        let statements = parse_sql(sql).unwrap();
        engine.execute_query(&statements[0]).unwrap();

        // Verify WAL was logged
        if let Some(ddl_wal) = get_ddl_wal_manager() {
            // Verify schema manager has the table
            let schema_manager = ddl_wal.get_schema_manager();
            let schema = schema_manager.lock().unwrap();

            // The table should be registered in schema evolution
            let version = schema.get_current_version("wal_test");
            assert!(version.is_some(), "Schema version should be set for created table");
        } else {
            panic!("DDL WAL manager should be initialized");
        }
    }

    #[test]
    fn test_schema_evolution_version_tracking() {
        let mut schema_manager = SchemaEvolutionManager::new();

        // Test initial state
        assert_eq!(schema_manager.get_current_version("test_table"), None);

        // Set version
        schema_manager.set_current_version("test_table".to_string(), 1);
        assert_eq!(schema_manager.get_current_version("test_table"), Some(1));

        // Increment version through table schema
        let mut table_schema = TableSchema::new("test_table".to_string(), "public".to_string());
        table_schema.increment_version();
        schema_manager.set_current_version("test_table".to_string(), table_schema.current_version);

        assert_eq!(schema_manager.get_current_version("test_table"), Some(2));
    }

    #[test]
    fn test_table_schema_add_column() {
        let mut table_schema = TableSchema::new("test".to_string(), "public".to_string());

        let column = ColumnSchema {
            name: "id".to_string(),
            data_type: "INTEGER".to_string(),
            nullable: false,
            default_value: None,
            position: 0,
            is_primary_key: true,
            is_unique: false,
            foreign_key: None,
        };

        // Add column
        table_schema.add_column(column.clone()).unwrap();
        assert_eq!(table_schema.columns.len(), 1);
        assert_eq!(table_schema.current_version, 1);

        // Try to add duplicate column
        let result = table_schema.add_column(column);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::RustgreSQLError::AlreadyExists(_)));
    }

    #[test]
    fn test_table_schema_add_constraint() {
        let mut table_schema = TableSchema::new("test".to_string(), "public".to_string());

        let constraint = ConstraintSchema {
            name: "pk_test".to_string(),
            constraint_type: ConstraintType::PrimaryKey,
            columns: vec!["id".to_string()],
            definition: None,
            deferrable: false,
            initially_deferred: false,
        };

        // Add constraint
        table_schema.add_constraint(constraint.clone()).unwrap();
        assert_eq!(table_schema.constraints.len(), 1);

        // Try to add duplicate constraint
        let result = table_schema.add_constraint(constraint);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::RustgreSQLError::AlreadyExists(_)));
    }

    #[test]
    fn test_table_schema_add_index() {
        let mut table_schema = TableSchema::new("test".to_string(), "public".to_string());

        let index = IndexSchema {
            name: "idx_test_name".to_string(),
            columns: vec!["name".to_string()],
            unique: false,
            index_type: IndexType::BTree,
            pages: vec![],
            statistics: Default::default(),
        };

        // Add index
        table_schema.add_index(index.clone()).unwrap();
        assert_eq!(table_schema.indexes.len(), 1);

        // Try to add duplicate index
        let result = table_schema.add_index(index);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::RustgreSQLError::AlreadyExists(_)));
    }

    #[test]
    fn test_foreign_key_reference_actions() {
        let fk_ref = ForeignKeyReference {
            referenced_table: "users".to_string(),
            referenced_column: "id".to_string(),
            referencing_column: "user_id".to_string(),
            on_delete: ReferentialAction::Cascade,
            on_update: ReferentialAction::Restrict,
        };

        assert_eq!(fk_ref.referenced_table, "users");
        assert_eq!(fk_ref.on_delete, ReferentialAction::Cascade);
        assert_eq!(fk_ref.on_update, ReferentialAction::Restrict);
    }

    #[test]
    fn test_comprehensive_ddl_workflow() {
        let engine = create_test_engine();

        // 1. Create users table
        let users_sql = "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            username VARCHAR(50) UNIQUE NOT NULL,
            email VARCHAR(255) UNIQUE,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )";
        let users_statements = parse_sql(users_sql).unwrap();
        engine.execute_query(&users_statements[0]).unwrap();

        // 2. Create posts table with foreign key
        let posts_sql = "CREATE TABLE posts (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            title VARCHAR(200) NOT NULL,
            content TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )";
        let posts_statements = parse_sql(posts_sql).unwrap();
        engine.execute_query(&posts_statements[0]).unwrap();

        // 3. Create index on posts title
        let title_index_sql = "CREATE INDEX idx_posts_title ON posts (title)";
        let title_index_statements = parse_sql(title_index_sql).unwrap();
        engine.execute_query(&title_index_statements[0]).unwrap();

        // 4. Add column to posts table
        let alter_sql = "ALTER TABLE posts ADD COLUMN status VARCHAR(20) DEFAULT 'draft'";
        let alter_statements = parse_sql(alter_sql).unwrap();
        engine.execute_query(&alter_statements[0]).unwrap();

        // 5. Add check constraint
        let check_sql = "ALTER TABLE posts ADD CONSTRAINT chk_status CHECK (status IN ('draft', 'published', 'archived'))";
        let check_statements = parse_sql(check_sql).unwrap();
        engine.execute_query(&check_statements[0]).unwrap();

        // 6. Clean up - drop posts table first (due to foreign key)
        let drop_posts_sql = "DROP TABLE posts";
        let drop_posts_statements = parse_sql(drop_posts_sql).unwrap();
        engine.execute_query(&drop_posts_statements[0]).unwrap();

        // 7. Then drop users table
        let drop_users_sql = "DROP TABLE users";
        let drop_users_statements = parse_sql(drop_users_sql).unwrap();
        engine.execute_query(&drop_users_statements[0]).unwrap();
    }

    #[test]
    fn test_index_types_and_properties() {
        let mut table_schema = TableSchema::new("test".to_string(), "public".to_string());

        // Test different index types
        let btree_index = IndexSchema {
            name: "idx_btree".to_string(),
            columns: vec!["id".to_string()],
            unique: false,
            index_type: IndexType::BTree,
            pages: vec![1, 2, 3],
            statistics: Default::default(),
        };

        let hash_index = IndexSchema {
            name: "idx_hash".to_string(),
            columns: vec!["hash_col".to_string()],
            unique: true,
            index_type: IndexType::Hash,
            pages: vec![4, 5],
            statistics: Default::default(),
        };

        // Add indexes
        table_schema.add_index(btree_index).unwrap();
        table_schema.add_index(hash_index).unwrap();

        // Verify index properties
        let btree_idx = table_schema.get_index("idx_btree").unwrap();
        assert_eq!(btree_idx.index_type, IndexType::BTree);
        assert!(!btree_idx.unique);
        assert_eq!(btree_idx.pages, vec![1, 2, 3]);

        let hash_idx = table_schema.get_index("idx_hash").unwrap();
        assert_eq!(hash_idx.index_type, IndexType::Hash);
        assert!(hash_idx.unique);
        assert_eq!(hash_idx.pages, vec![4, 5]);
    }

    #[test]
    fn test_column_schema_properties() {
        let column = ColumnSchema {
            name: "user_id".to_string(),
            data_type: "INTEGER".to_string(),
            nullable: false,
            default_value: None,
            position: 1,
            is_primary_key: false,
            is_unique: false,
            foreign_key: Some(ForeignKeyReference {
                referenced_table: "users".to_string(),
                referenced_column: "id".to_string(),
                referencing_column: "user_id".to_string(),
                on_delete: ReferentialAction::Cascade,
                on_update: ReferentialAction::Restrict,
            }),
        };

        assert_eq!(column.name, "user_id");
        assert_eq!(column.data_type, "INTEGER");
        assert!(!column.nullable);
        assert!(!column.is_primary_key);
        assert!(column.foreign_key.is_some());

        let fk_ref = column.foreign_key.unwrap();
        assert_eq!(fk_ref.referenced_table, "users");
        assert_eq!(fk_ref.on_delete, ReferentialAction::Cascade);
    }

    #[test]
    fn test_constraint_schema_properties() {
        let constraint = ConstraintSchema {
            name: "fk_orders_users".to_string(),
            constraint_type: ConstraintType::ForeignKey,
            columns: vec!["user_id".to_string()],
            definition: Some("FOREIGN KEY (user_id) REFERENCES users(id)".to_string()),
            deferrable: true,
            initially_deferred: false,
        };

        assert_eq!(constraint.name, "fk_orders_users");
        assert_eq!(constraint.constraint_type, ConstraintType::ForeignKey);
        assert_eq!(constraint.columns, vec!["user_id"]);
        assert!(constraint.deferrable);
        assert!(!constraint.initially_deferred);
    }

    #[test]
    fn test_schema_evolution_migration_queue() {
        let mut schema_manager = SchemaEvolutionManager::new();

        // Test empty migration queue
        assert!(schema_manager.next_migration().is_none());

        // Test version checking
        assert!(schema_manager.needs_migration(3, 1));
        assert!(!schema_manager.needs_migration(1, 1));
        assert!(!schema_manager.needs_migration(1, 3));
    }
}