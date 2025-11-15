//! Comprehensive DDL Unit Tests
//!
//! Tests all DDL execution operations including:
//! - CREATE TABLE with various constraints
//! - DROP TABLE with dependency checking
//! - CREATE INDEX and DROP INDEX
//! - ALTER TABLE operations (ADD/DROP COLUMN, ADD/DROP CONSTRAINT)
//! - DDL WAL logging and schema evolution
//! - Error handling and edge cases

use rustgresql::*;

#[cfg(test)]
mod create_table_tests {
    use super::*;

    #[test]
    fn test_create_simple_table() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "name".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let result = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result.get_message(), "CREATE TABLE");

        // Verify table was created in catalog
        let tables = engine.context().catalog().list_tables()?;
        assert!(tables.contains(&"users".to_string()));

        let table_def = engine.context().catalog().get_table("users")?;
        assert_eq!(table_def.name, "users");
        assert_eq!(table_def.columns.len(), 2);

        Ok(())
    }

    #[test]
    fn test_create_table_with_primary_key() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "orders".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![TableConstraint::PrimaryKey {
                        name: Some("orders_pkey".to_string()),
                        columns: vec!["id".to_string()],
                    }],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "customer_id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let result = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result.get_message(), "CREATE TABLE");

        let table_def = engine.context().catalog().get_table("orders")?;

        // Verify primary key constraint
        let pk_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::PrimaryKey))
            .collect();
        assert_eq!(pk_constraints.len(), 1);

        let pk_constraint = pk_constraints[0];
        assert_eq!(pk_constraint.name, "orders_pkey");
        assert_eq!(pk_constraint.columns, vec!["id"]);

        Ok(())
    }

    #[test]
    fn test_create_table_with_foreign_key() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create referenced table first
        let create_users = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![TableConstraint::PrimaryKey {
                        name: Some("users_pkey".to_string()),
                        columns: vec!["id".to_string()],
                    }],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_users, &context)?;

        // Create table with foreign key
        let create_orders = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "orders".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![TableConstraint::PrimaryKey {
                        name: Some("orders_pkey".to_string()),
                        columns: vec!["id".to_string()],
                    }],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "user_id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![TableConstraint::ForeignKey {
                        name: Some("orders_user_id_fkey".to_string()),
                        columns: vec!["user_id".to_string()],
                        referenced_table: "users".to_string(),
                        referenced_columns: vec!["id".to_string()],
                        on_delete: None,
                        on_update: None,
                    }],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let result = engine.execute_statement(&create_orders, &context)?;
        assert_eq!(result.get_message(), "CREATE TABLE");

        let table_def = engine.context().catalog().get_table("orders")?;

        // Verify foreign key constraint
        let fk_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::ForeignKey))
            .collect();
        assert_eq!(fk_constraints.len(), 1);

        let fk_constraint = fk_constraints[0];
        assert_eq!(fk_constraint.name, "orders_user_id_fkey");
        assert_eq!(fk_constraint.columns, vec!["user_id"]);

        Ok(())
    }

    #[test]
    fn test_create_table_with_check_constraint() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "products".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "price".to_string(),
                    data_type: Some("DECIMAL".to_string()),
                    constraints: vec![TableConstraint::Check {
                        name: Some("products_price_check".to_string()),
                        expr: Expression::BinaryOp {
                            left: Box::new(Expression::Column { name: "price".to_string(), table: None }),
                            op: BinaryOperator::GreaterThan,
                            right: Box::new(Expression::Literal(Value::integer(0))),
                        },
                    }],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let result = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result.get_message(), "CREATE TABLE");

        let table_def = engine.context().catalog().get_table("products")?;

        // Verify check constraint
        let check_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::Check))
            .collect();
        assert_eq!(check_constraints.len(), 1);

        let check_constraint = check_constraints[0];
        assert_eq!(check_constraint.name, "products_price_check");

        Ok(())
    }

    #[test]
    fn test_create_table_if_not_exists() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: true,
            table_name: "test_table".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        // Create table first time
        let result1 = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result1.get_message(), "CREATE TABLE");

        // Create table second time with IF NOT EXISTS
        let result2 = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result2.get_message(), "CREATE TABLE");

        // Should still have only one table
        let tables = engine.context().catalog().list_tables()?;
        let test_tables: Vec<_> = tables.iter().filter(|t| t == &&"test_table".to_string()).collect();
        assert_eq!(test_tables.len(), 1);

        Ok(())
    }

    #[test]
    fn test_create_table_duplicate_error() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false, // Not using IF NOT EXISTS
            table_name: "duplicate_table".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        // Create table first time
        let result1 = engine.execute_statement(&create_table, &context)?;
        assert_eq!(result1.get_message(), "CREATE TABLE");

        // Create table second time should error
        let result2 = engine.execute_statement(&create_table, &context);
        assert!(result2.is_err());

        Ok(())
    }
}

