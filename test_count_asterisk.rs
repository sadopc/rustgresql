use rustgresql::sql::{lexer::Lexer, parser::Parser};

fn main() {
    let sql = "SELECT COUNT(*) as total FROM users GROUP BY age;";
    
    println!("Testing SQL: {}", sql);
    
    // Test lexer
    let mut lexer = Lexer::new(sql);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => {
            println!("Lexer succeeded:");
            for (i, token) in tokens.iter().enumerate() {
                println!("  {}: {:?} at line {}, col {} = '{}'", 
                         i, token.token_type, token.line, token.column, token.value);
            }
            tokens
        }
        Err(e) => {
            println!("Lexer failed: {:?}", e);
            return;
        }
    };
    
    // Test parser
    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(statements) => {
            println!("Parser succeeded:");
            for (i, stmt) in statements.iter().enumerate() {
                println!("  {}: {:?}", i, stmt);
            }
        }
        Err(e) => {
            println!("Parser failed: {:?}", e);
        }
    }
}