//! RustgreSQL Benchmark Suite
//! Tests performance of various database operations

use rustgresql::{Database, Config, sql::parse_sql, executor::ExecutionEngine};
use std::fs;
use std::time::{Duration, Instant};
use std::sync::Arc;

/// Benchmark result for a single test
#[derive(Debug)]
struct BenchmarkResult {
    name: String,
    iterations: usize,
    total_time: Duration,
    min_time: Duration,
    max_time: Duration,
    avg_time: Duration,
    rows_affected: usize,
    ops_per_sec: f64,
}

impl BenchmarkResult {
    fn print(&self) {
        println!("\n┌─────────────────────────────────────────────────────────────┐");
        println!("│ {} ", self.name);
        println!("├─────────────────────────────────────────────────────────────┤");
        println!("│ Iterations:    {:>10}                                  │", self.iterations);
        println!("│ Total time:    {:>10.2?}                              │", self.total_time);
        println!("│ Min time:      {:>10.2?}                              │", self.min_time);
        println!("│ Max time:      {:>10.2?}                              │", self.max_time);
        println!("│ Avg time:      {:>10.2?}                              │", self.avg_time);
        println!("│ Rows affected: {:>10}                                  │", self.rows_affected);
        println!("│ Ops/sec:       {:>10.2}                                │", self.ops_per_sec);
        println!("└─────────────────────────────────────────────────────────────┘");
    }
}

/// Run a benchmark for a given SQL query
fn run_benchmark(
    engine: &ExecutionEngine,
    name: &str,
    sql: &str,
    iterations: usize,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    let mut times = Vec::with_capacity(iterations);
    let mut total_rows = 0;

    // Warm-up run
    let statements = parse_sql(sql)?;
    for stmt in &statements {
        let _ = engine.execute_query(stmt, None);
    }

    // Actual benchmark runs
    for _ in 0..iterations {
        let start = Instant::now();
        let statements = parse_sql(sql)?;
        for stmt in &statements {
            let (result, _) = engine.execute_query(stmt, None)?;
            total_rows += result.rows.len();
        }
        times.push(start.elapsed());
    }

    let total_time: Duration = times.iter().sum();
    let min_time = *times.iter().min().unwrap();
    let max_time = *times.iter().max().unwrap();
    let avg_time = total_time / iterations as u32;
    let ops_per_sec = iterations as f64 / total_time.as_secs_f64();

    Ok(BenchmarkResult {
        name: name.to_string(),
        iterations,
        total_time,
        min_time,
        max_time,
        avg_time,
        rows_affected: total_rows,
        ops_per_sec,
    })
}