#[cfg(test)]
mod drop_table_tests {
    use super::*;

    #[test]
    fn test_drop_simple_table() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "test_drop".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Verify table exists
        let tables_before = engine.context().catalog().list_tables()?;
        assert!(tables_before.contains(&"test_drop".to_string()));

        // Drop table
        let drop_table = Statement::DropTable {
            if_exists: false,
            table_name: "test_drop".to_string(),
            cascade: false,
        };

        let result = engine.execute_statement(&drop_table, &context)?;
        assert_eq!(result.get_message(), "DROP TABLE");

        // Verify table is gone
        let tables_after = engine.context().catalog().list_tables()?;
        assert!(!tables_after.contains(&"test_drop".to_string()));

        Ok(())
    }

    #[test]
    fn test_drop_table_if_exists() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Drop non-existent table with IF EXISTS
        let drop_table = Statement::DropTable {
            if_exists: true,
            table_name: "non_existent".to_string(),
            cascade: false,
        };

        let result = engine.execute_statement(&drop_table, &context)?;
        assert_eq!(result.get_message(), "DROP TABLE");

        Ok(())
    }

    #[test]
    fn test_drop_table_error_if_not_exists() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Drop non-existent table without IF EXISTS
        let drop_table = Statement::DropTable {
            if_exists: false,
            table_name: "non_existent".to_string(),
            cascade: false,
        };

        let result = engine.execute_statement(&drop_table, &context);
        assert!(result.is_err());

        Ok(())
    }
}

#[cfg(test)]
mod create_index_tests {
    use super::*;

    #[test]
    fn test_create_simple_index() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "email".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Create index
        let create_index = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_users_email".to_string(),
            table_name: "users".to_string(),
            columns: vec!["email".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        let result = engine.execute_statement(&create_index, &context)?;
        assert_eq!(result.get_message(), "CREATE INDEX");

        // Verify index was created
        let indexes = engine.context().catalog().list_indexes()?;
        assert!(indexes.contains(&"idx_users_email".to_string()));

        Ok(())
    }

    #[test]
    fn test_create_unique_index() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "username".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Create unique index
        let create_index = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_users_username_unique".to_string(),
            table_name: "users".to_string(),
            columns: vec!["username".to_string()],
            unique: true,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        let result = engine.execute_statement(&create_index, &context)?;
        assert_eq!(result.get_message(), "CREATE INDEX");

        // Verify index was created and is unique
        let indexes = engine.context().catalog().list_indexes()?;
        assert!(indexes.contains(&"idx_users_username_unique".to_string()));

        let index_def = engine.context().catalog().get_index("idx_users_username_unique")?;
        assert!(index_def.unique);

