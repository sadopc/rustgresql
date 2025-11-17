//! Tests for INSERT with DEFAULT values and partial column lists

use rustgresql::executor::ExecutionEngine;
use rustgresql::sql::parser::parse_sql;
use rustgresql::transaction::ddl_wal::{init_ddl_wal_manager};
use rustgresql::transaction::wal::WALManager;
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
fn test_insert_with_partial_columns_and_defaults() {
    let engine = create_test_engine();

    // Step 1: Create table with DEFAULT values
    let create_sql = r#"
        CREATE TABLE test_defaults (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255) UNIQUE,
            age INTEGER CHECK (age >= 18),
            status VARCHAR(20) DEFAULT 'active',
            department_id INTEGER
        )
    "#;

    let mut parser = Parser::new(create_sql);
    let create_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&create_stmt);
    assert!(result.is_ok(), "Failed to create table: {:?}", result.err());

    // Step 2: Insert with only some columns (should use DEFAULT and NULL)
    let insert_sql = "INSERT INTO test_defaults (id, name, email, age) VALUES (1, 'John Doe', 'john@example.com', 25)";
    let mut parser = Parser::new(insert_sql);
    let insert_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&insert_stmt);
    assert!(result.is_ok(), "Failed to insert with partial columns: {:?}", result.err());

    // Step 3: Insert with different columns
    let insert_sql2 = "INSERT INTO test_defaults (id, name, age) VALUES (2, 'Jane Doe', 30)";
    let mut parser = Parser::new(insert_sql2);
    let insert_stmt2 = parser.parse().unwrap();
    let result2 = engine.execute_statement(&insert_stmt2);
    assert!(result2.is_ok(), "Failed to insert with different partial columns: {:?}", result2.err());

    // Step 4: Insert all columns explicitly
    let insert_sql3 = "INSERT INTO test_defaults (id, name, email, age, status, department_id) VALUES (3, 'Bob Smith', 'bob@example.com', 35, 'inactive', 10)";
    let mut parser = Parser::new(insert_sql3);
    let insert_stmt3 = parser.parse().unwrap();
    let result3 = engine.execute_statement(&insert_stmt3);
    assert!(result3.is_ok(), "Failed to insert with all columns: {:?}", result3.err());

    // Step 5: Verify data was inserted correctly by selecting
    let select_sql = "SELECT * FROM test_defaults";
    let mut parser = Parser::new(select_sql);
    let select_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&select_stmt);

    assert!(result.is_ok(), "Failed to select data: {:?}", result.err());
    let query_result = result.unwrap();
    assert_eq!(query_result.rows.len(), 3, "Expected 3 rows");
}

#[test]
fn test_insert_without_not_null_column_fails() {
    // Create a test catalog and buffer manager
    let buffer_manager = Arc::new(BufferPoolManager::new(1000, 8192).unwrap());
    let catalog = Arc::new(CatalogManager::new().unwrap());

    // Create execution context
    let mut context = ExecutionContext::new();
    context.set_catalog(catalog.clone());
    context.set_buffer_manager(buffer_manager.clone());

    // Create execution engine
    let mut engine = ExecutionEngine::new(catalog.clone(), buffer_manager.clone());

    // Create table with NOT NULL column without DEFAULT
    let create_sql = r#"
        CREATE TABLE test_not_null (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255)
        )
    "#;

    let mut parser = Parser::new(create_sql);
    let create_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&create_stmt);
    assert!(result.is_ok(), "Failed to create table: {:?}", result.err());

    // Try to insert without providing the NOT NULL column (should fail)
    let insert_sql = "INSERT INTO test_not_null (id, email) VALUES (1, 'test@example.com')";
    let mut parser = Parser::new(insert_sql);
    let insert_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&insert_stmt);

    assert!(result.is_err(), "Expected error for missing NOT NULL column");
    let error_msg = format!("{:?}", result.err().unwrap());
    assert!(error_msg.contains("NOT NULL") || error_msg.contains("name"),
            "Error should mention NOT NULL or the column name, got: {}", error_msg);
}

#[test]
fn test_insert_all_columns_when_no_column_list() {
    // Create a test catalog and buffer manager
    let buffer_manager = Arc::new(BufferPoolManager::new(1000, 8192).unwrap());
    let catalog = Arc::new(CatalogManager::new().unwrap());

    // Create execution context
    let mut context = ExecutionContext::new();
    context.set_catalog(catalog.clone());
    context.set_buffer_manager(buffer_manager.clone());

    // Create execution engine
    let mut engine = ExecutionEngine::new(catalog.clone(), buffer_manager.clone());

    // Create table
    let create_sql = r#"
        CREATE TABLE test_all_columns (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            email VARCHAR(255)
        )
    "#;

    let mut parser = Parser::new(create_sql);
    let create_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&create_stmt);
    assert!(result.is_ok(), "Failed to create table: {:?}", result.err());

    // Insert with all columns (no column list) - should provide exactly 3 values
    let insert_sql = "INSERT INTO test_all_columns VALUES (1, 'John Doe', 'john@example.com')";
    let mut parser = Parser::new(insert_sql);
    let insert_stmt = parser.parse().unwrap();
    let result = engine.execute_statement(&insert_stmt);
    assert!(result.is_ok(), "Failed to insert with all columns: {:?}", result.err());

    // Try to insert with wrong number of values (should fail)
    let insert_sql_wrong = "INSERT INTO test_all_columns VALUES (2, 'Jane Doe')";
    let mut parser = Parser::new(insert_sql_wrong);
    let insert_stmt_wrong = parser.parse().unwrap();
    let result_wrong = engine.execute_statement(&insert_stmt_wrong);
    assert!(result_wrong.is_err(), "Expected error for wrong number of columns");
}
