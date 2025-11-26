// Debug script to isolate UNION ALL parsing issue
use rustgresql::sql::parser::parse_sql;

fn test_query(query: &str, description: &str) {
    println!("\n=== {} ===", description);
    println!("Query: {}", query);
    match parse_sql(query) {
        Ok(_) => {
            println!("✅ Parse successful!");
        }
        Err(e) => {
            println!("❌ Parse error: {}", e);
        }
    }
}

fn main() {
    // Test 1: Simple single SELECT with string literal
    test_query(
        "SELECT 'High Earners' AS category;",
        "Single SELECT with string literal"
    );

    // Test 2: Simple UNION ALL with simple literals (no spaces)
    test_query(
        "SELECT 'High' AS category UNION ALL SELECT 'Low' AS category;",
        "UNION ALL with simple string literals"
    );

    // Test 3: UNION ALL with spaces in first string literal only
    test_query(
        "SELECT 'High Earners' AS category UNION ALL SELECT 'Low' AS category;",
        "UNION ALL with spaces in first string"
    );

    // Test 4: UNION ALL with spaces in second string literal only
    test_query(
        "SELECT 'High' AS category UNION ALL SELECT 'Low Earners' AS category;",
        "UNION ALL with spaces in second string"
    );

    // Test 5: Original problematic query
    test_query(
        "SELECT 'High Earners' AS category, COUNT(*) AS count FROM employees WHERE salary > 80000.00 UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count FROM employees WHERE salary <= 60000.00;",
        "Original query with two UNION ALL parts"
    );
}