#[cfg(test)]
mod test {
    use rustgresql::sql::parser::parse_sql;

    #[test]
    fn test_union_all_parsing() {
        let sql = "SELECT 'High Earners' AS category, COUNT(*) AS count FROM employees WHERE salary > 80000.00 UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count FROM employees WHERE salary <= 60000.00 UNION ALL SELECT 'Mid Range' AS category, COUNT(*) AS count FROM employees WHERE salary > 60000.00 AND salary <= 80000.00;";

        println!("Testing UNION ALL query parsing...");
        println!("SQL: {}", sql);

        match parse_sql(sql) {
            Ok(statements) => {
                println!("Successfully parsed {} statements", statements.len());
                assert!(statements.len() > 0);
            }
            Err(e) => {
                println!("Parse error: {:?}", e);
                panic!("Failed to parse UNION ALL query: {:?}", e);
            }
        }
    }
}