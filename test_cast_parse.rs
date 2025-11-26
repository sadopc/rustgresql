use rustgresql::sql::parser::parse_sql;

fn main() {
    let sql = "SELECT CAST('hello' AS VARCHAR(100))";
    match parse_sql(sql) {
        Ok(_) => println!("Parse succeeded"),
        Err(e) => println!("Parse failed: {:?}", e),
    }
}
