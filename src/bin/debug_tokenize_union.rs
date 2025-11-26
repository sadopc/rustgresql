// Debug script to trace tokenization of UNION ALL queries
use rustgresql::sql::lexer::Lexer;
use rustgresql::sql::Token;

fn debug_tokenize(query: &str, description: &str) {
    println!("\n=== {} ===", description);
    println!("Query: {}", query);
    println!("\nTokens:");

    let mut lexer = Lexer::new(query);
    match lexer.tokenize() {
        Ok(tokens) => {
            for (i, token) in tokens.iter().enumerate() {
                println!("{:2}: {:<20} {:>10}:{:<3} {}",
                    i,
                    format!("{:?}", token.token_type),
                    token.line,
                    token.column,
                    if token.value.is_empty() { "".to_string() } else { format!("'{}'", token.value) }
                );
            }
        }
        Err(e) => {
            println!("Tokenization error: {}", e);
        }
    }
}

fn main() {
    // Test 1: Simple SELECT that works
    debug_tokenize(
        "SELECT 'High Earners' AS category;",
        "Simple working SELECT"
    );

    // Test 2: UNION ALL that fails
    debug_tokenize(
        "SELECT 'High' AS category UNION ALL SELECT 'Low' AS category;",
        "UNION ALL with simple literals"
    );

    // Test 3: Complex UNION ALL that fails
    debug_tokenize(
        "SELECT 'High Earners' AS category, COUNT(*) AS count UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count;",
        "UNION ALL with COUNT"
    );
}