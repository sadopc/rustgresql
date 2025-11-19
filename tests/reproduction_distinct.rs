use rustgresql::executor::Executor;
use rustgresql::storage::buffer::BufferPoolManager;
use rustgresql::storage::file_manager::DefaultFileManager;
use rustgresql::catalog::CatalogManager;
use rustgresql::sql::lexer::Lexer;
use rustgresql::sql::parser::Parser;
use std::sync::{Arc, Mutex};
use std::fs;

#[test]
fn test_reproduce_distinct_bug() {
    // Setup
    let db_file = "test_reproduce_distinct.db";
    if std::path::Path::new(db_file).exists() {
        fs::remove_file(db_file).unwrap();
    }
    
    // Initialize components with proper types
    let file_manager = DefaultFileManager::create(db_file, 8192).unwrap();
    let file_manager_arc: Arc<Mutex<dyn rustgresql::storage::FileManager + Send>> = Arc::new(Mutex::new(file_manager));
    
    let buffer_pool = Arc::new(BufferPoolManager::new(100, file_manager_arc));
    
    let mut catalog = CatalogManager::new();
    catalog.set_buffer_manager(buffer_pool.clone());
    catalog.initialize().unwrap();
    let catalog = Arc::new(catalog);

    // Initialize executor
    let mut executor = Executor::with_catalog_and_buffer(catalog, buffer_pool);

    // Helper to execute SQL
    let mut execute_sql = |sql: &str| {
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();
        executor.execute_statement(&statements[0]).unwrap()
    };

    // Create table
    execute_sql("CREATE TABLE items (id INTEGER PRIMARY KEY, category VARCHAR(50));");

    // Insert data with duplicates
    let inserts = vec![
        "INSERT INTO items VALUES (1, 'A');",
        "INSERT INTO items VALUES (2, 'A');",
        "INSERT INTO items VALUES (3, 'B');",
        "INSERT INTO items VALUES (4, 'B');",
        "INSERT INTO items VALUES (5, 'C');",
    ];

    for insert in inserts {
        execute_sql(insert);
    }

    // Query DISTINCT
    let result = execute_sql("SELECT DISTINCT category FROM items;");

    // Verify results
    // Expected: 3 rows (A, B, C)
    // If bug exists: 5 rows (A, A, B, B, C)
    println!("Number of rows returned: {}", result.rows.len());
    for row in &result.rows {
        println!("{:?}", row);
    }

    // Debugging equality
    if result.rows.len() >= 2 {
        let row0 = &result.rows[0];
        let row1 = &result.rows[1];
        println!("Row 0: {:?}", row0);
        println!("Row 1: {:?}", row1);
        println!("Row 0 == Row 1: {}", row0 == row1);
        if let (Some(v0), Some(v1)) = (row0.get(0), row1.get(0)) {
             println!("Val 0: {:?}", v0);
             println!("Val 1: {:?}", v1);
             println!("Val 0 == Val 1: {}", v0 == v1);
        }
    }

    // Clean up
    if std::path::Path::new(db_file).exists() {
        fs::remove_file(db_file).unwrap();
    }

    assert_eq!(result.rows.len(), 3, "Expected 3 unique rows, got {}", result.rows.len());
}
