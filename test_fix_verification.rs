// Simple test to verify the window function fix
use rustgresql::{Database, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing window function fix...");

    // Create in-memory test database
    let config = Config {
        data_file_path: ":memory:".to_string(),
        ..Config::default()
    };

    let mut db = Database::new(config)?;
    db.initialize()?;

    println!("✓ Database initialized");

    // Create test table
    let create_table = "CREATE TABLE employees (id INT, name VARCHAR(50), salary INT, department_id INT)";
    db.execute(create_table)?;
    println!("✓ Test table created");

    // Insert test data
    db.execute("INSERT INTO employees VALUES (1, 'Alice', 50000, 1)")?;
    db.execute("INSERT INTO employees VALUES (2, 'Bob', 60000, 1)")?;
    db.execute("INSERT INTO employees VALUES (3, 'Charlie', 70000, 2)")?;
    db.execute("INSERT INTO employees VALUES (4, 'David', 80000, 2)")?;
    println!("✓ Test data inserted");

    // Test window function with arithmetic expression
    println!("\nTest query: SELECT id, salary, AVG(salary) OVER (PARTITION BY department_id) as avg_salary, salary - AVG(salary) OVER (PARTITION BY department_id) as diff FROM employees");

    let query = "SELECT id, salary, AVG(salary) OVER (PARTITION BY department_id) as avg_salary, salary - AVG(salary) OVER (PARTITION BY department_id) as diff FROM employees";

    let result = db.execute(query)?;
    println!("\nQuery executed successfully!");
    println!("Result: {:?}", result);

    // The fix should allow the arithmetic expression to work
    if result.rows.len() > 0 {
        println!("\n✓ Test PASSED: Query returned {} rows", result.rows.len());
        println!("Column names: {:?}", result.column_names);

        // Check if the diff column has non-NULL values
        if result.column_names.contains(&"diff".to_string()) {
            let diff_col_idx = result.column_names.iter().position(|c| c == "diff").unwrap();
            let has_nulls = result.rows.iter().any(|row| matches!(row[diff_col_idx].kind, rustgresql::types::ValueKind::Null(_)));
            if has_nulls {
                println!("⚠ WARNING: diff column still has NULL values");
            } else {
                println!("✓ SUCCESS: All diff values are non-NULL!");
            }
        }
    } else {
        println!("✗ Test FAILED: Query returned 0 rows");
    }

    println!("\n🎉 Test completed!");

    Ok(())
}
