use rustgresql::{Database, Config};
use rustgresql::types::ValueKind;
use std::fs;

#[test]
fn test_crash_recovery() {
    let db_path = "test_crash.db";
    let wal_path = "test_crash.wal";
    
    // Clean up previous runs
    let _ = fs::remove_file(db_path);
    let _ = fs::remove_file(wal_path);
    
    let config = Config {
        page_size: 4096,
        buffer_pool_size: 10,
        wal_enabled: true,
        wal_file_path: Some(wal_path.to_string()),
        data_file_path: db_path.to_string(),
    };
    
    // Phase 1: Create table and insert data
    {
        let db = Database::new(config.clone()).expect("Failed to create DB");
        
        let engine = rustgresql::executor::ExecutionEngine::with_catalog_and_buffer(db.get_catalog(), db.get_buffer_manager());
        
        // Create table
        let stmts = rustgresql::sql::parse_sql("CREATE TABLE t1 (id INT, name TEXT);").unwrap();
        engine.execute_query(&stmts[0]).expect("Failed to create table");
        
        // Insert data
        let stmts = rustgresql::sql::parse_sql("INSERT INTO t1 VALUES (1, 'Alice');").unwrap();
        engine.execute_query(&stmts[0]).expect("Failed to insert data");
        
        // We deliberately do NOT flush explicitly.
        // Assuming BufferPoolManager/Database handles shutdown gracefully or WAL recovery handles unwritten pages.
        // If Drop flushes, this tests persistence. If not, it tests WAL recovery.
    }
    
    // Phase 2: Reopen and verify
    {
        let db = Database::open(config).expect("Failed to open DB");
        let engine = rustgresql::executor::ExecutionEngine::with_catalog_and_buffer(db.get_catalog(), db.get_buffer_manager());
        
        // Select data
        let stmts = rustgresql::sql::parse_sql("SELECT * FROM t1;").unwrap();
        let (result, _) = engine.execute_query(&stmts[0]).expect("Failed to select data");
        
        assert_eq!(result.rows.len(), 1, "Expected 1 row after recovery");
        
        let row = &result.rows[0];
        if let ValueKind::String(name) = &row[1].kind {
            assert_eq!(name, "Alice");
        } else {
            panic!("Expected string value for name column");
        }
    }
    
    // Cleanup
    let _ = fs::remove_file(db_path);
    let _ = fs::remove_file(wal_path);
}
