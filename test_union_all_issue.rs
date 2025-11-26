// Test script to reproduce the UNION ALL parsing issue
use rustgresql::sql::parser::Parser;

fn main() {
    let test_query = "SELECT 'High Earners' AS category, COUNT(*) AS count FROM employees WHERE salary > 80000.00 UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count FROM employees WHERE salary <= 60000.00 UNION ALL SELECT 'Mid Range' AS category, COUNT(*) AS count FROM employees WHERE salary > 60000.00 AND salary <= 80000.00;";

    println!("Testing UNION ALL query:");
    println!("{}", test_query);
    println!();

    match Parser::parse_sql(test_query) {
        Ok(statements) => {
            println!("✅ Parse successful!");
            println!("Number of statements: {}", statements.len());
            for (i, stmt) in statements.iter().enumerate() {
                println!("Statement {}: {:?}", i, stmt);
            }
        }
        Err(e) => {
            println!("❌ Parse error: {}", e);
        }
    }
}