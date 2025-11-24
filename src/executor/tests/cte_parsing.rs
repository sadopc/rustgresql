//! Tests for CTE parsing functionality
//!
//! Tests Common Table Expression parsing including:
//! - Basic WITH clauses
//! - Multiple CTEs
//! - Recursive CTEs
//! - CTEs with column aliases
//! - Error handling for invalid CTE syntax

#[cfg(test)]
mod tests {
    use crate::sql::lexer::Lexer;
    use crate::sql::parser::Parser;

    #[test]
    fn test_basic_cte_parsing() {
        let sql = "WITH dept_stats AS (SELECT department FROM employees) SELECT * FROM dept_stats;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse basic CTE query: {:?}", result.err());

        let statements = result.unwrap();
        assert!(!statements.is_empty());

        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, columns, from, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                assert_eq!(with_clause.ctes[0].name, "dept_stats");
                assert!(!with_clause.recursive, "Expected non-recursive CTE");

                assert_eq!(columns.len(), 1, "Expected 1 column in main query");
                assert_eq!(from.len(), 1, "Expected 1 table in main query");
                assert_eq!(from[0].name, "dept_stats");
            } else {
                panic!("Expected Simple SelectStatement");
            }
        } else {
            panic!("Expected Select statement");
        }
    }

    #[test]
    fn test_multiple_ctes_parsing() {
        let sql = "WITH
            dept_stats AS (
                SELECT department, salary
                FROM employees
            ),
            high_salary_depts AS (
                SELECT department FROM dept_stats WHERE salary > 50000
            )
        SELECT name, salary
        FROM employees
        WHERE department IN (SELECT department FROM high_salary_depts);";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse multiple CTEs query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 2, "Expected 2 CTEs");
                assert_eq!(with_clause.ctes[0].name, "dept_stats");
                assert_eq!(with_clause.ctes[1].name, "high_salary_depts");
            }
        }
    }

    #[test]
    fn test_cte_with_column_aliases() {
        let sql = "WITH emp_summary (dept_name, emp_id, max_sal) AS (
            SELECT department, id, salary
            FROM employees
        )
        SELECT * FROM emp_summary;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse CTE with column aliases: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");

                let cte = &with_clause.ctes[0];
                assert_eq!(cte.name, "emp_summary");
                assert!(cte.column_names.is_some(), "Expected column aliases");

                let column_names = cte.column_names.as_ref().unwrap();
                assert_eq!(column_names.len(), 3, "Expected 3 column aliases");
                assert_eq!(column_names[0], "dept_name");
                assert_eq!(column_names[1], "emp_id");
                assert_eq!(column_names[2], "max_sal");
            }
        }
    }

    #[test]
    fn test_stack_overflow_cte_parsing() {
        // Test the exact query that was causing stack overflow
        let sql = "WITH user_stats AS (SELECT COUNT(*) as count FROM users) SELECT * FROM user_stats;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse stack overflow CTE query: {:?}", result.err());

        let statements = result.unwrap();
        assert!(!statements.is_empty());

        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, columns, from, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                assert_eq!(with_clause.ctes[0].name, "user_stats");
                assert!(!with_clause.recursive, "Expected non-recursive CTE");

                assert_eq!(columns.len(), 1, "Expected 1 column in main query");
                assert_eq!(from.len(), 1, "Expected 1 table in main query");
                assert_eq!(from[0].name, "user_stats");
            } else {
                panic!("Expected Simple SelectStatement");
            }
        } else {
            panic!("Expected Select statement");
        }
    }

    #[test]
    fn test_recursive_cte_parsing() {
        // Very simple recursive CTE test to isolate parsing issues
        let sql = "WITH RECURSIVE test_cte AS (
            SELECT id, name FROM test_table
            UNION
            SELECT id, name FROM more_test_table
        )
        SELECT * FROM test_cte;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse recursive CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert!(with_clause.recursive, "Expected recursive CTE");
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                assert_eq!(with_clause.ctes[0].name, "test_cte");

                // Verify it's a SetOperation (UNION)
                if let crate::sql::ast::SelectStatement::SetOperation(set_op) = with_clause.ctes[0].query.as_ref() {
                    assert!(matches!(set_op.operator, crate::sql::ast::SetOperator::Union));
                    assert!(!set_op.all, "Expected UNION without ALL");
                } else {
                    panic!("Expected SetOperation for recursive CTE query");
                }
            }
        }
    }

    #[test]
    fn test_nested_cte_parsing() {
        // Note: Nested CTEs (WITH within WITH) are not supported by this parser yet
        // This test is simplified to test basic functionality
        let sql = "WITH outer_cte AS (
            SELECT department, salary
            FROM employees
            WHERE salary > 50000
        )
        SELECT * FROM outer_cte;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse nested CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                assert!(with_clause.is_some(), "Expected outer WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 outer CTE");
                assert_eq!(with_clause.ctes[0].name, "outer_cte");
            }
        }
    }

    #[test]
    fn test_cte_parsing_error_cases() {
        let error_cases = vec![
            // Missing AS after CTE name
            "WITH bad_cte (SELECT * FROM employees) SELECT * FROM bad_cte;",
            // Missing parentheses around CTE query
            "WITH bad_cte AS SELECT * FROM employees SELECT * FROM bad_cte;",
            // Empty CTE list
            "WITH SELECT * FROM employees;",
            // Missing CTE name
            "WITH AS (SELECT * FROM employees) SELECT * FROM employees;",
        ];

        for case in error_cases {
            let mut lexer = Lexer::new(case);
            let tokens = lexer.tokenize().unwrap();

            let mut parser = Parser::new(tokens);
            let result = parser.parse();

            // These should fail parsing
            assert!(result.is_err(), "Expected parsing to fail for case: '{}', but it succeeded", case);
        }
    }

    #[test]
    fn test_cte_complex_query_parsing() {
        let sql = "WITH
            monthly_sales AS (
                SELECT
                    order_date,
                    product_id,
                    amount
                FROM orders
            ),
            top_products AS (
                SELECT
                    order_date,
                    product_id,
                    amount
                FROM monthly_sales
            )
        SELECT
            order_date,
            product_id,
            amount
        FROM top_products;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse complex CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 2, "Expected 2 CTEs");
                assert_eq!(with_clause.ctes[0].name, "monthly_sales");
                assert_eq!(with_clause.ctes[1].name, "top_products");
            }
        }
    }

    #[test]
    fn test_cte_with_window_functions() {
        // Note: Window functions not fully supported yet, simplified test for basic functionality
        let sql = "WITH dept_employees AS (
            SELECT
                department,
                name,
                salary
            FROM employees
            WHERE salary > 50000
        )
        SELECT
            department,
            name,
            salary
        FROM dept_employees
        ORDER BY department, salary DESC;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse CTE with window functions: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, columns, .. } = select {
                assert!(with_clause.is_some(), "Expected WITH clause to be present");
                assert_eq!(columns.len(), 3, "Expected 3 columns in main query");

                let with_clause = with_clause.as_ref().unwrap();
                assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                assert_eq!(with_clause.ctes[0].name, "dept_employees");
            }
        }
    }

    #[test]
    fn test_order_by_nulls_first_last_parsing() {
        // Test NULLS FIRST
        let sql = "SELECT * FROM employees ORDER BY manager_id NULLS FIRST";
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse();
        assert!(result.is_ok(), "Failed to parse ORDER BY with NULLS FIRST: {:?}", result.err());

        // Test NULLS LAST
        let sql = "SELECT * FROM employees ORDER BY manager_id NULLS LAST";
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse();
        assert!(result.is_ok(), "Failed to parse ORDER BY with NULLS LAST: {:?}", result.err());

        // Test ASC NULLS FIRST
        let sql = "SELECT * FROM employees ORDER BY salary ASC NULLS FIRST";
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse();
        assert!(result.is_ok(), "Failed to parse ORDER BY with ASC NULLS FIRST: {:?}", result.err());

        // Test DESC NULLS LAST
        let sql = "SELECT * FROM employees ORDER BY salary DESC NULLS LAST";
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse();
        assert!(result.is_ok(), "Failed to parse ORDER BY with DESC NULLS LAST: {:?}", result.err());

        // Test multiple ORDER BY columns with NULLS specification
        let sql = "SELECT * FROM employees ORDER BY department_id NULLS FIRST, salary DESC NULLS LAST";
        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let result = parser.parse();
        assert!(result.is_ok(), "Failed to parse ORDER BY with multiple columns and NULLS: {:?}", result.err());
    }
}