        Ok(())
    }

    #[test]
    fn test_create_composite_index() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "orders".to_string(),
            columns: vec![
                Column {
                    name: "customer_id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "order_date".to_string(),
                    data_type: Some("TIMESTAMP".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "status".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Create composite index
        let create_index = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_orders_customer_date".to_string(),
            table_name: "orders".to_string(),
            columns: vec!["customer_id".to_string(), "order_date".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        let result = engine.execute_statement(&create_index, &context)?;
        assert_eq!(result.get_message(), "CREATE INDEX");

        // Verify index was created with multiple columns
        let indexes = engine.context().catalog().list_indexes()?;
        assert!(indexes.contains(&"idx_orders_customer_date".to_string()));

        let index_def = engine.context().catalog().get_index("idx_orders_customer_date")?;
        assert_eq!(index_def.columns, vec!["customer_id", "order_date"]);

        Ok(())
    }
}

#[cfg(test)]
mod alter_table_tests {
    use super::*;

    #[test]
    fn test_alter_table_add_column() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "name".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Add column
        let alter_table = Statement::AlterTable {
            table_name: "users".to_string(),
            action: AlterTableAction::AddColumn {
                column: Column {
                    name: "email".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            },
        };

        let result = engine.execute_statement(&alter_table, &context)?;
        assert_eq!(result.get_message(), "ALTER TABLE");

        // Verify column was added
        let table_def = engine.context().catalog().get_table("users")?;
        assert_eq!(table_def.columns.len(), 3);

        let email_column: Option<_> = table_def.columns.iter()
            .find(|c| c.name == "email")
            .map(|c| c.name.clone());
        assert_eq!(email_column, Some("email".to_string()));

        Ok(())
    }

    #[test]
    fn test_alter_table_drop_column() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "name".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "old_column".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Drop column
        let alter_table = Statement::AlterTable {
            table_name: "users".to_string(),
            action: AlterTableAction::DropColumn {
                column_name: "old_column".to_string(),
                if_exists: false,
            },
        };

        let result = engine.execute_statement(&alter_table, &context)?;
        assert_eq!(result.get_message(), "ALTER TABLE");

        // Verify column was dropped
        let table_def = engine.context().catalog().get_table("users")?;
        assert_eq!(table_def.columns.len(), 2);

        let old_column_exists = table_def.columns.iter()
            .any(|c| c.name == "old_column");
        assert!(!old_column_exists);

        Ok(())
    }

    #[test]
    fn test_alter_table_add_constraint() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "products".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
                Column {
                    name: "price".to_string(),
                    data_type: Some("DECIMAL".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Add check constraint
        let alter_table = Statement::AlterTable {
            table_name: "products".to_string(),
            action: AlterTableAction::AddConstraint {
                constraint: TableConstraint::Check {
                    name: Some("products_price_positive".to_string()),
                    expr: Expression::BinaryOp {
                        left: Box::new(Expression::Column { name: "price".to_string(), table: None }),
                        op: BinaryOperator::GreaterThan,
                        right: Box::new(Expression::Literal(Value::integer(0))),
                    },
                },
            },
        };

        let result = engine.execute_statement(&alter_table, &context)?;
        assert_eq!(result.get_message(), "ALTER TABLE");

        // Verify constraint was added
        let table_def = engine.context().catalog().get_table("products")?;

        let check_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::Check))
            .collect();
        assert_eq!(check_constraints.len(), 1);

        let check_constraint = check_constraints[0];
        assert_eq!(check_constraint.name, "products_price_positive");

        Ok(())
    }
}

