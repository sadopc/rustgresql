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
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

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
                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    // Create CTE operator using the parsed WITH clause
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
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
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

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

                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    // Create CTE operator using the parsed WITH clause
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
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
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

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
                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
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
                        // Both UNION and UNION ALL are now supported for recursive CTEs
                    // assert!(!set_op.all, "Recursive CTEs should use UNION, not UNION ALL");
                    } else {
                        panic!("Expected SetOperation for recursive CTE query");
                    }
                }
            }
        }
    }

    #[test]
    fn test_recursive_cte_union_all_support() {
        use crate::executor::operators::CTEOperator;
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

        // Test that UNION ALL is now supported for recursive CTEs
        let sql = "WITH RECURSIVE employee_hierarchy AS (
            SELECT id, name, manager_id, 0 AS level FROM employees WHERE manager_id IS NULL
            UNION ALL
            SELECT e.id, e.name, e.manager_id, eh.level + 1
            FROM employees e
            JOIN employee_hierarchy eh ON e.manager_id = eh.id
        )
        SELECT id, name, level FROM employee_hierarchy ORDER BY level, name;";

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse UNION ALL recursive CTE query: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    assert!(with_clause.recursive, "Expected recursive CTE flag to be true");
                    assert_eq!(with_clause.ctes.len(), 1);
                    assert_eq!(with_clause.ctes[0].name, "employee_hierarchy");

                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    // Create CTE operator using the parsed WITH clause
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
                    );

                    // Verify the CTE query is a SetOperation with UNION ALL
                    let cte_query = &cte_operator.with_clause.ctes[0].query;
                    if let crate::sql::ast::SelectStatement::SetOperation(set_op) = cte_query.as_ref() {
                        assert!(matches!(set_op.operator, crate::sql::ast::SetOperator::Union));
                        assert!(set_op.all, "Expected UNION ALL for recursive CTE");
                    } else {
                        panic!("Expected SetOperation for recursive CTE query");
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
    fn test_multiple_recursive_ctes_linear_dependency() {
        use crate::executor::operators::CTEOperator;
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

        // Test linear dependency chain: level1 -> level2 -> level3
        let sql = r#"
        WITH RECURSIVE
            level1 AS (
                SELECT 1 as id UNION ALL SELECT id + 1 FROM level1 WHERE id < 3
            ),
            level2 AS (
                SELECT id * 10 as value FROM level1
            ),
            level3 AS (
                SELECT value * 100 as result FROM level2
            )
        SELECT * FROM level3 ORDER BY result;
        "#;

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse multiple recursive CTEs with linear dependencies: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    assert!(with_clause.recursive, "Expected recursive CTE flag to be true");
                    assert_eq!(with_clause.ctes.len(), 3);

                    // Verify each CTE has the correct recursive flag
                    assert_eq!(with_clause.ctes[0].name, "level1");
                    assert!(with_clause.ctes[0].recursive, "level1 should be recursive");

                    assert_eq!(with_clause.ctes[1].name, "level2");
                    assert!(!with_clause.ctes[1].recursive, "level2 should be non-recursive");

                    assert_eq!(with_clause.ctes[2].name, "level3");
                    assert!(!with_clause.ctes[2].recursive, "level3 should be non-recursive");

                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    // Create CTE operator - this should work without errors
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
                    );

                    // Verify the CTE operator was created successfully
                    assert_eq!(cte_operator.with_clause.ctes.len(), 3);
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
    fn test_multiple_recursive_ctes_mixed_dependencies() {
        use crate::executor::operators::CTEOperator;
        use crate::catalog::CatalogManager;
        use std::sync::Arc;

        // Test mixed recursive and non-recursive dependencies
        let sql = r#"
        WITH RECURSIVE
            employees_hierarchy AS (
                SELECT id, name, manager_id, 0 AS level FROM employees WHERE manager_id IS NULL
                UNION ALL
                SELECT e.id, e.name, e.manager_id, eh.level + 1
                FROM employees e
                JOIN employees_hierarchy eh ON e.manager_id = eh.id
            ),
            department_stats AS (
                SELECT department_id, COUNT(*) as employee_count FROM employees_hierarchy GROUP BY department_id
            ),
            category_tree AS (
                SELECT id, name, parent_id, 0 AS depth FROM categories WHERE parent_id IS NULL
                UNION ALL
                SELECT c.id, c.name, c.parent_id, ct.depth + 1
                FROM categories c
                JOIN category_tree ct ON c.parent_id = ct.id
            )
        SELECT eh.name, ds.employee_count FROM employees_hierarchy eh
        LEFT JOIN department_stats ds ON eh.department_id = ds.department_id
        WHERE eh.level <= 2;
        "#;

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Failed to parse mixed recursive/non-recursive CTEs: {:?}", result.err());

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    assert!(with_clause.recursive, "Expected recursive CTE flag to be true");
                    assert_eq!(with_clause.ctes.len(), 3);

                    // Verify recursive flags
                    assert_eq!(with_clause.ctes[0].name, "employees_hierarchy");
                    assert!(with_clause.ctes[0].recursive, "employees_hierarchy should be recursive");

                    assert_eq!(with_clause.ctes[1].name, "department_stats");
                    assert!(!with_clause.ctes[1].recursive, "department_stats should be non-recursive");

                    assert_eq!(with_clause.ctes[2].name, "category_tree");
                    assert!(with_clause.ctes[2].recursive, "category_tree should be recursive");

                    // Create a catalog for the CTE operator
                    let catalog = Arc::new(CatalogManager::new());

                    // Create CTE operator
                    let cte_operator = CTEOperator::new(
                        with_clause.clone(),
                        crate::sql::ast::Statement::Select(select.clone()),
                        catalog
                    );

                    // Verify the CTE operator was created successfully
                    assert_eq!(cte_operator.with_clause.ctes.len(), 3);
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
    fn test_multiple_recursive_ctes_error_mutual_recursion() {
        // This test should fail if mutually recursive CTEs are properly detected
        let sql = r#"
        WITH RECURSIVE
            cte1 AS (
                SELECT * FROM cte2
            ),
            cte2 AS (
                SELECT * FROM cte1
            )
        SELECT * FROM cte1;
        "#;

        let mut lexer = crate::sql::lexer::Lexer::new(sql);
        let tokens = lexer.tokenize().unwrap();

        let mut parser = crate::sql::parser::Parser::new(tokens);
        let result = parser.parse();

        assert!(result.is_ok(), "Parsing should succeed - mutual recursion is a runtime error");

        let statements = result.unwrap();
        if let crate::sql::ast::Statement::Select(select) = &statements[0] {
            if let crate::sql::ast::SelectStatement::Simple { with_clause, .. } = select {
                if let Some(with_clause) = with_clause {
                    assert!(with_clause.recursive, "Expected recursive CTE flag to be true");
                    assert_eq!(with_clause.ctes.len(), 2);

                    // Both CTEs should be non-recursive since they don't contain UNION operations
                    assert!(!with_clause.ctes[0].recursive, "cte1 should be non-recursive (no UNION)");
                    assert!(!with_clause.ctes[1].recursive, "cte2 should be non-recursive (no UNION)");
                }
            }
        }
    }

    #[test]
    fn test_dependency_graph_construction() {
        use crate::executor::operators::{CTEDependencyGraph, CTENode};
        use crate::sql::ast::{CommonTableExpression, SelectStatement, WithClause};
        use std::collections::HashMap;

        // Create test CTEs with dependencies
        let cte1 = CommonTableExpression {
            name: "cte1".to_string(),
            column_names: None,
            query: Box::new(SelectStatement::SetOperation(crate::sql::ast::SetOperation {
                operator: crate::sql::ast::SetOperator::Union,
                left: Box::new(SelectStatement::Simple {
                    with_clause: None,
                    distinct: false,
                    columns: vec![],
                    from: vec![crate::sql::ast::TableRef::Table {
                        name: "base_table".to_string(),
                        alias: None,
                    }],
                    joins: vec![],
                    where_clause: None,
                    group_by: vec![],
                    having: None,
                    order_by: vec![],
                    limit: None,
                    offset: None,
                    named_windows: vec![],
                }),
                right: Box::new(SelectStatement::Simple {
                    with_clause: None,
                    distinct: false,
                    columns: vec![],
                    from: vec![crate::sql::ast::TableRef::Table {
                        name: "cte1".to_string(),
                        alias: None,
                    }],
                    joins: vec![],
                    where_clause: None,
                    group_by: vec![],
                    having: None,
                    order_by: vec![],
                    limit: None,
                    offset: None,
                    named_windows: vec![],
                }),
                all: false,
            })),
            recursive: true,
        };

        let cte2 = CommonTableExpression {
            name: "cte2".to_string(),
            column_names: None,
            query: Box::new(SelectStatement::Simple {
                with_clause: None,
                distinct: false,
                columns: vec![],
                from: vec![crate::sql::ast::TableRef::Table {
                    name: "cte1".to_string(),
                    alias: None,
                }],
                joins: vec![],
                where_clause: None,
                group_by: vec![],
                having: None,
                order_by: vec![],
                limit: None,
                offset: None,
                named_windows: vec![],
            }),
            recursive: false,
        };

        // Test dependency graph construction
        let mut graph = CTEDependencyGraph::new();
        graph.add_cte(&cte1);
        graph.add_cte(&cte2);

        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.nodes.contains_key("cte1"));
        assert!(graph.nodes.contains_key("cte2"));

        // cte1 should be recursive
        assert!(graph.nodes["cte1"].is_recursive);
        // cte2 should be non-recursive
        assert!(!graph.nodes["cte2"].is_recursive);

        // Test execution order building
        let result = graph.build_execution_order();
        assert!(result.is_ok(), "Failed to build execution order: {:?}", result.err());

        // Should have 2 groups: non-recursive first, then recursive
        assert_eq!(graph.execution_order.len(), 2);
        // First group should contain non-recursive CTEs
        assert_eq!(graph.execution_order[0].len(), 1);
        assert_eq!(graph.execution_order[0][0], "cte2");
        // Second group should contain recursive CTEs
        assert_eq!(graph.execution_order[1].len(), 1);
        assert_eq!(graph.execution_order[1][0], "cte1");
    }
}