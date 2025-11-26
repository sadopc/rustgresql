// Simple test to verify window function arithmetic fix
use rustgresql::executor::planner::QueryPlanner;
use rustgresql::sql::ast::{Expression, Statement, SelectStatement, ColumnExpr};
use std::collections::HashMap;

fn main() {
    println!("Testing window function arithmetic fix...\n");

    // Test the specific case: salary - AVG(salary) OVER (...)
    let sql = "SELECT e.salary - AVG(e.salary) OVER (PARTITION BY e.department_id) as salary_diff FROM employees e";

    // Parse the query
    let lexer = rustpgsql::sql::lexer::Lexer::new(sql);
    let tokens = lexer.tokenize().expect("Failed to tokenize");
    let mut parser = rustpgsql::sql::parser::Parser::new(tokens);
    let statements = parser.parse().expect("Failed to parse");

    println!("✓ Query parsed successfully");

    if let Statement::Select(select) = &statements[0] {
        if let SelectStatement::Simple { columns, .. } = select {
            let expr = &columns[0].expr;
            println!("✓ Found expression: {:?}", expr);

            // Test the QueryPlanner
            let planner = QueryPlanner::new();

            // Check if it contains window functions
            let contains_wf = planner.contains_window_functions(expr);
            println!("✓ Contains window functions: {}", contains_wf);

            if !contains_wf {
                eprintln!("❌ ERROR: Expression should contain window functions!");
                std::process::exit(1);
            }

            // Extract window functions
            let mut counter = 0;
            let (extracted_funcs, modified_expr) = planner.extract_window_functions_from_expression(expr, &mut counter);

            println!("✓ Extracted {} window function(s)", extracted_funcs.len());
            println!("✓ Modified expression: {:?}", modified_expr);

            // Verify the structure
            match &modified_expr {
                Expression::BinaryOp { left, op, right } => {
                    println!("✓ Expression is BinaryOp with operator: {:?}", op);
                    println!("  - Left: {:?}", left);
                    println!("  - Right: {:?}", right);

                    // Right side should be a column reference to the window function
                    if let Expression::Column { name, .. } = right.as_ref() {
                        println!("✓ Right side is a column reference: {}", name);
                        if !name.starts_with("win_func_") {
                            eprintln!("❌ ERROR: Column name should start with 'win_func_', got: {}", name);
                            std::process::exit(1);
                        }
                    } else {
                        eprintln!("❌ ERROR: Right side should be a Column reference, got: {:?}", right);
                        std::process::exit(1);
                    }
                }
                _ => {
                    eprintln!("❌ ERROR: Modified expression should be BinaryOp, got: {:?}", modified_expr);
                    std::process::exit(1);
                }
            }

            // Verify the extracted window function
            if extracted_funcs.is_empty() {
                eprintln!("❌ ERROR: Should have extracted at least one window function!");
                std::process::exit(1);
            }

            println!("✓ Window function extraction successful");
            println!("\n🎉 All tests passed! Window function arithmetic fix is working correctly.");
        }
    }
}
