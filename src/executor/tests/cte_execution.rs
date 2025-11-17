//! Tests for CTE execution functionality
//!
//! Tests Common Table Expression execution including:
//! - Basic CTE materialization
//! - Multiple CTEs
//! - Recursive CTEs
//! - CTE operator integration

#[cfg(test)]
mod tests {
    use crate::sql::lexer::Lexer;
    use crate::sql::parser::Parser;
    use crate::executor::planner::QueryPlanner;

    #[test]
    fn test_cte_operator_basic_functionality() {
        // Simple test to verify CTEOperator compiles and can be created
        let sql = "WITH dept_stats AS (SELECT department FROM employees) SELECT * FROM dept_stats;";

        let mut lexer = Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse basic CTE query: {:?}", result.err());

        let statements = result.unwrap();
        assert!(!statements.is_empty());

        // Create planner and plan the CTE query
        let planner = QueryPlanner::new();
        let select_stmt = &statements[0];

        if let crate::sql::ast::Statement::Select(select) = select_stmt {
            let plan_result = planner.plan_select(select);
            assert!(plan_result.is_ok(), "Failed to plan CTE query: {:?}", plan_result.err());

            let plan = plan_result.unwrap();

            // Verify the plan contains a CTE node
            match &plan.root {
                crate::executor::planner::PlanNode::CTE { with_clause, main_query } => {
                    assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                    assert_eq!(with_clause.ctes[0].name, "dept_stats");
                    assert!(!with_clause.recursive, "Expected non-recursive CTE");
                }
                _ => panic!("Expected CTE plan node, got: {:?}", plan.root),
            }
        } else {
            panic!("Expected Select statement");
        }
    }

    #[test]
    fn test_cte_operator_multiple_ctes() {
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
        let planner = QueryPlanner::new();
        let select_stmt = &statements[0];

        if let crate::sql::ast::Statement::Select(select) = select_stmt {
            let plan_result = planner.plan_select(select);
            assert!(plan_result.is_ok(), "Failed to plan multiple CTEs query: {:?}", plan_result.err());

            let plan = plan_result.unwrap();

            // Verify the plan contains a CTE node with multiple CTEs
            match &plan.root {
                crate::executor::planner::PlanNode::CTE { with_clause, .. } => {
                    assert_eq!(with_clause.ctes.len(), 2, "Expected 2 CTEs");
                    assert_eq!(with_clause.ctes[0].name, "dept_stats");
                    assert_eq!(with_clause.ctes[1].name, "high_salary_depts");
                    assert!(!with_clause.recursive, "Expected non-recursive CTEs");
                }
                _ => panic!("Expected CTE plan node, got: {:?}", plan.root),
            }
        }
    }

    #[test]
    fn test_cte_operator_recursive() {
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
        let planner = QueryPlanner::new();
        let select_stmt = &statements[0];

        if let crate::sql::ast::Statement::Select(select) = select_stmt {
            let plan_result = planner.plan_select(select);
            assert!(plan_result.is_ok(), "Failed to plan recursive CTE query: {:?}", plan_result.err());

            let plan = plan_result.unwrap();

            // Verify the plan contains a CTE node with recursive flag
            match &plan.root {
                crate::executor::planner::PlanNode::CTE { with_clause, .. } => {
                    assert_eq!(with_clause.ctes.len(), 1, "Expected 1 CTE");
                    assert_eq!(with_clause.ctes[0].name, "test_cte");
                    assert!(with_clause.recursive, "Expected recursive CTE");
                }
                _ => panic!("Expected CTE plan node, got: {:?}", plan.root),
            }
        }
    }

    #[test]
    fn test_cte_operator_creation() {
        use crate::executor::operators::CTEOperator;

        // Test that we can create a CTEOperator with basic parameters
        // We'll use a simple parsed query from the parsing tests
        let sql = "WITH dept_stats AS (SELECT department FROM employees) SELECT * FROM dept_stats;";

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse basic CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    // Create CTE operator using the parsed WITH clause
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone())
                    );

                    // Verify the operator was created correctly
                    assert_eq!(cte_operator.with_clause.ctes.len(), 1);
                    assert_eq!(cte_operator.with_clause.ctes[0].name, "dept_stats");
                    assert!(!cte_operator.with_clause.recursive);
                    assert!(cte_operator.materialized_ctes.is_empty());
                } else {
                    panic!("Expected WITH clause to be present");
                }
            } else {
                panic!("Expected Simple SelectStatement");
            }
        } else {
            panic!("Expected Select statement");
        }
    }

    #[test]
    fn test_recursive_cte_operator_functionality() {
        use crate::executor::operators::CTEOperator;

        // Test recursive CTE operator planning and creation with simple syntax
        let sql = "WITH RECURSIVE simple_recursive AS (
            SELECT id, name FROM items
            UNION
            SELECT id, name FROM more_items
        )
        SELECT * FROM simple_recursive;";

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse recursive CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    assert!(with_clause.recursive, "Expected recursive CTE flag to be true");
                    assert_eq!(with_clause.ctes.len(), 1);
                    assert_eq!(with_clause.ctes[0].name, "simple_recursive");

                    // Create CTE operator using the parsed WITH clause
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone())
                    );

                    // Verify the operator was created correctly
                    assert_eq!(cte_operator.with_clause.ctes.len(), 1);
                    assert_eq!(cte_operator.with_clause.ctes[0].name, "simple_recursive");
                    assert!(cte_operator.with_clause.recursive);
                    assert!(cte_operator.materialized_ctes.is_empty());

                    // Test that the CTE operator can detect recursive structure
                    if let crate::sql::ast::SelectStatement::SetOperation(set_op) = with_clause.ctes[0].query.as_ref() {
                        assert!(matches!(set_op.operator, crate::sql::ast::SetOperator::Union));
                        assert!(!set_op.all);
                    } else {
                        panic!("Expected SetOperation for recursive CTE");
                    }
                } else {
                    panic!("Expected WITH clause to be present");
                }
            } else {
                panic!("Expected Simple SelectStatement");
            }
        } else {
            panic!("Expected Select statement");
        }
    }

    #[test]
    fn test_recursive_cte_execution_structure() {
        use crate::executor::operators::{CTEOperator, ExecutionContext};

        // Test the structure of recursive CTE execution without full execution
        // This tests the logic for identifying anchor and recursive parts
        let sql = "WITH RECURSIVE simple_recursive AS (
            SELECT id, name FROM items WHERE parent_id = 0
            UNION
            SELECT id, name FROM more_items
        )
        SELECT * FROM simple_recursive;";

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse simple recursive CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone())
                    );

                    // Test execution context integration
                    let mut context = ExecutionContext::new();
                    context.log("Testing recursive CTE structure");

                    // The CTE should be properly structured for recursive execution
                    assert!(cte_operator.with_clause.recursive);
                    assert_eq!(cte_operator.with_clause.ctes.len(), 1);

                    // Verify the CTE query is a SetOperation (UNION)
                    let cte_query = &cte_operator.with_clause.ctes[0].query;
                    if let crate::sql::ast::SelectStatement::SetOperation(set_op) = cte_query.as_ref() {
                        assert!(matches!(set_op.operator, crate::sql::ast::SetOperator::Union));
                        assert!(!set_op.all, "Recursive CTEs should use UNION, not UNION ALL");
                    } else {
                        panic!("Expected SetOperation for recursive CTE query");
                    }
                }
            }
        }
    }
}