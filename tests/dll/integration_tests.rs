//! DDL Integration Tests
//!
//! End-to-end tests for DDL workflows, error scenarios, and complex interactions.
//! Tests comprehensive DDL operation sequences, transaction handling, and
//! integration with other database components.

use rustgresql::*;

#[cfg(test)]
mod complex_ddl_workflows {
    use super::*;

    #[test]
    fn test_complex_table_creation_workflow() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create a complex table with multiple constraints
        let create_orders = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "orders".to_string(),
            columns: vec![
                Column {
                    name: "id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![
                        TableConstraint::PrimaryKey {
                            name: Some("orders_pkey".to_string()),
                            columns: vec!["id".to_string()],
                        },
                        TableConstraint::Check {
                            name: Some("orders_id_check".to_string()),
                            expr: Expression::BinaryOp {
                                left: Box::new(Expression::Column { name: "id".to_string(), table: None }),
                                op: BinaryOperator::GreaterThan,
                                right: Box::new(Expression::Literal(Value::integer(0))),
                            },
                        },
                    ],
                    default: Some(Expression::Literal(Value::integer(1))),
                    comment: Some("Order ID".to_string()),
                },
                Column {
                    name: "customer_id".to_string(),
                    data_type: Some("INTEGER".to_string()),
                    constraints: vec![],
                    default: None,
                    comment: Some("Customer reference".to_string()),
                },
                Column {
                    name: "order_date".to_string(),
                    data_type: Some("TIMESTAMP".to_string()),
                    constraints: vec![
                        TableConstraint::Check {
                            name: Some("orders_date_check".to_string()),
                            expr: Expression::BinaryOp {
                                left: Box::new(Expression::Column { name: "order_date".to_string(), table: None }),
                                op: BinaryOperator::LessThanOrEquals,
                                right: Box::new(Expression::Literal(Value::string("NOW()"))),
                            },
                        },
                    ],
                    default: Some(Expression::Literal(Value::string("CURRENT_TIMESTAMP"))),
                    comment: Some("When order was placed".to_string()),
                },
                Column {
                    name: "total_amount".to_string(),
                    data_type: Some("DECIMAL".to_string()),
                    constraints: vec![
                        TableConstraint::Check {
                            name: Some("orders_amount_check".to_string()),
                            expr: Expression::BinaryOp {
                                left: Box::new(Expression::Column { name: "total_amount".to_string(), table: None }),
                                op: BinaryOperator::GreaterThanOrEquals,
                                right: Box::new(Expression::Literal(Value::float(0.0))),
                            },
                        },
                    ],
                    default: Some(Expression::Literal(Value::float(0.0))),
                    comment: Some("Total order amount".to_string()),
                },
                Column {
                    name: "status".to_string(),
                    data_type: Some("VARCHAR".to_string()),
                    constraints: vec![
                        TableConstraint::Check {
                            name: Some("orders_status_check".to_string()),
                            expr: Expression::BinaryOp {
                                left: Box::new(Expression::Column { name: "status".to_string(), table: None }),
                                op: BinaryOperator::In,
                                right: Box::new(Expression::Literal(Value::list(vec![
                                    Value::string("pending"),
                                    Value::string("processing"),
                                    Value::string("shipped"),
                                    Value::string("delivered"),
                                    Value::string("cancelled"),
                                ]))),
                            },
                        },
                    ],
                    default: Some(Expression::Literal(Value::string("pending"))),
                    comment: Some("Order status".to_string()),
                },
            ],
            constraints: vec![
                TableConstraint::ForeignKey {
                    name: Some("orders_customer_id_fkey".to_string()),
                    columns: vec!["customer_id".to_string()],
                    referenced_table: "customers".to_string(),
                    referenced_columns: vec!["id".to_string()],
                    on_delete: Some("RESTRICT".to_string()),
                    on_update: Some("CASCADE".to_string()),
                },
                TableConstraint::Unique {
                    name: Some("orders_customer_date_unique".to_string()),
                    columns: vec!["customer_id".to_string(), "order_date".to_string()],
                },
            ],
            with_options: vec![],
            table_space: Some("orders_tablespace".to_string()),
        });

        let result = engine.execute_statement(&create_orders, &context)?;
        assert_eq!(result.get_message(), "CREATE TABLE");

        // Verify table structure
        let table_def = engine.context().catalog().get_table("orders")?;
        assert_eq!(table_def.name, "orders");
        assert_eq!(table_def.columns.len(), 5);

        // Verify primary key constraint
        let pk_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::PrimaryKey))
            .collect();
        assert_eq!(pk_constraints.len(), 1);

        // Verify foreign key constraint
        let fk_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::ForeignKey))
            .collect();
        assert_eq!(fk_constraints.len(), 1);

        // Verify unique constraint
        let unique_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::Unique))
            .collect();
        assert_eq!(unique_constraints.len(), 1);

        // Verify check constraints (column-level + table-level)
        let check_constraints: Vec<_> = table_def.constraints
            .iter()
            .filter(|c| matches!(c.constraint_type, ConstraintType::Check))
            .collect();
        assert_eq!(check_constraints.len(), 4); // 3 column-level + 1 table-level

        Ok(())
    }

    #[test]
    fn test_table_evolution_workflow() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // 1. Create initial table
        let create_initial = Statement::CreateTable(CreateTable {
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

        engine.execute_statement(&create_initial, &context)?;

        // 2. Add email column
        let add_email = Statement::AlterTable {
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

        engine.execute_statement(&add_email, &context)?;

        // 3. Add email unique constraint
        let add_unique_email = Statement::AlterTable {
            table_name: "users".to_string(),
            action: AlterTableAction::AddConstraint {
                constraint: TableConstraint::Unique {
                    name: Some("users_email_unique".to_string()),
                    columns: vec!["email".to_string()],
                },
            },
        };

        engine.execute_statement(&add_unique_email, &context)?;

        // 4. Add created_at column with default
        let add_created_at = Statement::AlterTable {
            table_name: "users".to_string(),
            action: AlterTableAction::AddColumn {
                column: Column {
                    name: "created_at".to_string(),
                    data_type: Some("TIMESTAMP".to_string()),
                    constraints: vec![],
                    default: Some(Expression::Literal(Value::string("CURRENT_TIMESTAMP"))),
                    comment: None,
                },
            },
        };

        engine.execute_statement(&add_created_at, &context)?;

        // 5. Create index on username
        let create_username_idx = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_users_username".to_string(),
            table_name: "users".to_string(),
            columns: vec!["username".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        engine.execute_statement(&create_username_idx, &context)?;

        // Verify final table structure
        let table_def = engine.context().catalog().get_table("users")?;
        assert_eq!(table_def.columns.len(), 4);

        let column_names: Vec<_> = table_def.columns.iter()
            .map(|c| c.name.clone())
            .collect();
        assert!(column_names.contains(&"id".to_string()));
        assert!(column_names.contains(&"username".to_string()));
        assert!(column_names.contains(&"email".to_string()));
        assert!(column_names.contains(&"created_at".to_string()));

        // Verify constraints
        let constraints: Vec<_> = table_def.constraints.iter().collect();
        assert_eq!(constraints.len(), 2); // 1 primary key + 1 unique

        // Verify index
        let indexes = engine.context().catalog().list_indexes()?;
        assert!(indexes.contains(&"idx_users_username".to_string()));

        Ok(())
    }

    #[test]
    fn test_drop_table_with_dependencies_cascade() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // 1. Create users table (referenced)
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

        engine.execute_statement(&create_users, &context)?;

        // 2. Create orders table (references users)
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

        engine.execute_statement(&create_orders, &context)?;

        // 3. Create index on orders.user_id
        let create_order_user_idx = Statement::CreateIndex {
            if_not_exists: false,
            index_name: "idx_orders_user_id".to_string(),
            table_name: "orders".to_string(),
            columns: vec!["user_id".to_string()],
            unique: false,
            index_type: Some("btree".to_string()),
            with_options: vec![],
        };

        engine.execute_statement(&create_order_user_idx, &context)?;

        // Verify initial state
        let tables_before = engine.context().catalog().list_tables()?;
        assert!(tables_before.contains(&"users".to_string()));
        assert!(tables_before.contains(&"orders".to_string()));

        let indexes_before = engine.context().catalog().list_indexes()?;
        assert!(indexes_before.contains(&"idx_orders_user_id".to_string()));

        // 4. Try to drop users table (should fail due to dependency)
        let drop_users_no_cascade = Statement::DropTable {
            if_exists: false,
            table_name: "users".to_string(),
            cascade: false,
        };

        let result = engine.execute_statement(&drop_users_no_cascade, &context);
        assert!(result.is_err());

        // 5. Drop orders table first
        let drop_orders = Statement::DropTable {
            if_exists: false,
            table_name: "orders".to_string(),
            cascade: false,
        };

        engine.execute_statement(&drop_orders, &context)?;

        // 6. Now drop users table
        let drop_users = Statement::DropTable {
            if_exists: false,
            table_name: "users".to_string(),
            cascade: false,
        };

        engine.execute_statement(&drop_users, &context)?;

        // Verify final state
        let tables_after = engine.context().catalog().list_tables()?;
        assert!(!tables_after.contains(&"users".to_string()));
        assert!(!tables_after.contains(&"orders".to_string()));

        let indexes_after = engine.context().catalog().list_indexes()?;
        assert!(!indexes_after.contains(&"idx_orders_user_id".to_string()));

        Ok(())
    }
}

#[cfg(test)]
mod concurrent_ddl_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn test_concurrent_table_creation() -> Result<()> {
        let catalog = Arc::new(Mutex::new(Catalog::new()));
        let engine = Arc::new(Mutex::new(ExecutionEngine::new(Arc::clone(&catalog))));

        let mut handles = vec![];

        // Spawn multiple threads creating different tables
        for i in 0..5 {
            let engine_clone = Arc::clone(&engine);
            let handle = thread::spawn(move || -> Result<()> {
                let engine = engine_clone.lock().unwrap();
                let context = ExecutionContext::new();

                let table_name = format!("concurrent_table_{}", i);

                let create_table = Statement::CreateTable(CreateTable {
                    if_not_exists: false,
                    table_name: table_name.clone(),
                    columns: vec![
                        Column {
                            name: "id".to_string(),
                            data_type: Some("INTEGER".to_string()),
                            constraints: vec![TableConstraint::PrimaryKey {
                                name: Some(format!("{}_pkey", table_name)),
                                columns: vec!["id".to_string()],
                            }],
                            default: None,
                            comment: None,
                        },
                        Column {
                            name: "data".to_string(),
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
                Ok(())
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify all tables were created
        let catalog_guard = catalog.lock().unwrap();
        let tables = catalog_guard.list_tables()?;

        for i in 0..5 {
            let table_name = format!("concurrent_table_{}", i);
            assert!(tables.contains(&table_name), "Table {} was not created", table_name);
        }

        Ok(())
    }

    #[test]
    fn test_concurrent_index_creation() -> Result<()> {
        let catalog = Arc::new(Mutex::new(Catalog::new()));
        let engine = Arc::new(Mutex::new(ExecutionEngine::new(Arc::clone(&catalog))));

        // Create base table first
        {
            let engine = engine.lock().unwrap();
            let context = ExecutionContext::new();

            let create_table = Statement::CreateTable(CreateTable {
                if_not_exists: false,
                table_name: "test_table".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: Some("INTEGER".to_string()),
                        constraints: vec![TableConstraint::PrimaryKey {
                            name: Some("test_table_pkey".to_string()),
                            columns: vec!["id".to_string()],
                        }],
                        default: None,
                        comment: None,
                    },
                    Column {
                        name: "col1".to_string(),
                        data_type: Some("INTEGER".to_string()),
                        constraints: vec![],
                        default: None,
                        comment: None,
                    },
                    Column {
                        name: "col2".to_string(),
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
        }

        let mut handles = vec![];

        // Spawn multiple threads creating indexes on different columns
        for i in 0..3 {
            let engine_clone = Arc::clone(&engine);
            let handle = thread::spawn(move || -> Result<()> {
                let engine = engine_clone.lock().unwrap();
                let context = ExecutionContext::new();

                let column_name = format!("col{}", i + 1);
                let index_name = format!("idx_test_table_{}", column_name);

                let create_index = Statement::CreateIndex {
                    if_not_exists: false,
                    index_name: index_name.clone(),
                    table_name: "test_table".to_string(),
                    columns: vec![column_name],
                    unique: false,
                    index_type: Some("btree".to_string()),
                    with_options: vec![],
                };

                engine.execute_statement(&create_index, &context)?;
                Ok(())
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify all indexes were created
        let catalog_guard = catalog.lock().unwrap();
        let indexes = catalog_guard.list_indexes()?;

        for i in 1..=3 {
            let index_name = format!("idx_test_table_col{}", i);
            assert!(indexes.contains(&index_name), "Index {} was not created", index_name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_large_table_creation_performance() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create table with many columns
        let mut columns = vec![
            Column {
                name: "id".to_string(),
                data_type: Some("INTEGER".to_string()),
                constraints: vec![TableConstraint::PrimaryKey {
                    name: Some("large_table_pkey".to_string()),
                    columns: vec!["id".to_string()],
                }],
                default: None,
                comment: None,
            }
        ];

        // Add 100 columns
        for i in 0..100 {
            columns.push(Column {
                name: format!("col_{:03}", i),
                data_type: Some("VARCHAR".to_string()),
                constraints: vec![],
                default: None,
                comment: None,
            });
        }

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "large_table".to_string(),
            columns,
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        let start_time = Instant::now();
        let result = engine.execute_statement(&create_table, &context)?;
        let duration = start_time.elapsed();

        assert_eq!(result.get_message(), "CREATE TABLE");

        // Verify table was created with all columns
        let table_def = engine.context().catalog().get_table("large_table")?;
        assert_eq!(table_def.columns.len(), 101); // 1 id + 100 columns

        // Performance should be reasonable (less than 1 second for 101 columns)
        assert!(duration.as_secs() < 1, "Table creation took too long: {:?}", duration);

        Ok(())
    }

    #[test]
    fn test_many_indexes_performance() -> Result<()> {
        let catalog = Catalog::new();
        let engine = ExecutionEngine::new(catalog);
        let context = ExecutionContext::new();

        // Create base table with many columns
        let mut columns = vec![
            Column {
                name: "id".to_string(),
                data_type: Some("INTEGER".to_string()),
                constraints: vec![TableConstraint::PrimaryKey {
                    name: Some("index_test_pkey".to_string()),
                    columns: vec!["id".to_string()],
                }],
                default: None,
                comment: None,
            }
        ];

        for i in 0..50 {
            columns.push(Column {
                name: format!("col_{:03}", i),
                data_type: Some("INTEGER".to_string()),
                constraints: vec![],
                default: None,
                comment: None,
            });
        }

        let create_table = Statement::CreateTable(CreateTable {
            if_not_exists: false,
            table_name: "index_test_table".to_string(),
            columns,
            constraints: vec![],
            with_options: vec![],
            table_space: None,
        });

        engine.execute_statement(&create_table, &context)?;

        // Create indexes on many columns
        let start_time = Instant::now();

        for i in 0..50 {
            let column_name = format!("col_{:03}", i);
            let index_name = format!("idx_index_test_table_{}", column_name);

            let create_index = Statement::CreateIndex {
                if_not_exists: false,
                index_name,
                table_name: "index_test_table".to_string(),
                columns: vec![column_name],
                unique: false,
                index_type: Some("btree".to_string()),
                with_options: vec![],
            };

            engine.execute_statement(&create_index, &context)?;
        }

        let duration = start_time.elapsed();

        // Verify all indexes were created
        let indexes = engine.context().catalog().list_indexes()?;
        assert_eq!(indexes.len(), 50);

        // Performance should be reasonable (less than 5 seconds for 50 indexes)
        assert!(duration.as_secs() < 5, "Index creation took too long: {:?}", duration);

        Ok(())
    }
}

#[cfg(test)]
mod wal_recovery_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ddl_wal_recovery_simulation() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("recovery_test.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        init_ddl_wal_manager(wal_manager)?;

        let ddl_wal = get_ddl_wal_manager().unwrap();

        // Simulate a series of DDL operations
        let tx_id = 1000;

        // Begin transaction
        let start_lsn = ddl_wal.begin_ddl_transaction(tx_id)?;

        // Log CREATE TABLE
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
                    name: "data".to_string(),
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

        let create_lsn = ddl_wal.log_create_table(
            tx_id,
            "recovery_test_table",
            Some("public".to_string()),
            &table_schema,
        )?;

        // Log ALTER TABLE ADD COLUMN
        let new_column = ColumnSchema {
            name: "new_field".to_string(),
            data_type: "TEXT".to_string(),
            nullable: true,
            default_value: Some("DEFAULT_VALUE".to_string()),
            max_length: None,
            precision: None,
            scale: None,
        };

        let add_column_lsn = ddl_wal.log_add_column(
            tx_id,
            "recovery_test_table",
            Some("public".to_string()),
            &new_column,
        )?;

        // Commit transaction
        let commit_lsn = ddl_wal.commit_ddl_transaction(tx_id)?;

        // Verify transaction state
        assert!(!ddl_wal.is_transaction_active(tx_id));

        // Verify LSNs are in order
        assert!(create_lsn > start_lsn);
        assert!(add_column_lsn > create_lsn);
        assert!(commit_lsn > add_column_lsn);

        // Simulate recovery by checking schema evolution manager
        let schema_manager = ddl_wal.get_schema_manager();
        let schema = schema_manager.lock().unwrap();

        // In a real scenario, we would replay the WAL to rebuild the schema
        // For now, we just verify the WAL logging structure
        assert_eq!(schema.get_current_version("recovery_test_table"), Some(1));

        Ok(())
    }

    #[test]
    fn test_ddl_wal_rollback_recovery() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("rollback_recovery.wal");

        let wal_manager = WALManager::create(&wal_path, 8192)?;
        init_ddl_wal_manager(wal_manager)?;

        let ddl_wal = get_ddl_wal_manager().unwrap();

        let tx_id = 2000;

        // Begin transaction
        let start_lsn = ddl_wal.begin_ddl_transaction(tx_id)?;

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
            "rollback_test_table",
            Some("public".to_string()),
            &table_schema,
        )?;

        // Verify transaction state before rollback
        let tx_state_before = ddl_wal.get_transaction_state(tx_id);
        assert!(tx_state_before.is_some());
        assert_eq!(tx_state_before.unwrap().modified_objects.len(), 1);

        // Rollback transaction
        let rollback_lsn = ddl_wal.rollback_ddl_transaction(tx_id)?;

        // Verify transaction state after rollback
        assert!(!ddl_wal.is_transaction_active(tx_id));
        let tx_state_after = ddl_wal.get_transaction_state(tx_id);
        assert!(tx_state_after.is_none());

        // Verify LSN order
        assert!(create_lsn > start_lsn);
        assert!(rollback_lsn > create_lsn);

        Ok(())
    }
}