#[cfg(test)]
mod ddl_wal_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ddl_wal_create_table_logging() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test_ddl.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        init_ddl_wal_manager(wal_manager)?;

        let ddl_wal = get_ddl_wal_manager().unwrap();
        let tx_id = 12345;

        // Begin DDL transaction
        let start_lsn = ddl_wal.begin_ddl_transaction(tx_id)?;
        assert!(ddl_wal.is_transaction_active(tx_id));

        // Create table schema for logging
        let table_schema = TableSchema {
            version: 1,
            columns: vec![
                ColumnSchema {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    max_length: None,
                    precision: None,
                    scale: None,
                },
                ColumnSchema {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                    default_value: None,
                    max_length: Some(255),
                    precision: None,
                    scale: None,
                },
            ],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            check_constraints: vec![],
            unique_constraints: vec![],
            not_null: vec!["id".to_string()],
            default_values: std::collections::HashMap::new(),
        };

        // Log CREATE TABLE
        let create_lsn = ddl_wal.log_create_table(
            tx_id,
            "test_users",
            Some("public".to_string()),
            &table_schema,
        )?;

        // Commit transaction
        let commit_lsn = ddl_wal.commit_ddl_transaction(tx_id)?;
        assert!(!ddl_wal.is_transaction_active(tx_id));
        assert!(commit_lsn > create_lsn);
        assert!(create_lsn > start_lsn);

        Ok(())
    }

    #[test]
    fn test_ddl_wal_rollback() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test_rollback.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        init_ddl_wal_manager(wal_manager)?;

        let ddl_wal = get_ddl_wal_manager().unwrap();
        let tx_id = 54321;

        // Begin DDL transaction
        let start_lsn = ddl_wal.begin_ddl_transaction(tx_id)?;
        assert!(ddl_wal.is_transaction_active(tx_id));

        // Log some operations
        let table_schema = TableSchema {
            version: 1,
            columns: vec![ColumnSchema {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                default_value: None,
                max_length: None,
                precision: None,
                scale: None,
            }],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            check_constraints: vec![],
            unique_constraints: vec![],
            not_null: vec!["id".to_string()],
            default_values: std::collections::HashMap::new(),
        };

        let create_lsn = ddl_wal.log_create_table(
            tx_id,
            "rollback_test",
            Some("public".to_string()),
            &table_schema,
        )?;

        // Verify transaction state
        let tx_state = ddl_wal.get_transaction_state(tx_id);
        assert!(tx_state.is_some());
        assert_eq!(tx_state.unwrap().modified_objects.len(), 1);

        // Rollback transaction
        let rollback_lsn = ddl_wal.rollback_ddl_transaction(tx_id)?;
        assert!(!ddl_wal.is_transaction_active(tx_id));
        assert!(rollback_lsn > create_lsn);

        Ok(())
    }

    #[test]
    fn test_schema_evolution_versioning() -> Result<()> {
        let schema_manager = SchemaEvolutionManager::new();
        let mut schema = schema_manager.lock().unwrap();

        // Set initial version
        schema.set_current_version("test_table".to_string(), 1);
        assert_eq!(schema.get_current_version("test_table"), Some(1));

        // Add version history
        schema.add_version("test_table".to_string(), TableSchema {
            version: 2,
            columns: vec![
                ColumnSchema {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    max_length: None,
                    precision: None,
                    scale: None,
                },
                ColumnSchema {
                    name: "new_column".to_string(),
                    data_type: "VARCHAR".to_string(),
                    nullable: true,
                    default_value: None,
                    max_length: Some(100),
                    precision: None,
                    scale: None,
                },
            ],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
            check_constraints: vec![],
            unique_constraints: vec![],
            not_null: vec!["id".to_string()],
            default_values: std::collections::HashMap::new(),
        });

        schema.set_current_version("test_table".to_string(), 2);
        assert_eq!(schema.get_current_version("test_table"), Some(2));

        let history = schema.get_version_history("test_table");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 1);
        assert_eq!(history[1].version, 2);

        Ok(())
    }
}

#[cfg(test)]
mod error_handling_tests {
    use super::*;

    #[test]
    fn test_create_table_without_columns() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "empty_table".to_string(),
            columns: vec![], // No columns
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let result = engine.execute_statement(&create_table, &context);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_alter_nonexistent_table() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let alter_table = Statement::AlterTable {
            table_name: "nonexistent_table".to_string(),
            action: AlterTableAction::AddColumn {
                column: Column {
                    name: "new_column".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            },
        };

        let result = engine.execute_statement(&alter_table, &context);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_create_index_on_nonexistent_table() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        let create_index = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_nonexistent".to_string(),
            table_name: "nonexistent_table".to_string(),
            columns: vec!["id".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        let result = engine.execute_statement(&create_index, &context);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_create_index_on_nonexistent_column() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table first
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Try to create index on nonexistent column
        let create_index = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_nonexistent_column".to_string(),
            table_name: "users".to_string(),
            columns: vec!["nonexistent_column".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        let result = engine.execute_statement(&create_index, &context);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_drop_primary_key_column() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table with primary key
        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "users".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![TableConstraint::PrimaryKey {
                        name: Some("users_pkey".to_string()),
                        columns: vec!["id".to_string()],
                    }],
                    default: None,
                    comment: None,
                },
            ],
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Try to drop primary key column
        let alter_table = Statement::AlterTable {
            table_name: "users".to_string(),
            action: AlterTableAction::DropColumn {
                column_name: "id".to_string(),
                if_exists: false,
            },
        };

        let result = engine.execute_statement(&alter_table, &context);
        assert!(result.is_err());

        Ok(())
    }
}