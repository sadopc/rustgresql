//! Query planner
//!
//! Converts SQL AST into execution plans

use crate::{Result, sql::ast::*, executor::operators::{*, HashJoinOperator, MergeJoinOperator}, executor::scanner::TableScanner};

/// Execution plan
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub root: PlanNode,
    pub output_schema: Vec<(String, crate::types::DataType)>,
}

/// Plan node
#[derive(Debug, Clone)]
pub enum PlanNode {
    /// Table scan
    Scan {
        table_name: String,
        /// Column indices to project (empty means all columns)
        columns: Vec<String>,
    },
    /// Index scan
    IndexScan {
        table_name: String,
        index_name: String,
        index_condition: Option<Expression>,
        columns: Vec<String>,
    },
    /// Index-only scan (covering index)
    IndexOnlyScan {
        table_name: String,
        index_name: String,
        index_condition: Option<Expression>,
        columns: Vec<String>,
    },
    /// Filter operation
    Filter {
        input: Box<PlanNode>,
        condition: Expression,
    },
    /// Project operation
    Project {
        input: Box<PlanNode>,
        columns: Vec<(String, Expression)>,
    },
    /// Join operation
    Join {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expression>,
        join_type: JoinType,
    },
    /// Hash join operation
    HashJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expression>,
        join_type: JoinType,
        hash_key_columns: Vec<String>,
    },
    /// Merge join operation
    MergeJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expression>,
        join_type: JoinType,
        sort_columns: Vec<String>,
    },
    /// Insert operation
    Insert {
        table_name: String,
        columns: Vec<String>,
        values: Vec<Vec<Expression>>,
    },
    /// Update operation
    Update {
        table_name: String,
        assignments: Vec<(String, Expression)>,
        condition: Option<Expression>,
    },
    /// Delete operation
    Delete {
        table_name: String,
        condition: Option<Expression>,
    },
    /// Aggregate operation (GROUP BY and aggregate functions)
    Aggregate {
        input: Box<PlanNode>,
        group_by_columns: Vec<Expression>,
        aggregate_functions: Vec<(String, Expression)>,
        having_clause: Option<Expression>,
    },
    /// Set operation (UNION, INTERSECT, EXCEPT)
    SetOperation {
        operator: crate::sql::ast::SetOperator,
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        all: bool,
    },
    /// Subquery execution
    Subquery {
        query: Box<crate::sql::ast::Statement>,
        // For correlated subqueries, we need to track outer context
        correlated_columns: Vec<String>,
    },
    /// Values operator for in-memory data
    Values {
        rows: Vec<Vec<crate::types::Value>>,
        column_names: Vec<String>,
    },
    /// CTE (Common Table Expression) operator
    CTE {
        with_clause: crate::sql::ast::WithClause,
        main_query: Box<crate::sql::ast::Statement>,
    },
}

