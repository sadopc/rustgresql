//! Integration test for window function execution
//!
//! This test verifies that the window function fix resolves the NULL value issue
//! in arithmetic expressions containing window functions.
//!
//! Original problem: e.salary - AVG(e.salary) OVER (PARTITION BY e.department_id)
//! was returning NULL instead of calculated salary differences from department averages.

use rustgresql::{Database, Config, sql::parse_sql, executor::ExecutionEngine};
use rustgresql::types::{DataType, Value};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Window Function Execution Test...");

    // Create in-memory test database
    let config = Config {
        data_file_path: ":memory:".to_string(),
        ..Config::default()
    };

    let mut db = Database::new(config)?;
    db.initialize()?;

    // Create execution engine
    let execution_engine = ExecutionEngine::with_catalog_buffer_and_tm(
        db.get_catalog(),
        db.get_buffer_manager(),
        db.transaction_manager.clone()
    );

    println!("✓ Database initialized");

    // Create test tables
    setup_test_tables(&execution_engine)?;
    println!("✓ Test tables created");

    // Insert test data
    insert_test_data(&execution_engine)?;
    println!("✓ Test data inserted");

    // Test the specific failing query
    test_complex_window_function_query(&execution_engine)?;
    println!("✓ Complex window function query test completed");

    // Test basic window functions for backwards compatibility
    test_basic_window_functions(&execution_engine)?;
    println!("✓ Basic window functions test completed");

    println!("🎉 All tests passed! Window function fix is working correctly.");
    Ok(())
}

fn setup_test_tables(execution_engine: &ExecutionEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Create departments table
    let create_departments = r#"
        CREATE TABLE departments (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            budget NUMERIC(12, 2),
            location VARCHAR(100)
        )"#;

    let result = execution_engine.execute(create_departments)?;
    if !result.is_success() {
        return Err(format!("Failed to create departments table: {:?}", result.get_error()).into());
    }

    // Create employees table
    let create_employees = r#"
        CREATE TABLE employees (
            id INTEGER PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            department_id INTEGER,
            salary NUMERIC(10, 2),
            hire_date DATE,
            email VARCHAR(100),
            is_active BOOLEAN DEFAULT TRUE,
            manager_id INTEGER
        )"#;

    let result = execution_engine.execute(create_employees)?;
    if !result.is_success() {
        return Err(format!("Failed to create employees table: {:?}", result.get_error()).into());
    }

    Ok(())
}

fn insert_test_data(execution_engine: &ExecutionEngine) -> Result<(), Box<dyn std::error::Error>> {
    // Insert departments data
    let insert_departments = r#"
        INSERT INTO departments (id, name, budget, location) VALUES
            (1, 'Engineering', 1000000.00, 'New York'),
            (2, 'Sales', 500000.00, 'San Francisco'),
            (3, 'Marketing', 300000.00, 'Los Angeles'),
            (4, 'HR', 200000.00, 'New York'),
            (5, 'Finance', 400000.00, 'Chicago')
    "#;

    let result = execution_engine.execute(insert_departments)?;
    if !result.is_success() {
        return Err(format!("Failed to insert departments data: {:?}", result.get_error()).into());
    }

    // Insert employees data
    let insert_employees = r#"
        INSERT INTO employees (id, name, department_id, salary, hire_date, email, is_active, manager_id) VALUES
            (1, 'Alice Johnson', 1, 95000.00, '2020-01-15', 'alice@example.com', TRUE, NULL),
            (2, 'Bob Smith', 1, 85000.00, '2020-03-20', 'bob@example.com', TRUE, 1),
            (3, 'Carol White', 2, 75000.00, '2019-06-10', 'carol@example.com', TRUE, NULL),
            (4, 'David Brown', 2, 70000.00, '2021-02-01', 'david@example.com', TRUE, 3),
            (5, 'Eve Davis', 3, 65000.00, '2021-05-15', 'eve@example.com', TRUE, NULL),
            (6, 'Frank Miller', 3, 60000.00, '2022-01-10', 'frank@example.com', TRUE, 5),
            (7, 'Grace Lee', 1, 90000.00, '2019-09-01', 'grace@example.com', TRUE, 1),
            (8, 'Henry Wilson', 4, 55000.00, '2022-03-15', 'henry@example.com', TRUE, NULL),
            (9, 'Iris Taylor', 5, 80000.00, '2020-07-20', 'iris@example.com', TRUE, NULL),
            (10, 'Jack Anderson', 1, 78000.00, '2021-11-05', 'jack@example.com', FALSE, 1)
    "#;

    let result = execution_engine.execute(insert_employees)?;
    if !result.is_success() {
        return Err(format!("Failed to insert employees data: {:?}", result.get_error()).into());
    }

    Ok(())
}

