use rustgresql::sql::{lexer::Lexer, parser::Parser};

fn test_sql(sql: &str, description: &str) {
    println!("\n=== {} ===", description);
    println!("SQL: {}", sql);
    
    // Test lexer
    let mut lexer = Lexer::new(sql);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(e) => {
            println!("Lexer failed: {:?}", e);
            return;
        }
    };
    
    // Test parser
    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(statements) => {
            println!("✓ Parser succeeded");
            for (i, stmt) in statements.iter().enumerate() {
                println!("  Statement {}: {:?}", i, stmt);
            }
        }
        Err(e) => {
            println!("✗ Parser failed: {:?}", e);
        }
    }
}

fn main() {
    // Test cases
    test_sql("SELECT COUNT(*) as total FROM users GROUP BY age;", 
             "COUNT(*) with AS alias");
    
    test_sql("SELECT COUNT(*) FROM users;", 
             "COUNT(*) without alias");
    
    test_sql("SELECT COUNT(*) total FROM users;", 
             "COUNT(*) with implicit alias (no AS)");
    
    test_sql("SELECT COUNT(*), SUM(age) as total_age FROM users;", 
             "Multiple functions with mixed alias styles");
    
    test_sql("SELECT * FROM users;", 
             "Simple SELECT *");
    
    test_sql("SELECT name as user_name, age FROM users;", 
             "Columns with AS alias");
    
    test_sql("SELECT name user_name, age FROM users;", 
             "Columns with implicit alias");
}