impl PlanNode {
    /// Execute the plan node and return results
    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        match self {
            PlanNode::Scan { table_name, .. } => {
                // Create TableScanner with catalog and buffer manager from context
                if let (Some(catalog), Some(buffer_manager)) = (context.get_catalog(), context.get_buffer_manager()) {
                    match TableScanner::new(catalog.clone(), buffer_manager.clone(), table_name) {
                        Ok(scanner) => {
                            let operator = ScanOperator::with_scanner(table_name.clone(), scanner);
                            operator.execute(context)
                        }
                        Err(_) => {
                            // Fall back to empty scanner if table doesn't exist or can't be scanned
                            let operator = ScanOperator::new(table_name.clone());
                            operator.execute(context)
                        }
                    }
                } else {
                    let operator = ScanOperator::new(table_name.clone());
                    operator.execute(context)
                }
            }
            PlanNode::IndexScan { table_name, index_name, index_condition, columns } => {
                let operator = IndexScanOperator::new(
                    table_name.clone(),
                    index_name.clone(),
                    index_condition.clone(),
                    columns.clone(),
                );
                operator.execute(context)
            }
            PlanNode::IndexOnlyScan { table_name, index_name, index_condition, columns } => {
                let operator = IndexOnlyScanOperator::new(
                    table_name.clone(),
                    index_name.clone(),
                    index_condition.clone(),
                    columns.clone(),
                );
                operator.execute(context)
            }
            PlanNode::Filter { input, condition } => {
                let input_plan = input.as_ref().clone();
                let operator = FilterOperator::new(input_plan, condition.clone());
                operator.execute(context)
            }
            PlanNode::Project { input, columns } => {
                let input_plan = input.as_ref().clone();
                let operator = ProjectOperator::new(input_plan, columns.clone());
                operator.execute(context)
            }
            PlanNode::Join { left, right, condition, join_type } => {
                let left_plan = left.as_ref().clone();
                let right_plan = right.as_ref().clone();
                let operator = JoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone());
                operator.execute(context)
            }
            PlanNode::HashJoin { left, right, condition, join_type, hash_key_columns } => {
                let left_plan = left.as_ref().clone();
                let right_plan = right.as_ref().clone();
                let operator = HashJoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone(), hash_key_columns.clone());
                operator.execute(context)
            }
            PlanNode::MergeJoin { left, right, condition, join_type, sort_columns } => {
                let left_plan = left.as_ref().clone();
                let right_plan = right.as_ref().clone();
                let operator = MergeJoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone(), sort_columns.clone());
                operator.execute(context)
            }
            PlanNode::Insert { table_name, columns, values } => {
                // Create TableScanner with catalog and buffer manager from context
                if let (Some(catalog), Some(buffer_manager)) = (context.get_catalog(), context.get_buffer_manager()) {
                    match TableScanner::new(catalog.clone(), buffer_manager.clone(), table_name) {
                        Ok(scanner) => {
                            let operator = InsertOperator::with_scanner(table_name.clone(), columns.clone(), values.clone(), scanner);
                            operator.execute(context)
                        }
                        Err(_) => {
                            // Fall back to without scanner if table doesn't exist
                            let operator = InsertOperator::new(table_name.clone(), columns.clone(), values.clone());
                            operator.execute(context)
                        }
                    }
                } else {
                    let operator = InsertOperator::new(table_name.clone(), columns.clone(), values.clone());
                    operator.execute(context)
                }
            }
            PlanNode::Update { table_name, assignments, condition } => {
                // Create TableScanner with catalog and buffer manager from context
                if let (Some(catalog), Some(buffer_manager)) = (context.get_catalog(), context.get_buffer_manager()) {
                    match TableScanner::new(catalog.clone(), buffer_manager.clone(), table_name) {
                        Ok(scanner) => {
                            let operator = UpdateOperator::with_scanner(table_name.clone(), assignments.clone(), condition.clone(), scanner);
                            operator.execute(context)
                        }
                        Err(_) => {
                            let operator = UpdateOperator::new(table_name.clone(), assignments.clone(), condition.clone());
                            operator.execute(context)
                        }
                    }
                } else {
                    let operator = UpdateOperator::new(table_name.clone(), assignments.clone(), condition.clone());
                    operator.execute(context)
                }
            }
            PlanNode::Delete { table_name, condition } => {
                // Create TableScanner with catalog and buffer manager from context
                if let (Some(catalog), Some(buffer_manager)) = (context.get_catalog(), context.get_buffer_manager()) {
                    match TableScanner::new(catalog.clone(), buffer_manager.clone(), table_name) {
                        Ok(scanner) => {
                            let operator = DeleteOperator::with_scanner(table_name.clone(), condition.clone(), scanner);
                            operator.execute(context)
                        }
                        Err(_) => {
                            let operator = DeleteOperator::new(table_name.clone(), condition.clone());
                            operator.execute(context)
                        }
                    }
                } else {
                    let operator = DeleteOperator::new(table_name.clone(), condition.clone());
                    operator.execute(context)
                }
            }
            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                let input_plan = input.as_ref().clone();
                let operator = AggregateOperator::new(input_plan, group_by_columns.clone(), aggregate_functions.clone(), having_clause.clone());
                operator.execute(context)
            }
            PlanNode::SetOperation { operator, left, right, all } => {
                let set_operator = crate::executor::operators::SetOperationOperator::new(
                    operator.clone(),
                    *left.clone(),
                    *right.clone(),
                    *all,
                );
                set_operator.execute(context)
            }
            PlanNode::Subquery { query, correlated_columns } => {
                let operator = crate::executor::operators::SubqueryOperator::new(
                    *query.clone(),
                    correlated_columns.clone(),
                );
                operator.execute(context)
            }
            PlanNode::Values { rows, column_names } => {
                Ok(QueryResult {
                    rows: rows.clone(),
                    column_names: column_names.clone(),
                })
            }
            PlanNode::CTE { with_clause, main_query } => {
                let cte_operator = CTEOperator::new(with_clause.clone(), *main_query.clone());
                cte_operator.execute(context)
            }
        }
    }
}

