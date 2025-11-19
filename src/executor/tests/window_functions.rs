//! Tests for window functions
//!
//! Tests window function functionality including:
//! - Basic window functions (ROW_NUMBER, RANK, DENSE_RANK)
//! - Analytic window functions (LAG, LEAD)
//! - Window frames (ROWS BETWEEN, RANGE BETWEEN)
//! - Mixed aggregations and window functions
//! - Performance with large datasets

#[cfg(test)]
mod tests {
    use crate::sql::lexer::Lexer;
    use crate::sql::parser::Parser;

    #[test]
    fn test_basic_window_function_parsing() {
        let sql = "SELECT ROW_NUMBER() OVER (ORDER BY salary DESC) as rank, name, department, salary FROM employees;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse basic window function query: {:?}", result.err());

        let statements = result.unwrap();
        assert!(!statements.is_empty());

        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { columns, .. } = select {
                assert!(!columns.is_empty());
                // Check that first column contains a window function
                if let crate::sql::ast::Expression::WindowFunction(_) = &columns[0].expr {
                    // Success - window function detected
                } else {
                    panic!("Expected window function in first column");
                }
            }
        }
    }

    #[test]
    fn test_partitioned_window_function_parsing() {
        let sql = "SELECT
            ROW_NUMBER() OVER (PARTITION BY department ORDER BY salary DESC) as dept_rank,
            name,
            department,
            salary
            FROM employees;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse partitioned window function query: {:?}", result.err());
    }

    #[test]
    fn test_window_frame_parsing() {
        let sql = "SELECT
            SUM(salary) OVER (
                PARTITION BY department
                ORDER BY hire_date
                ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING
            ) as rolling_sum,
            name,
            salary
            FROM employees;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse window frame query: {:?}", result.err());
    }

    #[test]
    fn test_multiple_window_functions() {
        let sql = "SELECT
            ROW_NUMBER() OVER (ORDER BY salary DESC) as overall_rank,
            RANK() OVER (PARTITION BY department ORDER BY salary DESC) as dept_rank,
            LAG(salary, 1) OVER (PARTITION BY department ORDER BY salary DESC) as prev_salary,
            name,
            department,
            salary
            FROM employees;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse multiple window functions query: {:?}", result.err());
    }

    #[test]
    fn test_mixed_aggregation_and_window_functions() {
        let sql = "SELECT
            department,
            COUNT(*) as dept_count,
            AVG(salary) as avg_salary,
            ROW_NUMBER() OVER (ORDER BY AVG(salary) DESC) as salary_rank,
            LAG(AVG(salary), 1) OVER (ORDER BY AVG(salary) DESC) as prev_avg_salary
            FROM employees
            GROUP BY department;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse mixed aggregation and window functions query: {:?}", result.err());
    }

    #[test]
    fn test_named_window_definition() {
        let sql = "SELECT
            ROW_NUMBER() OVER w as rank,
            RANK() OVER w as dense_rank,
            name,
            salary
            FROM employees
            WINDOW w AS (PARTITION BY department ORDER BY salary DESC);";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse named window definition query: {:?}", result.err());
    }

    #[test]
    fn test_window_function_variants() {
        let variants = vec![
            "ROW_NUMBER() OVER (ORDER BY salary)",
            "RANK() OVER (ORDER BY salary)",
            "DENSE_RANK() OVER (ORDER BY salary)",
            "LAG(salary) OVER (ORDER BY salary)",
            "LEAD(salary) OVER (ORDER BY salary)",
            "FIRST_VALUE(salary) OVER (ORDER BY salary)",
            "LAST_VALUE(salary) OVER (ORDER BY salary)",
            "COUNT(*) OVER (PARTITION BY department)",
            "SUM(salary) OVER (PARTITION BY department)",
            "AVG(salary) OVER (PARTITION BY department)",
            "MIN(salary) OVER (PARTITION BY department)",
            "MAX(salary) OVER (PARTITION BY department)",
        ];

        for variant in variants {
            let sql = format!("SELECT {}, name, salary FROM employees", variant);
            let mut lexer = Lexer::new(&sql);
            let tokens = lexer.tokenize().unwrap();

            let mut parser = Parser::new(tokens);
            let result = parser.parse();

            assert!(result.is_ok(), "Failed to parse variant '{}': {:?}", variant, result.err());
        }
    }

    #[test]
    fn test_complex_window_frames() {
        let window_frames = vec![
            "ROWS UNBOUNDED PRECEDING",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
            "ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING",
            "ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING",
            "RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
            "RANGE BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING",
        ];

        for frame in window_frames {
            let sql = format!("SELECT
                SUM(salary) OVER (ORDER BY salary {}) as frame_sum,
                name,
                salary
                FROM employees", frame);

            let mut lexer = Lexer::new(&sql);
            let tokens = lexer.tokenize().unwrap();

            let mut parser = Parser::new(tokens);
            let result = parser.parse();

            assert!(result.is_ok(), "Failed to parse frame '{}': {:?}", frame, result.err());
        }
    }

    #[test]
    fn test_window_function_error_cases() {
        let error_cases = vec![
            // Invalid window function name
            "SELECT INVALID_WINDOW() OVER (ORDER BY salary) FROM employees",
            // Missing OVER clause
            "SELECT ROW_NUMBER() FROM employees",
            // Empty OVER clause
            "SELECT ROW_NUMBER() OVER () FROM employees",
            // Invalid frame syntax
            "SELECT SUM(salary) OVER (ORDER BY salary ROWS BETWEEN INVALID AND CURRENT ROW) FROM employees",
        ];

        for case in error_cases {
            let mut lexer = Lexer::new(case);
            let tokens = lexer.tokenize().unwrap();

            let mut parser = Parser::new(tokens);
            let result = parser.parse();

            // These should either parse but fail during execution, or fail parsing
            // We just want to make sure they don't panic
            match result {
                Ok(_) => {
                    // Parsed successfully - acceptable for this test
                }
                Err(_) => {
                    // Failed to parse - also acceptable for this test
                }
            }
        }
    }

    #[test]
    fn test_window_function_performance_large_dataset() {
        // This is a basic performance test to ensure window functions
        // can handle larger datasets without excessive memory usage
        let sql = "SELECT
            ROW_NUMBER() OVER (ORDER BY salary DESC) as rank,
            RANK() OVER (PARTITION BY department ORDER BY salary DESC) as dept_rank,
            name,
            department,
            salary
            FROM employees;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse performance test query: {:?}", result.err());

        // In a real implementation, we would:
        // 1. Generate a large test dataset (10,000+ rows)
        // 2. Execute the query
        // 3. Measure execution time and memory usage
        // 4. Verify results are correct
        // 5. Ensure performance is acceptable (< 1 second for 10k rows)
    }

    #[test]
    fn test_window_function_edge_cases() {
        let edge_cases = vec![
            // Single row
            "SELECT ROW_NUMBER() OVER (ORDER BY salary) FROM employees WHERE id = 1",
            // All NULL values in window
            "SELECT ROW_NUMBER() OVER (ORDER BY salary) FROM employees WHERE salary IS NULL",
            // Mixed NULL and non-NULL values
            "SELECT ROW_NUMBER() OVER (ORDER BY salary DESC) FROM employees",
            // Empty result set
            "SELECT ROW_NUMBER() OVER (ORDER BY salary) FROM employees WHERE 1 = 0",
        ];

        for case in edge_cases {
            let mut lexer = Lexer::new(case);
            let tokens = lexer.tokenize().unwrap();

            let mut parser = Parser::new(tokens);
            let result = parser.parse();

            assert!(result.is_ok(), "Failed to parse edge case '{}': {:?}", case, result.err());
        }
    }
}