/// Run a benchmark for INSERT operations
fn run_insert_benchmark(
    engine: &ExecutionEngine,
    name: &str,
    sql_template: &str,
    iterations: usize,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    let mut times = Vec::with_capacity(iterations);
    let mut total_rows = 0;

    for i in 0..iterations {
        // Replace placeholder with unique value
        let sql = sql_template.replace("{i}", &i.to_string());

        let start = Instant::now();
        let statements = parse_sql(&sql)?;
        for stmt in &statements {
            let (result, _) = engine.execute_query(stmt, None)?;
            total_rows += result.rows.len();
        }
        times.push(start.elapsed());
    }

    let total_time: Duration = times.iter().sum();
    let min_time = *times.iter().min().unwrap_or(&Duration::ZERO);
    let max_time = *times.iter().max().unwrap_or(&Duration::ZERO);
    let avg_time = if iterations > 0 { total_time / iterations as u32 } else { Duration::ZERO };
    let ops_per_sec = if total_time.as_secs_f64() > 0.0 {
        iterations as f64 / total_time.as_secs_f64()
    } else {
        0.0
    };

    Ok(BenchmarkResult {
        name: name.to_string(),
        iterations,
        total_time,
        min_time,
        max_time,
        avg_time,
        rows_affected: total_rows,
        ops_per_sec,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           RustgreSQL Benchmark Suite                          ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    // Clean up any existing test database
    let db_path = "benchmark.db";
    if std::path::Path::new(db_path).exists() {
        fs::remove_file(db_path)?;
    }

    // Create database with reasonable buffer pool
    let config = Config {
        data_file_path: db_path.to_string(),
        buffer_pool_size: 50000,
        ..Config::default()
    };

    let db = Database::new(config)?;
    db.initialize()?;

    let engine = Arc::new(ExecutionEngine::with_catalog_buffer_and_tm(
        db.get_catalog(),
        db.get_buffer_manager(),
        db.transaction_manager.clone()
    ));

    // ========================================
    // Setup: Create tables
    // ========================================
    println!("\n📦 Setting up benchmark tables...\n");

    let setup_queries = vec![
        // Users table
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            username VARCHAR(50),
            email VARCHAR(100),
            age INTEGER,
            created_at TIMESTAMP,
            is_active BOOLEAN
        )",
        // Products table
        "CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100),
            description TEXT,
            price DECIMAL(10,2),
            stock INTEGER,
            category_id INTEGER
        )",
        // Orders table
        "CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            product_id INTEGER,
            quantity INTEGER,
            total_price DECIMAL(10,2),
            order_date TIMESTAMP,
            status VARCHAR(20)
        )",
        // Categories table
        "CREATE TABLE categories (
            id INTEGER PRIMARY KEY,
            name VARCHAR(50),
            parent_id INTEGER
        )",
        // Create indexes
        "CREATE INDEX idx_users_email ON users(email)",
        "CREATE INDEX idx_products_category ON products(category_id)",
        "CREATE INDEX idx_orders_user ON orders(user_id)",
    ];

    for sql in &setup_queries {
        let statements = parse_sql(sql)?;
        for stmt in statements {
            engine.execute_query(&stmt, None)?;
        }
    }
    println!("✓ Tables and indexes created\n");

    // ========================================
    // Benchmark 1: Single Row Inserts
    // ========================================
    println!("\n🔥 Running Benchmarks...\n");

    let results = vec![
        // Single row inserts
        run_insert_benchmark(
            &engine,
            "Single Row INSERT (users)",
            "INSERT INTO users (id, username, email, age, is_active) VALUES ({i}, 'user{i}', 'user{i}@test.com', 25, true)",
            1000,
        )?,

        // Batch inserts (10 rows per insert)
        run_insert_benchmark(
            &engine,
            "Batch INSERT (10 rows)",
            "INSERT INTO products (id, name, price, stock, category_id) VALUES
                ({i}0, 'Product {i}0', 99.99, 100, 1),
                ({i}1, 'Product {i}1', 149.99, 50, 2),
                ({i}2, 'Product {i}2', 29.99, 200, 1),
                ({i}3, 'Product {i}3', 199.99, 30, 3),
                ({i}4, 'Product {i}4', 49.99, 150, 2),
                ({i}5, 'Product {i}5', 79.99, 80, 1),
                ({i}6, 'Product {i}6', 299.99, 20, 3),
                ({i}7, 'Product {i}7', 39.99, 120, 2),
                ({i}8, 'Product {i}8', 89.99, 60, 1),
                ({i}9, 'Product {i}9', 159.99, 40, 3)",
            100,
        )?,
    ];

    // Insert some orders for query benchmarks
    println!("\n📊 Inserting test data for query benchmarks...");
    for i in 0..500 {
        let sql = format!(
            "INSERT INTO orders (id, user_id, product_id, quantity, total_price, status) VALUES ({}, {}, {}, {}, {:.2}, '{}')",
            i, i % 1000, i % 1000, (i % 10) + 1, (i as f64 * 15.5) % 1000.0,
            if i % 3 == 0 { "completed" } else if i % 3 == 1 { "pending" } else { "shipped" }
        );
        let statements = parse_sql(&sql)?;
        for stmt in statements {
            engine.execute_query(&stmt, None)?;
        }
    }

    // Insert categories
    for i in 1..=10 {
        let sql = format!(
            "INSERT INTO categories (id, name, parent_id) VALUES ({}, 'Category {}', {})",
            i, i, if i > 3 { i - 3 } else { 0 }
        );
        let statements = parse_sql(&sql)?;
        for stmt in statements {
            engine.execute_query(&stmt, None)?;
        }
    }
    println!("✓ Test data inserted\n");

    // ========================================
    // Query Benchmarks
    // ========================================
    let mut all_results = results;

    // Simple SELECT
    println!("Running: Simple SELECT...");
    all_results.push(run_benchmark(
        &engine,
        "Simple SELECT (all rows)",
        "SELECT * FROM users;",
        100,
    )?);

    // SELECT with WHERE clause
    println!("Running: SELECT with WHERE...");
    all_results.push(run_benchmark(
        &engine,
        "SELECT with WHERE",
        "SELECT * FROM users WHERE age > 20;",
        100,
    )?);

    // SELECT with ORDER BY
    println!("Running: SELECT with ORDER BY...");
    all_results.push(run_benchmark(
        &engine,
        "SELECT with ORDER BY",
        "SELECT * FROM users ORDER BY username;",
        50,
    )?);

    // SELECT with LIMIT - try different syntax
    println!("Running: SELECT with LIMIT...");
    all_results.push(run_benchmark(
        &engine,
        "SELECT with LIMIT",
        "SELECT id, username, email FROM users ORDER BY id LIMIT 100;",
        100,
    )?);

    // Aggregate functions
    println!("Running: COUNT aggregate...");
    all_results.push(run_benchmark(
        &engine,
        "COUNT aggregate",
        "SELECT COUNT(*) FROM users;",
        100,
    )?);

    println!("Running: SUM/AVG aggregates...");
    all_results.push(run_benchmark(
        &engine,
        "SUM/AVG aggregates",
        "SELECT SUM(quantity), AVG(total_price) FROM orders;",
        100,
    )?);

    // GROUP BY
    println!("Running: GROUP BY...");
    all_results.push(run_benchmark(
        &engine,
        "GROUP BY with COUNT",
        "SELECT status, COUNT(*) FROM orders GROUP BY status;",
        100,
    )?);

    // JOIN operations
    println!("Running: JOIN...");
    all_results.push(run_benchmark(
        &engine,
        "Simple JOIN (2 tables)",
        "SELECT u.username, o.total_price FROM users u JOIN orders o ON u.id = o.user_id LIMIT 100;",
        50,
    )?);

    // Subquery - simplified
    println!("Running: Subquery...");
    all_results.push(run_benchmark(
        &engine,
        "Subquery in WHERE",
        "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE total_price > 500);",
        50,
    )?);

    // UPDATE benchmark
    println!("Running: UPDATE...");
    all_results.push(run_benchmark(
        &engine,
        "UPDATE single row",
        "UPDATE users SET age = 30 WHERE id = 1;",
        100,
    )?);

    // DELETE benchmark (on orders to avoid removing too much data)
    println!("Running: DELETE...");
    all_results.push(run_benchmark(
        &engine,
        "DELETE with WHERE",
        "DELETE FROM orders WHERE id > 10000;",
        100,
    )?);

    // Complex query - skip for now due to potential parsing issues
    // all_results.push(run_benchmark(...));

    // ========================================
    // Print all results
    // ========================================
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║                    BENCHMARK RESULTS                          ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    for result in &all_results {
        result.print();
    }

    // ========================================
    // Summary
    // ========================================
    println!("\n╔═══════════════════════════════════════════════════════════════╗");
    println!("║                       SUMMARY                                 ║");
    println!("╠═══════════════════════════════════════════════════════════════╣");

    let total_ops: usize = all_results.iter().map(|r| r.iterations).sum();
    let total_time: Duration = all_results.iter().map(|r| r.total_time).sum();

    println!("║ Total benchmarks run:  {:>5}                                 ║", all_results.len());
    println!("║ Total operations:      {:>5}                                 ║", total_ops);
    println!("║ Total time:            {:>8.2?}                            ║", total_time);
    println!("║ Overall ops/sec:       {:>8.2}                            ║", total_ops as f64 / total_time.as_secs_f64());
    println!("╚═══════════════════════════════════════════════════════════════╝");

    // Find fastest and slowest
    let fastest = all_results.iter().min_by(|a, b| a.avg_time.cmp(&b.avg_time)).unwrap();
    let slowest = all_results.iter().max_by(|a, b| a.avg_time.cmp(&b.avg_time)).unwrap();

    println!("\n🏆 Fastest: {} ({:.2?} avg)", fastest.name, fastest.avg_time);
    println!("🐢 Slowest: {} ({:.2?} avg)", slowest.name, slowest.avg_time);

    // Cleanup
    fs::remove_file(db_path).ok();

    println!("\n✅ Benchmark complete!");

    Ok(())
}
