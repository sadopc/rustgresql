//! RustgreSQL - A PostgreSQL-like database implemented in Rust
//!
//! Main entry point for the database application

use rustgresql::{Database, Config, sql::parse_sql, executor::ExecutionEngine};
use std::env;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use rustyline::history::FileHistory;
use comfy_table::Table;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;

fn main() -> rustgresql::Result<()> {
    // Initialize logging
    env_logger::init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let config = if args.len() > 1 {
        Config {
            data_file_path: args[1].clone(),
            ..Config::default()
        }
    } else {
        Config::default()
    };

    // Initialize or open database
    let db = if std::path::Path::new(&config.data_file_path).exists() {
        Database::open(config)?
    } else {
        let mut db = Database::new(config)?;
        db.initialize()?;
        db
    };

    // Initialize execution engine with database's catalog, buffer manager, and transaction manager
    let execution_engine = ExecutionEngine::with_catalog_buffer_and_tm(
        db.get_catalog(), 
        db.get_buffer_manager(),
        db.transaction_manager.clone()
    );

    println!("RustgreSQL v0.1.0 - Phase 1.1 Query Execution Engine");
    println!("Type 'help' for commands, SQL queries, or 'exit' to quit.");
    println!();

    // Initialize Rustyline editor with file history
    let mut rl: Editor<(), FileHistory> = Editor::new().unwrap();
    let history_file = "rustgresql_history.txt";
    if rl.load_history(history_file).is_err() {
        println!("No previous history.");
    }

    let mut query_buffer = String::new();
    let mut current_transaction_id: Option<u64> = None;

    // Enhanced REPL with SQL execution
    loop {
        let prompt = if query_buffer.is_empty() {
            if current_transaction_id.is_some() { "rustgresql(tx)> " } else { "rustgresql> " }
        } else {
            "-> " 
        };
        
        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                
                // Handle special commands immediately if buffer is empty
                if query_buffer.is_empty() && (trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit")) {
                    break;
                }
                
                if query_buffer.is_empty() && trimmed.eq_ignore_ascii_case("help") {
                    print_help();
                    rl.add_history_entry(trimmed);
                    continue;
                }

                if query_buffer.is_empty() && trimmed.eq_ignore_ascii_case("status") {
                    print_status(&db);
                    rl.add_history_entry(trimmed);
                    continue;
                }

                 if query_buffer.is_empty() && trimmed.eq_ignore_ascii_case("examples") {
                    print_examples();
                    rl.add_history_entry(trimmed);
                    continue;
                }
                
                // Append line to buffer
                query_buffer.push_str(&line);
                query_buffer.push('\n');

                // Check if query is complete (ends with semicolon)
                if query_buffer.trim_end().ends_with(';') {
                    let full_query = query_buffer.trim().to_string();
                    rl.add_history_entry(&full_query);
                    
                    // Process the query
                    match execute_sql(&execution_engine, &full_query, &mut current_transaction_id) {
                        Ok(results) => {
                            for (result, stats) in results {
                                print_query_result(&result, &stats);
                            }
                            // Flush changes to disk
                            if let Err(e) = db.get_buffer_manager().flush_all_pages() {
                                eprintln!("Flush error: {}", e);
                            }
                        }
                        Err(e) => {
                            // Check if it's a special command that slipped through (unlikely given logic above but safe)
                            if !handle_special_command(&full_query, &db) {
                                eprintln!("Error: {}", e);
                            }
                        }
                    }
                    
                    // Reset buffer
                    query_buffer.clear();
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                query_buffer.clear();
            }
            Err(ReadlineError::Eof) => {
                println!("^D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    
    // Save history
    rl.save_history(history_file).unwrap();

    println!("Goodbye!");
    Ok(())
}

fn execute_sql(engine: &ExecutionEngine, sql: &str, current_tx_id: &mut Option<u64>) -> rustgresql::Result<Vec<(rustgresql::executor::QueryResult, rustgresql::executor::ExecutionStats)>> {
    // Parse the SQL statement
    let statements = parse_sql(sql)?;
    use rustgresql::sql::Statement;

    if statements.is_empty() {
        // No statements found (e.g., just comments), return success with empty result
        return Ok(vec![(rustgresql::executor::QueryResult {
            rows: vec![],
            column_names: vec![],
        }, rustgresql::executor::ExecutionStats::default())]);
    }

    let mut results = Vec::new();
    for statement in statements {
        match statement {
            Statement::BeginTransaction => {
                if current_tx_id.is_some() {
                     println!("Warning: There is already a transaction in progress");
                     continue;
                }
                if let Some(tm) = &engine.transaction_manager {
                    let tx_id = tm.begin_transaction(rustgresql::transaction::manager::IsolationLevel::ReadCommitted)?;
                    *current_tx_id = Some(tx_id);
                    // Return empty result for BEGIN
                     results.push((rustgresql::executor::QueryResult { rows: vec![], column_names: vec![] }, rustgresql::executor::ExecutionStats::default()));
                     println!("BEGIN");
                }
            },
            Statement::CommitTransaction => {
                 if let Some(tx_id) = *current_tx_id {
                     if let Some(tm) = &engine.transaction_manager {
                         tm.commit_transaction(tx_id)?;
                         *current_tx_id = None;
                         results.push((rustgresql::executor::QueryResult { rows: vec![], column_names: vec![] }, rustgresql::executor::ExecutionStats::default()));
                         println!("COMMIT");
                     }
                 } else {
                     println!("Warning: There is no transaction in progress");
                 }
            },
            Statement::RollbackTransaction => {
                 if let Some(tx_id) = *current_tx_id {
                     if let Some(tm) = &engine.transaction_manager {
                         tm.rollback_transaction(tx_id)?;
                         *current_tx_id = None;
                         results.push((rustgresql::executor::QueryResult { rows: vec![], column_names: vec![] }, rustgresql::executor::ExecutionStats::default()));
                         println!("ROLLBACK");
                     }
                 } else {
                     println!("Warning: There is no transaction in progress");
                 }
            },
            _ => {
                let result = engine.execute_query(&statement, *current_tx_id)?;
                results.push(result);
            }
        }
    }
    
    Ok(results)
}

fn print_query_result(result: &rustgresql::executor::QueryResult, stats: &rustgresql::executor::ExecutionStats) {
    if result.rows.is_empty() && result.column_names.is_empty() {
        println!("Query executed successfully.");
        if stats.execution_time_ms > 0 {
            println!("Execution time: {}ms", stats.execution_time_ms);
        }
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    // Add header
    if !result.column_names.is_empty() {
        table.set_header(&result.column_names);
    }

    // Add rows
    for row in &result.rows {
        let row_cells: Vec<String> = row.iter()
            .map(|value| format_value(value))
            .collect();
        table.add_row(row_cells);
    }

    println!("{}", table);

    // Print statistics
    println!("Rows returned: {}", result.rows.len());
    if stats.execution_time_ms > 0 {
        println!("Execution time: {}ms", stats.execution_time_ms);
    }
}

fn format_value(value: &rustgresql::types::Value) -> String {
    match &value.kind {
        rustgresql::types::ValueKind::Null(_) => "NULL".to_string(),
        rustgresql::types::ValueKind::String(s) => s.clone(),
        rustgresql::types::ValueKind::Integer(i) => i.to_string(),
        rustgresql::types::ValueKind::Float(f) => f.to_string(),
        rustgresql::types::ValueKind::Boolean(b) => b.to_string(),
        rustgresql::types::ValueKind::Timestamp(ts) => ts.format("%Y-%m-%d %H:%M:%S").to_string(),
        rustgresql::types::ValueKind::List(list) => {
            let items: Vec<String> = list.iter().map(|v| format_value(v)).collect();
            format!("[{}]", items.join(", "))
        }
    }
}

fn print_help() {
    println!("╭─────────────────────────────────────────────────────────────╮");
    println!("│              RustgreSQL v0.1.0 - HELP SYSTEM              │");
    println!("╰─────────────────────────────────────────────────────────────╯");
    println!();
    
    println!("🔧 BUILT-IN COMMANDS");
    println!("  help       - Show this comprehensive help message");
    println!("  status     - Display database configuration and statistics");
    println!("  examples   - Show SQL query examples and usage patterns");
    println!("  exit       - Exit the application gracefully");
    println!();
    
    println!("📊 SQL DDL COMMANDS (Data Definition Language)");
    println!("  CREATE TABLE    - Create new tables with columns and constraints");
    println!("  DROP TABLE      - Remove existing tables and their data");
    println!("  ALTER TABLE     - Modify table structure (ADD/DROP columns)");
    println!("  CREATE INDEX    - Create indexes for performance optimization");
    println!("  DROP INDEX      - Remove existing indexes");
    println!("  CREATE VIEW     - Create virtual tables based on queries");
    println!("  DROP VIEW       - Remove existing views");
    println!();
    
    println!("📝 SQL DML COMMANDS (Data Manipulation Language)");
    println!("  SELECT          - Query and retrieve data from tables");
    println!("  INSERT INTO     - Add new rows to tables");
    println!("  UPDATE          - Modify existing rows in tables");
    println!("  DELETE FROM     - Remove rows from tables");
    println!();
    
    println!("🔍 QUERY FEATURES");
    println!("  ✅ Basic arithmetic operations (+, -, *, /, %)");
    println!("  ✅ Comparison operators (=, !=, <>, <, <=, >, >=)");
    println!("  ✅ Logical operators (AND, OR, NOT, BETWEEN, IN)");
    println!("  ✅ Three-valued logic with proper NULL handling");
    println!("  ✅ String operations (LIKE, ILIKE, CONCAT, SUBSTRING)");
    println!("  ✅ Aggregate functions (COUNT, SUM, AVG, MIN, MAX)");
    println!("  ✅ Window functions (ROW_NUMBER, RANK, LAG, LEAD)");
    println!("  ✅ Common Table Expressions (WITH clauses)");
    println!("  ✅ Subqueries and correlated subqueries");
    println!("  ✅ JOIN operations (INNER, LEFT, RIGHT, FULL OUTER)");
    println!("  ✅ GROUP BY and HAVING clauses");
    println!("  ✅ ORDER BY with multiple columns and directions");
    println!("  ✅ LIMIT and OFFSET for pagination");
    println!("  ✅ DISTINCT for unique results");
    println!("  ✅ CASE expressions for conditional logic");
    println!();
    
    println!("🏗️  DATABASE ENGINE FEATURES");
    println!("  ✅ B-tree storage engine with efficient indexing");
    println!("  ✅ Buffer pool management (configurable size)");
    println!("  ✅ Write-Ahead Logging (WAL) for durability");
    println!("  ✅ ACID transactions with MVCC (Multi-Version Concurrency Control)");
    println!("  ✅ Cost-based query optimization");
    println!("  ✅ Parallel query execution support");
    println!("  ✅ Schema management and metadata catalog");
    println!("  ✅ Constraint enforcement (PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK)");
    println!("  ✅ Data types: INTEGER, BIGINT, TEXT, REAL, BOOLEAN, TIMESTAMP");
    println!();
    
    println!("💡 USAGE TIPS");
    println!("  • All SQL statements must end with a semicolon (;)");
    println!("  • Use multi-line queries for complex statements");
    println!("  • Table and column names are case-sensitive");
    println!("  • String literals use single quotes ('text')");
    println!("  • Use examples command to see sample queries");
    println!();
    
    println!("📚 EXAMPLE QUERIES");
    println!("  CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);");
    println!("  INSERT INTO users VALUES (1, 'Alice', 25);");
    println!("  SELECT name, age FROM users WHERE age >= 18 ORDER BY name;");
    println!("  SELECT COUNT(*) as total FROM users GROUP BY age;");
    println!();
    
    println!("For more examples, type: examples");
    println!("For database status, type: status");
}

fn print_status(db: &Database) {
    println!("╭─────────────────────────────────────────────────────────────╮");
    println!("│              RustgreSQL v0.1.0 - DATABASE STATUS           │");
    println!("╰─────────────────────────────────────────────────────────────╯");
    println!();
    
    println!("📊 DATABASE INFORMATION");
    println!("  Version: RustgreSQL v0.1.0");
    println!("  Engine: Phase 1.1 Query Execution Engine");
    println!("  Build: Debug mode");
    println!();
    
    println!("💾 STORAGE CONFIGURATION");
    println!("  Data file: {}", db.config.data_file_path);
    println!("  Page size: {} bytes ({} KB)", db.config.page_size, db.config.page_size / 1024);
    println!("  Buffer pool size: {} pages", db.config.buffer_pool_size);
    println!("  Buffer memory: ~{} MB", (db.config.buffer_pool_size * db.config.page_size) / (1024 * 1024));
    println!();
    
    println!("🔒 TRANSACTION & DURABILITY");
    println!("  WAL enabled: {}", if db.config.wal_enabled { "✅ Yes" } else { "❌ No" });
    if let Some(wal_path) = &db.config.wal_file_path {
        println!("  WAL file: {}", wal_path);
    }
    println!("  Transaction isolation: MVCC (Multi-Version Concurrency Control)");
    println!("  ACID compliance: Full support");
    println!();
    
    println!("🏗️  ENGINE CAPABILITIES");
    println!("  Storage engine: B-tree with efficient indexing");
    println!("  Query optimization: Cost-based optimizer");
    println!("  Parallel execution: Supported");
    println!("  Schema management: Full catalog system");
    println!("  Constraint enforcement: PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK");
    println!();
    
    println!("📈 SUPPORTED DATA TYPES");
    println!("  Numeric: INTEGER, BIGINT, REAL");
    println!("  Text: TEXT (variable length strings)");
    println!("  Boolean: BOOLEAN (TRUE/FALSE/NULL)");
    println!("  Temporal: TIMESTAMP (date and time)");
    println!("  Special: NULL (three-valued logic)");
    println!();
    
    println!("🔍 QUERY FEATURES");
    println!("  DDL: CREATE, ALTER, DROP (TABLE, INDEX, VIEW)");
    println!("  DML: SELECT, INSERT, UPDATE, DELETE");
    println!("  Joins: INNER, LEFT, RIGHT, FULL OUTER");
    println!("  Aggregates: COUNT, SUM, AVG, MIN, MAX");
    println!("  Window functions: ROW_NUMBER, RANK, LAG, LEAD");
    println!("  Subqueries: Correlated and non-correlated");
    println!("  CTEs: Common Table Expressions (WITH clauses)");
    println!();
    
    println!("⚡ PERFORMANCE FEATURES");
    println!("  Indexing: B-tree indexes for fast lookups");
    println!("  Buffer management: LRU eviction policy");
    println!("  Query planning: Parallel execution planning");
    println!("  Statistics: Cost-based optimization");
    println!();
    
    println!("🛡️  SAFETY & RELIABILITY");
    println!("  Crash recovery: WAL-based recovery");
    println!("  Data integrity: Page checksums");
    println!("  Concurrency: MVCC for high concurrency");
    println!("  Durability: Write-ahead logging");
    println!();
    
    println!("💻 SYSTEM INFORMATION");
    println!("  Target: {}", std::env::consts::OS);
    println!("  Architecture: {}", std::env::consts::ARCH);
    println!("  Rust version: {}", option_env!("RUSTC_VERSION").unwrap_or("unknown"));
    println!();
    
    println!("📝 USAGE NOTES");
    println!("  • All SQL statements must end with semicolon (;)");
    println!("  • Database file is created automatically if it doesn't exist");
    println!("  • Use 'help' for comprehensive command reference");
    println!("  • Use 'examples' for SQL query samples");
    println!("  • Database persists between sessions");
}

fn print_examples() {
    println!("╭─────────────────────────────────────────────────────────────╮");
    println!("│              RustgreSQL v0.1.0 - SQL EXAMPLES             │");
    println!("╰─────────────────────────────────────────────────────────────╯");
    println!();
    
    println!("🏗️  DATA DEFINITION (DDL)");
    println!("-- Create tables with various data types and constraints");
    println!("CREATE TABLE users (");
    println!("    id INTEGER PRIMARY KEY,");
    println!("    name TEXT NOT NULL,");
    println!("    email TEXT UNIQUE,");
    println!("    age INTEGER CHECK (age >= 0),");
    println!("    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP");
    println!(");");
    println!();
    println!("CREATE TABLE orders (");
    println!("    order_id BIGINT PRIMARY KEY,");
    println!("    user_id INTEGER REFERENCES users(id),");
    println!("    amount REAL NOT NULL,");
    println!("    status TEXT DEFAULT 'pending'");
    println!(");");
    println!();
    println!("-- Create indexes for performance");
    println!("CREATE INDEX idx_users_email ON users(email);");
    println!("CREATE INDEX idx_orders_user_id ON orders(user_id);");
    println!();
    
    println!("📝 DATA MANIPULATION (DML)");
    println!("-- Insert data into tables");
    println!("INSERT INTO users (id, name, email, age) VALUES");
    println!("    (1, 'Alice Johnson', 'alice@example.com', 28),");
    println!("    (2, 'Bob Smith', 'bob@example.com', 35),");
    println!("    (3, 'Carol Davis', 'carol@example.com', 42);");
    println!();
    println!("INSERT INTO orders (order_id, user_id, amount, status) VALUES");
    println!("    (1001, 1, 99.99, 'completed'),");
    println!("    (1002, 2, 149.50, 'pending'),");
    println!("    (1003, 1, 75.25, 'shipped');");
    println!();
    
    println!("🔍 BASIC QUERIES");
    println!("-- Select all columns");
    println!("SELECT * FROM users;");
    println!();
    println!("-- Select specific columns with aliases");
    println!("SELECT name AS full_name, email AS contact FROM users;");
    println!();
    println!("-- Filter with WHERE clause");
    println!("SELECT name, age FROM users WHERE age > 30;");
    println!();
    println!("-- Multiple conditions with AND/OR");
    println!("SELECT * FROM users WHERE age >= 25 AND (name LIKE 'A%' OR name LIKE 'C%');");
    println!();
    
    println!("🧮 ARITHMETIC & EXPRESSIONS");
    println!("-- Mathematical operations");
    println!("SELECT name, age + 10 AS age_in_decade FROM users;");
    println!("SELECT amount * 1.10 AS price_with_tax FROM orders;");
    println!();
    println!("-- Case expressions");
    println!("SELECT name, age,");
    println!("    CASE");
    println!("        WHEN age < 30 THEN 'Young'");
    println!("        WHEN age < 40 THEN 'Middle-aged'");
    println!("        ELSE 'Senior'");
    println!("    END AS age_group");
    println!("FROM users;");
    println!();
    
    println!("🔗 JOINS & RELATIONSHIPS");
    println!("-- Inner join");
    println!("SELECT u.name, o.order_id, o.amount");
    println!("FROM users u");
    println!("INNER JOIN orders o ON u.id = o.user_id;");
    println!();
    println!("-- Left join (show all users, even without orders)");
    println!("SELECT u.name, COUNT(o.order_id) AS order_count");
    println!("FROM users u");
    println!("LEFT JOIN orders o ON u.id = o.user_id");
    println!("GROUP BY u.id, u.name;");
    println!();
    
    println!("📊 AGGREGATION & GROUPING");
    println!("-- Basic aggregates");
    println!("SELECT COUNT(*) AS total_users, AVG(age) AS avg_age FROM users;");
    println!();
    println!("-- Group by with having");
    println!("SELECT status, COUNT(*) AS count, AVG(amount) AS avg_amount");
    println!("FROM orders");
    println!("GROUP BY status");
    println!("HAVING COUNT(*) > 1;");
    println!();
    
    println!("🪟 WINDOW FUNCTIONS");
    println!("-- Row numbering");
    println!("SELECT name, age,");
    println!("    ROW_NUMBER() OVER (ORDER BY age DESC) AS age_rank");
    println!("FROM users;");
    println!();
    println!("-- Running totals");
    println!("SELECT order_id, amount,");
    println!("    SUM(amount) OVER (ORDER BY order_id) AS running_total");
    println!("FROM orders;");
    println!();
    
    println!("🔍 SUBQUERIES & CTEs");
    println!("-- Common Table Expression (CTE)");
    println!("WITH user_stats AS (");
    println!("    SELECT user_id, COUNT(*) AS order_count, SUM(amount) AS total_spent");
    println!("    FROM orders");
    println!("    GROUP BY user_id");
    println!(")");
    println!("SELECT u.name, us.order_count, us.total_spent");
    println!("FROM users u");
    println!("INNER JOIN user_stats us ON u.id = us.user_id;");
    println!();
    println!("-- Subquery in WHERE clause");
    println!("SELECT name FROM users");
    println!("WHERE id IN (SELECT DISTINCT user_id FROM orders WHERE amount > 100);");
    println!();
    
    println!("🔧 STRING & NULL OPERATIONS");
    println!("-- String functions and pattern matching");
    println!("SELECT name, LENGTH(name) AS name_length");
    println!("FROM users");
    println!("WHERE name LIKE '%son%';");
    println!();
    println!("-- NULL handling");
    println!("SELECT name, COALESCE(email, 'No email') AS contact_info");
    println!("FROM users");
    println!("WHERE email IS NOT NULL;");
    println!();
    
    println!("📄 PAGINATION & ORDERING");
    println!("-- Order by multiple columns");
    println!("SELECT * FROM users ORDER BY age DESC, name ASC;");
    println!();
    println!("-- Limit and offset for pagination");
    println!("SELECT * FROM users ORDER BY created_at DESC LIMIT 10 OFFSET 20;");
    println!();
    
    println!("💡 TIPS FOR TESTING");
    println!("• Start with DDL to create tables, then DML to add data");
    println!("• Use semicolons (;) at the end of each statement");
    println!("• Build complex queries step by step");
    println!("• Use aliases to make results more readable");
    println!("• Test joins with small datasets first");
    println!();
    
    println!("🚀 QUICK START SEQUENCE");
    println!("1. CREATE TABLE test (id INT, name TEXT);");
    println!("2. INSERT INTO test VALUES (1, 'Hello');");
    println!("3. SELECT * FROM test;");
    println!();
}

fn handle_special_command(cmd: &str, _db: &Database) -> bool {
    match cmd {
        "test" => {
            println!("Running basic functionality test...");
            run_basic_test();
            true
        }
        _ => false,
    }
}

fn run_basic_test() {
    use rustgresql::executor::ExecutionEngine;

    let engine = ExecutionEngine::new();
    let mut tx_id = None;

    // Test basic expression evaluation
    let test_queries = vec![
        "SELECT 1 + 1 AS result;",
        "SELECT ABS(-42) AS absolute;",
        "SELECT 5 > 3 AS test;",
        "SELECT TRUE AND FALSE AS logical;",
    ];

    for query in test_queries {
        println!("Executing: {}", query);
        match execute_sql(&engine, query, &mut tx_id) {
            Ok(results) => {
                for (result, stats) in results {
                    print_query_result(&result, &stats);
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!();
    }
}