/// Query planner
#[derive(Debug)]
pub struct QueryPlanner {
    // In a real implementation, this would have access to catalog metadata
    // For now, we'll keep it simple
}

impl QueryPlanner {
    pub fn new() -> Self {
        Self { }
    }

    /// Create execution plan from SELECT statement
    pub fn plan_select(&self, select: &SelectStatement) -> Result<ExecutionPlan> {
        match select {
            SelectStatement::Simple {
                with_clause,
                from,
                joins,
                where_clause,
                columns,
                group_by,
                having,
                ..
            } => {
                // Check if this is a CTE query first
                if let Some(with_clause) = with_clause {
                    return self.plan_cte_select(with_clause, select);
                }
                // Start with table scans
                let mut plan = self.plan_from_clause(from)?;

                // Apply joins
                for join in joins {
                    let right_plan = self.plan_table_scan(&join.table.name)?;
                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: join.condition.clone(),
                        join_type: join.join_type,
                    };
                }

                // Apply WHERE clause
                if let Some(ref where_clause) = where_clause {
                    let planned_where = self.plan_subqueries_in_expression(where_clause)?;
                    plan = PlanNode::Filter {
                        input: Box::new(plan),
                        condition: planned_where,
                    };
                }

                // Apply column projection
                if columns.len() == 1 && matches!(columns[0], Expression::Star) {
                    // SELECT * - no explicit projection needed
                } else {
                    let projections: Result<Vec<(String, Expression)>> = columns
                        .iter()
                        .enumerate()
                        .map(|(i, expr)| {
                            let planned_expr = self.plan_subqueries_in_expression(expr)?;
                            let column_name = match expr {
                                Expression::Column { name, .. } => name.clone(),
                                Expression::Star => "*".to_string(),
                                Expression::Subquery(_) => format!("subquery{}", i),
                                _ => format!("col{}", i),
                            };
                            Ok((column_name, planned_expr))
                        })
                        .collect();

                    plan = PlanNode::Project {
                        input: Box::new(plan),
                        columns: projections?,
                    };
                }

                // Apply GROUP BY and aggregate functions
                if !group_by.is_empty() || self.has_aggregate_functions(columns) {
                    plan = self.plan_aggregation(plan, select)?;
                }

                // Apply HAVING clause if present
                if let Some(ref having_clause) = having {
                    let planned_having = self.plan_subqueries_in_expression(having_clause)?;
                    plan = PlanNode::Filter {
                        input: Box::new(plan),
                        condition: planned_having,
                    };
                }

                // Create output schema (simplified)
                let output_schema = match &plan {
                    PlanNode::Scan { table_name, .. } => {
                        // In a real implementation, this would query the catalog
                        vec![("column1".to_string(), crate::types::DataType::new(crate::types::DataTypeKind::Text))]
                    }
                    PlanNode::Project { columns, .. } => {
                        columns.iter().map(|(name, _)| {
                            (name.clone(), crate::types::DataType::new(crate::types::DataTypeKind::Text))
                        }).collect()
                    }
                    _ => vec![],
                };

                Ok(ExecutionPlan {
                    root: plan,
                    output_schema,
                })
            }
            SelectStatement::SetOperation(set_op) => {
                // For now, create a placeholder plan for set operations
                // This will be implemented in the next phase
                let left_plan = self.plan_select(&set_op.left)?;
                let right_plan = self.plan_select(&set_op.right)?;

                let plan = PlanNode::SetOperation {
                    operator: set_op.operator,
                    left: Box::new(left_plan.root),
                    right: Box::new(right_plan.root),
                    all: set_op.all,
                };

                Ok(ExecutionPlan {
                    root: plan,
                    output_schema: left_plan.output_schema, // Simplified - should merge schemas
                })
            }
        }
    }

    /// Create execution plan from INSERT statement
    pub fn plan_insert(&self, insert: &InsertStatement) -> Result<ExecutionPlan> {
        Ok(ExecutionPlan {
            root: PlanNode::Insert {
                table_name: insert.table.name.clone(),
                columns: insert.columns.clone(),
                values: insert.values.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create execution plan from UPDATE statement
    pub fn plan_update(&self, update: &UpdateStatement) -> Result<ExecutionPlan> {
        Ok(ExecutionPlan {
            root: PlanNode::Update {
                table_name: update.table.name.clone(),
                assignments: update.assignments.clone(),
                condition: update.where_clause.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create execution plan from DELETE statement
    pub fn plan_delete(&self, delete: &DeleteStatement) -> Result<ExecutionPlan> {
        Ok(ExecutionPlan {
            root: PlanNode::Delete {
                table_name: delete.table.name.clone(),
                condition: delete.where_clause.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create plan for FROM clause
    fn plan_from_clause(&self, from: &[TableRef]) -> Result<PlanNode> {
        match from {
            [] => Err(crate::error::RustgreSQLError::Parse("No tables in FROM clause".to_string())),
            [table] => self.plan_table_scan(&table.name),
            tables => {
                // Multiple tables without explicit joins - create cross joins
                let mut plan = self.plan_table_scan(&tables[0].name)?;
                for table in &tables[1..] {
                    let right_plan = self.plan_table_scan(&table.name)?;
                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: None,
                        join_type: JoinType::Inner,
                    };
                }
                Ok(plan)
            }
        }
    }

    /// Create table scan plan
    fn plan_table_scan(&self, table_name: &str) -> Result<PlanNode> {
        // In a real implementation, this would validate the table exists
        // and get column information from the catalog
        Ok(PlanNode::Scan {
            table_name: table_name.to_string(),
            columns: vec![], // Empty means all columns
        })
    }

    /// Check if any expressions contain aggregate functions
    fn has_aggregate_functions(&self, expressions: &[Expression]) -> bool {
        self.contains_aggregate_functions_expressions(expressions)
    }

    /// Recursively check if expressions contain aggregate functions
    fn contains_aggregate_functions_expressions(&self, expressions: &[Expression]) -> bool {
        for expr in expressions {
            if self.is_aggregate_function(expr) {
                return true;
            }
            if let Expression::Function { args, .. } = expr {
                if self.contains_aggregate_functions_expressions(args) {
                    return true;
                }
            }
            if let Expression::BinaryOp { left, right, .. } = expr {
                // Check left and right sub-expressions
                let left_exprs = vec![(**left).clone()];
                let right_exprs = vec![(**right).clone()];
                if self.contains_aggregate_functions_expressions(&left_exprs) ||
                   self.contains_aggregate_functions_expressions(&right_exprs) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if an expression is an aggregate function
    fn is_aggregate_function(&self, expr: &Expression) -> bool {
        if let Expression::Function { name, .. } = expr {
            matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
        } else {
            false
        }
    }

    /// Plan aggregation (GROUP BY and aggregate functions)
    fn plan_aggregation(&self, input: PlanNode, select: &SelectStatement) -> Result<PlanNode> {
        match select {
            SelectStatement::Simple { group_by, columns, having, .. } => {
                // Extract aggregate functions from SELECT list
                let mut aggregate_functions = Vec::new();
                let mut group_by_columns = group_by.clone();

                // Add GROUP BY columns from SELECT list that aren't aggregate functions
                for (i, expr) in columns.iter().enumerate() {
                    if !self.is_aggregate_function(expr) && !matches!(expr, Expression::Star) {
                        // Check if this column is already in GROUP BY
                        if !group_by.iter().any(|g_expr| self.expressions_equal(g_expr, expr)) {
                            // For SQL compliance, non-aggregated columns must be in GROUP BY
                            // In PostgreSQL, this would be an error, but for now we'll add them automatically
                            group_by_columns.push(expr.clone());
                        }
                    }
                }

                // Extract aggregate functions with their aliases
                for (i, expr) in columns.iter().enumerate() {
                    if self.is_aggregate_function(expr) {
                        let alias = match expr {
                            Expression::Function { name, .. } => {
                                format!("{}_{}", name.to_lowercase(), i)
                            }
                            _ => format!("aggregate{}", i),
                        };
                        aggregate_functions.push((alias, expr.clone()));
                    } else if matches!(expr, Expression::Star) && !group_by.is_empty() {
                        // SELECT * with GROUP BY - this is complex, for now treat as COUNT(*)
                        let count_star = Expression::Function {
                            name: "COUNT".to_string(),
                            args: vec![Expression::Star],
                        };
                        let alias = format!("count_star_{}", i);
                        aggregate_functions.push((alias, count_star));
                    }
                }

                // Handle COUNT(*) case
                if columns.len() == 1 && matches!(&columns[0], Expression::Star) {
                    let count_star = Expression::Function {
                        name: "COUNT".to_string(),
                        args: vec![Expression::Star],
                    };
                    aggregate_functions.push(("count".to_string(), count_star));
                }

                Ok(PlanNode::Aggregate {
                    input: Box::new(input),
                    group_by_columns,
                    aggregate_functions,
                    having_clause: having.clone(),
                })
            }
            SelectStatement::SetOperation(_) => {
                // For now, set operations with aggregation are not supported
                Err(crate::error::RustgreSQLError::Internal(
                    "Aggregation with set operations is not yet supported".to_string()
                ))
            }
        }
    }

    /// Check if two expressions are equal (simplified comparison)
    fn expressions_equal(&self, a: &Expression, b: &Expression) -> bool {
        match (a, b) {
            (Expression::Column { name: name_a, .. }, Expression::Column { name: name_b, .. }) => {
                name_a == name_b
            }
            (Expression::Value(val_a), Expression::Value(val_b)) => {
                format!("{:?}", val_a) == format!("{:?}", val_b)
            }
            (Expression::Function { name: name_a, args: args_a }, Expression::Function { name: name_b, args: args_b }) => {
                name_a == name_b && args_a.len() == args_b.len()
            }
            _ => false,
        }
    }

    /// Detect subqueries in an expression and return a modified expression with Subquery plan nodes
    fn plan_subqueries_in_expression(&self, expr: &Expression) -> Result<Expression> {
        match expr {
            Expression::Subquery(subquery_stmt) => {
                // For now, keep subqueries as-is in expressions
                // Correlation detection will happen during expression evaluation
                Ok(expr.clone())
            }
            Expression::BinaryOp { left, op, right } => {
                let left_planned = self.plan_subqueries_in_expression(left)?;
                let right_planned = self.plan_subqueries_in_expression(right)?;
                Ok(Expression::BinaryOp {
                    left: Box::new(left_planned),
                    op: *op,
                    right: Box::new(right_planned),
                })
            }
            Expression::Function { name, args } => {
                let planned_args: Result<Vec<Expression>> = args
                    .iter()
                    .map(|arg| self.plan_subqueries_in_expression(arg))
                    .collect();
                Ok(Expression::Function {
                    name: name.clone(),
                    args: planned_args?,
                })
            }
            Expression::UnaryOp { op, expr } => {
                let planned_expr = self.plan_subqueries_in_expression(expr)?;
                Ok(Expression::UnaryOp {
                    op: *op,
                    expr: Box::new(planned_expr),
                })
            }
            _ => Ok(expr.clone()),
        }
    }

    /// Check if an expression contains any subqueries
    fn contains_subquery(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Subquery(_) => true,
            Expression::BinaryOp { left, right, .. } => {
                self.contains_subquery(left) || self.contains_subquery(right)
            }
            Expression::Function { args, .. } => {
                args.iter().any(|arg| self.contains_subquery(arg))
            }
            Expression::UnaryOp { expr, .. } => {
                self.contains_subquery(expr)
            }
            _ => false,
        }
    }

    /// Plan a CTE SELECT statement
    fn plan_cte_select(&self, with_clause: &WithClause, select: &SelectStatement) -> Result<ExecutionPlan> {
        // Create a CTE plan node that will handle the entire WITH clause
        let plan_node = PlanNode::CTE {
            with_clause: with_clause.clone(),
            main_query: Box::new(crate::sql::ast::Statement::Select(select.clone())),
        };

        // For now, we'll use the column names from the main query
        // In a full implementation, we'd need to derive the output schema from the main query
        let output_schema = Vec::new();

        Ok(ExecutionPlan {
            root: plan_node,
            output_schema,
        })
    }
}