fn test_complex_window_function_query(execution_engine: &ExecutionEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Testing complex window function query...");

    // This is the exact failing query from the original problem
    let complex_query = r#"
        SELECT
            e.name,
            e.department_id,
            e.salary,
            e.hire_date,
            ROW_NUMBER() OVER (PARTITION BY e.department_id ORDER BY e.hire_date) AS dept_hire_order,
            RANK() OVER (PARTITION BY e.department_id ORDER BY e.salary DESC) AS dept_salary_rank,
            SUM(e.salary) OVER (PARTITION BY e.department_id) AS dept_total_salary,
            AVG(e.salary) OVER (PARTITION BY e.department_id) AS dept_avg_salary,
            e.salary - AVG(e.salary) OVER (PARTITION BY e.department_id) AS salary_vs_dept_avg
        FROM employees e
        WHERE e.is_active = TRUE
        ORDER BY e.department_id, e.salary DESC
    "#;

    let result = execution_engine.execute(complex_query)?;

    if !result.is_success() {
        return Err(format!("Complex window function query failed: {:?}", result.get_error()).into());
    }

    let query_result = result.get_result();
    println!("Query executed successfully. Retrieved {} rows.", query_result.len());

    // Verify results
    verify_complex_query_results(query_result)?;

    Ok(())
}

fn verify_complex_query_results(rows: &[Vec<Value>]) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Analyzing results...");

    // Expected department averages based on test data:
    // Engineering (dept 1): Alice(95000) + Bob(85000) + Grace(90000) = 270000 / 3 = 90000
    // Sales (dept 2): Carol(75000) + David(70000) = 145000 / 2 = 72500
    // Marketing (dept 3): Eve(65000) + Frank(60000) = 125000 / 2 = 62500
    // HR (dept 4): Henry(55000) = 55000 / 1 = 55000
    // Finance (dept 5): Iris(80000) = 80000 / 1 = 80000

    let expected_department_averages = std::collections::HashMap::from([
        (1, 90000.0), // Engineering
        (2, 72500.0), // Sales
        (3, 62500.0), // Marketing
        (4, 55000.0), // HR
        (5, 80000.0), // Finance
    ]);

    let mut null_count = 0;
    let mut correct_calculations = 0;
    let mut total_rows = 0;

    for row in rows {
        // Column mapping based on query:
        // 0: name, 1: department_id, 2: salary, 3: hire_date,
        // 4: dept_hire_order, 5: dept_salary_rank, 6: dept_total_salary,
        // 7: dept_avg_salary, 8: salary_vs_dept_avg

        if row.len() >= 9 {
            let name = &row[0];
            let department_id = &row[1];
            let salary = &row[2];
            let dept_avg_salary = &row[7];
            let salary_vs_dept_avg = &row[8];

            total_rows += 1;

            println!("Row: {} (Dept: {}, Salary: {})", name, department_id, salary);
            println!("  - Dept Avg Salary: {}", dept_avg_salary);
            println!("  - Salary vs Dept Avg: {}", salary_vs_dept_avg);

            // Check if salary_vs_dept_avg is NULL
            if matches!(salary_vs_dept_avg, Value::Null) {
                null_count += 1;
                println!("  ❌ ERROR: salary_vs_dept_avg is NULL!");
            } else {
                // Verify the calculation is correct
                if let (Value::Numeric(salary_val), Value::Numeric(avg_val), Value::Numeric(diff_val)) =
                    (salary, dept_avg_salary, salary_vs_dept_avg) {

                    let expected_diff = salary_val - avg_val;
                    if (diff_val - expected_diff).abs() < 0.01 {
                        correct_calculations += 1;
                        println!("  ✅ Correct calculation: {} - {} = {}", salary_val, avg_val, diff_val);
                    } else {
                        println!("  ❌ Wrong calculation: expected {}, got {}", expected_diff, diff_val);
                    }
                } else {
                    println!("  ❌ Unexpected data types");
                }
            }
        }
    }

    println!("\n📈 Test Results Summary:");
    println!("  - Total rows processed: {}", total_rows);
    println!("  - Rows with NULL salary_vs_dept_avg: {}", null_count);
    println!("  - Rows with correct calculations: {}", correct_calculations);
    println!("  - Success rate: {:.1}%", (correct_calculations as f64 / total_rows as f64) * 100.0);

    if null_count > 0 {
        return Err(format!("Found {} rows with NULL salary_vs_dept_avg values!", null_count).into());
    }

    if correct_calculations != total_rows {
        return Err(format!("Only {}/{} rows had correct calculations!", correct_calculations, total_rows).into());
    }

    Ok(())
}

fn test_basic_window_functions(execution_engine: &ExecutionEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🧪 Testing basic window functions for backwards compatibility...");

    // Simple window function test
    let basic_query = r#"
        SELECT
            name,
            salary,
            AVG(salary) OVER (PARTITION BY department_id) AS dept_avg,
            ROW_NUMBER() OVER (PARTITION BY department_id ORDER BY salary DESC) AS dept_rank
        FROM employees
        WHERE is_active = TRUE
        ORDER BY department_id, salary DESC
    "#;

    let result = execution_engine.execute(basic_query)?;

    if !result.is_success() {
        return Err(format!("Basic window function query failed: {:?}", result.get_error()).into());
    }

    let query_result = result.get_result();
    println!("Basic window function query executed successfully. Retrieved {} rows.", query_result.len());

    // Verify basic window functions work
    for (i, row) in query_result.iter().enumerate() {
        if row.len() >= 4 {
            println!("Row {}: {} (Salary: {}, Dept Avg: {}, Rank: {})",
                i + 1, row[0], row[1], row[2], row[3]);

            // Check for NULL values in basic window functions
            if matches!(&row[2], Value::Null) || matches!(&row[3], Value::Null) {
                return Err("Basic window functions returned NULL values!".into());
            }
        }
    }

    println!("✅ Basic window functions are working correctly!");

    Ok(())
}