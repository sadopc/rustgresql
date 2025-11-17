//! RustgreSQL - A PostgreSQL-like database implemented in Rust
//!
//! Main entry point for the database application

use rustgresql::{Database, Config, sql::parse_sql, executor::ExecutionEngine};
use std::env;
use std::io::{self, Write};

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

    // Initialize execution engine
    let execution_engine = ExecutionEngine::new();

    println!("RustgreSQL v0.1.0 - Phase 1.1 Query Execution Engine");
    println!("Type 'help' for commands, SQL queries, or 'exit' to quit.");
    println!();

    // Enhanced REPL with SQL execution
    loop {
        print!("rustgresql> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let command = input.trim();

        match command {
            "exit" | "quit" => break,
            "help" => {
                print_help();
            }
            "status" => {
                print_status(&db);
            }
            "examples" => {
                print_examples();
            }
            cmd if !cmd.is_empty() => {
                // Try to execute as SQL
                match execute_sql(&execution_engine, cmd) {
                    Ok((result, stats)) => {
                        print_query_result(&result, &stats);
                    }
                    Err(e) => {
                        // Check if it's a special command first
                        if !handle_special_command(cmd, &db) {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    println!("Goodbye!");
    Ok(())
}

fn execute_sql(engine: &ExecutionEngine, sql: &str) -> rustgresql::Result<(rustgresql::executor::QueryResult, rustgresql::executor::ExecutionStats)> {
    // Parse the SQL statement
    let statements = parse_sql(sql)?;

    if statements.is_empty() {
        return Err(rustgresql::error::RustgreSQLError::Parse("No SQL statement found".to_string()));
    }

    // Execute the first statement (for now, we'll support single statements)
    let statement = &statements[0];
    engine.execute_query(statement)
}

fn print_query_result(result: &rustgresql::executor::QueryResult, stats: &rustgresql::executor::ExecutionStats) {
    if result.rows.is_empty() && result.column_names.is_empty() {
        println!("Query executed successfully.");
        if stats.execution_time_ms > 0 {
            println!("Execution time: {}ms", stats.execution_time_ms);
        }
        return;
    }

    // Print column headers
    if !result.column_names.is_empty() {
        let header: String = result.column_names.join(" | ");
        let separator: String = result.column_names.iter().map(|_| "-".repeat(header.len() / result.column_names.len())).collect::<Vec<_>>().join("-+-");
        println!("{}", header);
        println!("{}", separator);
    }

    // Print rows
    for row in &result.rows {
        let row_str: String = row.iter()
            .map(|value| format_value(value))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("{}", row_str);
    }

    // Print statistics
    println!();
    println!("Rows returned: {}", result.rows.len());
    if stats.execution_time_ms > 0 {
        println!("Execution time: {}ms", stats.execution_time_ms);
    }
}

fn format_value(value: &rustgresql::types::Value) -> String {
    match &value.kind {
        rustgresql::types::ValueKind::Null(_) => "NULL".to_string(),
        rustgresql::types::ValueKind::String(s) => format!("'{}'", s),
        rustgresql::types::ValueKind::Integer(i) => i.to_string(),
        rustgresql::types::ValueKind::Float(f) => f.to_string(),
        rustgresql::types::ValueKind::Boolean(b) => b.to_string(),
        rustgresql::types::ValueKind::Timestamp(ts) => format!("'{}'", ts.format("%Y-%m-%d %H:%M:%S")),
    }
}

fn print_help() {
    println!("Available commands:");
    println!("  help       - Show this help message");
    println!("  status     - Show database status");
    println!("  examples    - Show SQL examples");
    println!("  exit       - Exit the program");
    println!();
    println!("SQL commands:");
    println!("  SELECT     - Query data from tables");
    println!("  INSERT     - Insert data into tables");
    println!("  UPDATE     - Update existing data");
    println!("  DELETE     - Delete data from tables");
    println!("  CREATE TABLE - Create new tables");
    println!();
    println!("Features available:");
    println!("  ✅ Arithmetic operations (+, -, *, /)");
    println!("  ✅ Comparison operations (=, !=, <, <=, >, >=)");
    println!("  ✅ Logical operations (AND, OR, NOT)");
    println!("  ✅ Three-valued logic with NULL handling");
    println!("  ✅ Built-in functions (ABS, COALESCE, LENGTH)");
    println!("  ✅ String pattern matching (LIKE, ILIKE)");
    println!("  ✅ Computed columns and expressions");
}

fn print_status(db: &Database) {
    println!("Database Status:");
    println!("  Version: RustgreSQL v0.1.0");
    println!("  Engine: Phase 1.1 Query Execution Engine");
    println!("  Data file: {}", db.config.data_file_path);
    println!("  Page size: {} bytes", db.config.page_size);
    println!("  Buffer pool size: {} pages", db.config.buffer_pool_size);
    println!("  WAL enabled: {}", db.config.wal_enabled);
}

fn print_examples() {
    println!("SQL Examples:");
    println!();
    println!("-- Basic queries");
    println!("SELECT * FROM test_table;");
    println!("SELECT name, age FROM users WHERE age > 18;");
    println!();
    println!("-- Arithmetic operations");
    println!("SELECT name, salary + bonus AS total_comp FROM employees;");
    println!("SELECT price * quantity AS total FROM orders;");
    println!();
    println!("-- Complex expressions");
    println!("SELECT * FROM users WHERE age >= 18 AND status = 'active';");
    println!("SELECT ABS(price - discount) FROM products;");
    println!();
    println!("-- String operations");
    println!("SELECT * FROM users WHERE name LIKE 'John%';");
    println!("SELECT LENGTH(description) FROM products;");
    println!();
    println!("-- Three-valued logic with NULL");
    println!("SELECT * FROM orders WHERE customer_id IS NULL;");
    println!("SELECT COALESCE(phone, 'N/A') FROM customers;");
}

fn handle_special_command(cmd: &str, db: &Database) -> bool {
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

    // Test basic expression evaluation
    let test_queries = vec![
        "SELECT 1 + 1 AS result;",
        "SELECT ABS(-42) AS absolute;",
        "SELECT 5 > 3 AS test;",
        "SELECT TRUE AND FALSE AS logical;",
    ];

    for query in test_queries {
        println!("Executing: {}", query);
        match execute_sql(&engine, query) {
            Ok((result, stats)) => {
                print_query_result(&result, &stats);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        println!();
    }
}
