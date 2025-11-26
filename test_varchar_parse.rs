use rustgresql::sql::parser::parse_sql;

fn main() {
    let sql = "SELECT CAST('hello' AS VARCHAR)";
    match parse_sql(sql) {
        Ok(_) => println!("Parse succeeded"),
        Err(e) => println!("Parse failed: {:?}", e),
    }
}
