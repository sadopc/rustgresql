//! Query planner
//!
//! Converts SQL AST into execution plans

use crate::{Result, sql::ast::*, executor::operators::{*, HashJoinOperator, MergeJoinOperator}, executor::scanner::TableScanner};
use std::collections::HashMap;

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
        alias: Option<String>,
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
        table_aliases: HashMap<String, String>,
        left_columns: Option<Vec<String>>,
        right_columns: Option<Vec<String>>,
    },
    /// Join operation
    Join {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expression>,
        join_type: JoinType,
        left_alias: Option<String>,
        right_alias: Option<String>,
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
    /// Parallel Scan operation
    ParallelScan {
        table_name: String,
        columns: Vec<String>,
    },
    /// Parallel Hash Join operation
    ParallelHashJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expression>,
        join_type: JoinType,
        hash_key_columns: Vec<String>,
    },
    /// Parallel Aggregate operation
    ParallelAggregate {
        input: Box<PlanNode>,
        group_by_columns: Vec<Expression>,
        aggregate_functions: Vec<(String, Expression)>,
        having_clause: Option<Expression>,
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
    /// Window function operation
    Window {
        input: Box<PlanNode>,
        window_functions: Vec<(String, Expression)>,
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
    /// CTE Scan operator for accessing materialized CTE results
    CTEScan {
        cte_name: String,
        alias: Option<String>,
    },
    /// Sort operation
    Sort {
        input: Box<PlanNode>,
        order_by: Vec<OrderBy>,
    },
    /// Limit operation
    Limit {
        input: Box<PlanNode>,
        limit: i64,
        offset: Option<i64>,
    },
    /// Distinct operation (remove duplicate rows)
    Distinct {
        input: Box<PlanNode>,
    },
}

impl PlanNode {
    /// Execute the plan node and return results
    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        println!("DEBUG: PlanNode::execute called with {:?}", std::mem::discriminant(self));
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
            PlanNode::Project { input, columns, .. } => {
                println!("DEBUG: Executing Project with {} columns", columns.len());
                let input_plan = input.as_ref().clone();
                let operator = ProjectOperator::new(input_plan, columns.clone());
                operator.execute(context)
            }
            PlanNode::Join { left, right, condition, join_type, left_alias, right_alias } => {
                let left_plan = left.as_ref().clone();
                let right_plan = right.as_ref().clone();
                let operator = JoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone(), left_alias.clone(), right_alias.clone());
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
            PlanNode::ParallelScan { table_name, columns } => {
                #[cfg(feature = "parallel")]
                {
                    // For now, fall back to serial execution until ParallelExecutor is fully integrated
                    // In a real implementation, this would delegate to ParallelExecutor
                    let operator = ScanOperator::new(table_name.clone());
                    operator.execute(context)
                }
                #[cfg(not(feature = "parallel"))]
                {
                    let operator = ScanOperator::new(table_name.clone());
                    operator.execute(context)
                }
            }
            PlanNode::ParallelHashJoin { left, right, condition, join_type, hash_key_columns } => {
                 #[cfg(feature = "parallel")]
                {
                    // Fallback to serial hash join
                    let left_plan = left.as_ref().clone();
                    let right_plan = right.as_ref().clone();
                    let operator = HashJoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone(), hash_key_columns.clone());
                    operator.execute(context)
                }
                #[cfg(not(feature = "parallel"))]
                {
                    let left_plan = left.as_ref().clone();
                    let right_plan = right.as_ref().clone();
                    let operator = HashJoinOperator::new(left_plan, right_plan, condition.clone(), join_type.clone(), hash_key_columns.clone());
                    operator.execute(context)
                }
            }
            PlanNode::ParallelAggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                #[cfg(feature = "parallel")]
                {
                    // Fallback to serial aggregation
                    let input_plan = input.as_ref().clone();
                    let operator = AggregateOperator::new(input_plan, group_by_columns.clone(), aggregate_functions.clone(), having_clause.clone());
                    operator.execute(context)
                }
                 #[cfg(not(feature = "parallel"))]
                {
                    let input_plan = input.as_ref().clone();
                    let operator = AggregateOperator::new(input_plan, group_by_columns.clone(), aggregate_functions.clone(), having_clause.clone());
                    operator.execute(context)
                }
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
            PlanNode::Window { input, window_functions } => {
                println!("DEBUG: Executing Window with {} window functions", window_functions.len());
                let input_plan = input.as_ref().clone();

                // Extract WindowFunction objects from expressions
                let mut window_funcs = Vec::new();
                let mut partition_by = Vec::new();
                let mut order_by = Vec::new();
                let mut window_frame = None;

                for (name, expr) in window_functions {
                    if let Expression::WindowFunction(ref wf) = expr {
                        // Take window clause from first function (simplified)
                        if window_funcs.is_empty() {
                            partition_by = wf.window_clause.partition_by.clone();
                            order_by = wf.window_clause.order_by.clone();
                            window_frame = wf.window_clause.window_frame.clone();
                        }
                        window_funcs.push((name.clone(), wf.clone()));
                    }
                }

                let operator = WindowOperator::new(input_plan, window_funcs, partition_by, order_by, window_frame);
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
                let catalog = context.get_catalog().ok_or_else(|| {
                    crate::error::RustgreSQLError::Execution("Catalog not available in execution context for CTE".to_string())
                })?;
                let mut cte_operator = CTEOperator::new(with_clause.clone(), *main_query.clone(), catalog.clone());
                cte_operator.execute(context)
            }
            PlanNode::CTEScan { cte_name, alias } => {
                let cte_scan_operator = crate::executor::operators::CTEScanOperator::new(
                    cte_name.clone()
                );
                cte_scan_operator.execute(context)
            }
            PlanNode::Sort { input, order_by } => {
                let input_plan = input.as_ref().clone();
                let operator = crate::executor::operators::SortOperator::new(input_plan, order_by.clone());
                operator.execute(context)
            }
            PlanNode::Limit { input, limit, offset } => {
                let input_plan = input.as_ref().clone();
                let operator = crate::executor::operators::LimitOperator::new(input_plan, *limit, offset.clone());
                operator.execute(context)
            }
            PlanNode::Distinct { input } => {
                let input_plan = input.as_ref().clone();
                let operator = crate::executor::operators::DistinctOperator::new(input_plan);
                operator.execute(context)
            }
        }
    }
}

/// Query planner
#[derive(Debug)]
pub struct QueryPlanner {
    pub catalog: Option<std::sync::Arc<crate::catalog::CatalogManager>>,
    materialized_ctes: std::collections::HashMap<String, QueryResult>,
}

impl QueryPlanner {
    pub fn new() -> Self {
        Self {
            catalog: None,
            materialized_ctes: std::collections::HashMap::new(),
        }
    }

    pub fn with_catalog(catalog: std::sync::Arc<crate::catalog::CatalogManager>) -> Self {
        Self {
            catalog: Some(catalog),
            materialized_ctes: std::collections::HashMap::new(),
        }
    }

    pub fn with_ctes(catalog: Option<std::sync::Arc<crate::catalog::CatalogManager>>, materialized_ctes: std::collections::HashMap<String, QueryResult>) -> Self {
        Self {
            catalog,
            materialized_ctes,
        }
    }

    /// Create execution plan from SELECT statement
    pub fn plan_select(&self, select: &SelectStatement) -> Result<ExecutionPlan> {
        println!("DEBUG: plan_select called");
        match select {
            SelectStatement::Simple {
                distinct,
                with_clause,
                from,
                joins,
                where_clause,
                columns,
                group_by,
                having,
                order_by,
                limit,
                offset,
                ..
            } => {
                println!("DEBUG: plan_select - Simple statement with {} columns", columns.len());
                // Check if this is a CTE query first
                if let Some(with_clause) = with_clause {
                    return self.plan_cte_select(with_clause, select);
                }
                // Start with table scans
                let mut plan = self.plan_from_clause(from)?;

                // Apply joins
                for join in joins {
                    let right_plan = self.plan_table_ref(&join.table)?;

                    // Extract left alias from the current plan
                    let left_alias = self.extract_alias_from_plan(&plan);

                    // Extract right alias from table ref
                    let right_alias = match &join.table {
                        TableRef::Table { alias, .. } => alias.clone(),
                        TableRef::Subquery { alias, .. } => alias.clone(),
                    };

                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: join.condition.clone(),
                        join_type: join.join_type,
                        left_alias,
                        right_alias,
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

                // Apply GROUP BY and aggregate functions
                if !group_by.is_empty() || self.has_aggregate_functions(&columns.iter().map(|c| c.expr.clone()).collect::<Vec<_>>()) {
                    plan = self.plan_aggregation(plan, select)?;
                }

                // Apply column projection
                if columns.len() == 1 && matches!(columns[0].expr, Expression::Star) {
                    // SELECT * - no explicit projection needed
                } else {
                    // Check if we have aggregation
                    let has_aggregation = !group_by.is_empty() || self.has_aggregate_functions(&columns.iter().map(|c| c.expr.clone()).collect::<Vec<_>>());
                    println!("DEBUG: has_aggregation = {}, group_by.is_empty() = {}", has_aggregation, group_by.is_empty());

                    let projections: Result<Vec<(String, Expression)>> = columns
                        .iter()
                        .enumerate()
                        .map(|(i, col_spec)| {
                            let planned_expr = self.plan_subqueries_in_expression(&col_spec.expr)?;

                            let final_expr = if has_aggregation {
                                // Skip rewriting for subqueries - they should be evaluated independently
                                if matches!(planned_expr, Expression::Subquery(_)) {
                                    planned_expr
                                } else if self.is_aggregate_function(&planned_expr) {
                                     let alias = if let Some(alias) = &col_spec.alias {
                                        alias.clone()
                                    } else {
                                        match &planned_expr {
                                            Expression::Function { name, .. } => {
                                                format!("{}_{}", name.to_lowercase(), i)
                                            }
                                            _ => format!("aggregate{}", i),
                                        }
                                    };
                                    Expression::Column { table: None, name: alias }
                                } else if matches!(planned_expr, Expression::Star) && !group_by.is_empty() {
                                     let alias = if let Some(alias) = &col_spec.alias {
                                        alias.clone()
                                    } else {
                                        format!("count_star_{}", i)
                                    };
                                    Expression::Column { table: None, name: alias }
                                } else {
                                    planned_expr
                                }
                            } else {
                                planned_expr
                            };

                            let column_name = if let Some(alias) = &col_spec.alias {
                                alias.clone()
                            } else {
                                match &col_spec.expr {
                                    Expression::Column { name, .. } => name.clone(),
                                    Expression::Star => "*".to_string(),
                                    Expression::Subquery(_) => format!("subquery{}", i),
                                    _ => format!("col{}", i),
                                }
                            };
                            Ok((column_name, final_expr))
                        })
                        .collect();

                    let all_projections = projections?;

                    // Separate window functions from regular projections
                    println!("DEBUG: Before separation, all_projections has {} items", all_projections.len());
                    let (window_funcs, regular_projections) = self.separate_window_functions(&all_projections);
                    println!("DEBUG: Separated {} window functions and {} regular projections", window_funcs.len(), regular_projections.len());
                    println!("DEBUG: Window function aliases: {:?}", window_funcs.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>());

                    // Apply regular projections if any
                    if !regular_projections.is_empty() || window_funcs.is_empty() {
                        let columns_to_use = if window_funcs.is_empty() { &all_projections } else { &regular_projections };
                        println!("DEBUG: Creating Project with {} columns", columns_to_use.len());
                        plan = PlanNode::Project {
                            input: Box::new(plan),
                            columns: columns_to_use.clone(),
                            table_aliases: std::collections::HashMap::new(),
                            left_columns: None,
                            right_columns: None,
                        };
                    }

                    // Apply window functions if any
                    if !window_funcs.is_empty() {
                        println!("DEBUG: Creating Window node on top with {} window functions", window_funcs.len());
                        plan = PlanNode::Window {
                            input: Box::new(plan),
                            window_functions: window_funcs,
                        };
                    }
                }


                // Apply HAVING clause if present
                if let Some(ref having_clause) = having {
                    let planned_having = self.plan_subqueries_in_expression(having_clause)?;
                    plan = PlanNode::Filter {
                        input: Box::new(plan),
                        condition: planned_having,
                    };
                }

                // Apply ORDER BY clause if present
                if !order_by.is_empty() {
                    plan = PlanNode::Sort {
                        input: Box::new(plan),
                        order_by: order_by.clone(),
                    };
                }

                // Apply LIMIT clause if present
                if let Some(limit_val) = limit {
                    plan = PlanNode::Limit {
                        input: Box::new(plan),
                        limit: *limit_val,
                        offset: offset.clone(),
                    };
                }

                // Apply DISTINCT if present
                if *distinct {
                    plan = PlanNode::Distinct {
                        input: Box::new(plan),
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
        // Extract table name from TableRef (only simple tables allowed for INSERT)
        let table_name = match &insert.table {
            TableRef::Table { name, .. } => name.clone(),
            TableRef::Subquery { .. } => {
                return Err(crate::RustgreSQLError::Parse("INSERT into subquery is not supported".to_string()));
            }
        };

        Ok(ExecutionPlan {
            root: PlanNode::Insert {
                table_name,
                columns: insert.columns.clone(),
                values: insert.values.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create execution plan from UPDATE statement
    pub fn plan_update(&self, update: &UpdateStatement) -> Result<ExecutionPlan> {
        // Extract table name from TableRef (only simple tables allowed for UPDATE)
        let table_name = match &update.table {
            TableRef::Table { name, .. } => name.clone(),
            TableRef::Subquery { .. } => {
                return Err(crate::RustgreSQLError::Parse("UPDATE subquery is not supported".to_string()));
            }
        };

        Ok(ExecutionPlan {
            root: PlanNode::Update {
                table_name,
                assignments: update.assignments.clone(),
                condition: update.where_clause.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create execution plan from DELETE statement
    pub fn plan_delete(&self, delete: &DeleteStatement) -> Result<ExecutionPlan> {
        // Extract table name from TableRef (only simple tables allowed for DELETE)
        let table_name = match &delete.table {
            TableRef::Table { name, .. } => name.clone(),
            TableRef::Subquery { .. } => {
                return Err(crate::RustgreSQLError::Parse("DELETE from subquery is not supported".to_string()));
            }
        };

        Ok(ExecutionPlan {
            root: PlanNode::Delete {
                table_name,
                condition: delete.where_clause.clone(),
            },
            output_schema: vec![],
        })
    }

    /// Create plan for FROM clause
    fn plan_from_clause(&self, from: &[TableRef]) -> Result<PlanNode> {
        match from {
            [] => Err(crate::error::RustgreSQLError::Parse("No tables in FROM clause".to_string())),
            [table] => self.plan_table_ref(table),
            tables => {
                // Multiple tables without explicit joins - create cross joins
                let mut plan = self.plan_table_ref(&tables[0])?;
                for table in &tables[1..] {
                    let right_plan = self.plan_table_ref(table)?;

                    // Extract left alias from the current plan
                    let left_alias = self.extract_alias_from_plan(&plan);

                    // Extract right alias from table ref
                    let right_alias = match table {
                        TableRef::Table { alias, .. } => alias.clone(),
                        TableRef::Subquery { alias, .. } => alias.clone(),
                    };

                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: None,
                        join_type: JoinType::Inner,
                        left_alias,
                        right_alias,
                    };
                }
                Ok(plan)
            }
        }
    }

    /// Create plan for a table reference (either a table or subquery)
    fn plan_table_ref(&self, table_ref: &TableRef) -> Result<PlanNode> {
        match table_ref {
            TableRef::Table { name, alias } => {
                self.plan_table_scan(name, alias.as_ref())
            }
            TableRef::Subquery { subquery, alias } => {
                // Create a subquery plan node
                // Note: This is a derived table subquery, not correlated, so no correlated columns
                Ok(PlanNode::Subquery {
                    query: subquery.clone(),
                    correlated_columns: vec![],
                })
            }
        }
    }

    /// Extract table alias from a plan node
    fn extract_alias_from_plan(&self, plan: &PlanNode) -> Option<String> {
        match plan {
            PlanNode::Scan { alias, .. } => alias.clone(),
            PlanNode::CTEScan { alias, cte_name, .. } => alias.clone().or_else(|| Some(cte_name.clone())),
            PlanNode::Join { left_alias, .. } => left_alias.clone(),
            PlanNode::Filter { input, .. } => self.extract_alias_from_plan(input),
            PlanNode::Aggregate { input, .. } => self.extract_alias_from_plan(input),
            PlanNode::Sort { input, .. } => self.extract_alias_from_plan(input),
            PlanNode::Limit { input, .. } => self.extract_alias_from_plan(input),
            PlanNode::Project { input, .. } => self.extract_alias_from_plan(input),
            // For other plan nodes, we might need more complex handling
            _ => None,
        }
    }

    /// Create table scan plan (or expand view if it's a view)
    fn plan_table_scan(&self, table_name: &str, alias: Option<&String>) -> Result<PlanNode> {
        // First check if this table name refers to a materialized CTE
        if let Some(cte_result) = self.materialized_ctes.get(table_name) {
            // This is a CTE reference - create a CTEScan plan node
            println!("DEBUG: Resolving '{}' as CTE with {} rows", table_name, cte_result.rows.len());
            return Ok(PlanNode::CTEScan {
                cte_name: table_name.to_string(),
                alias: alias.map(|a| a.clone()),
            });
        }

        // Check if this is a view that needs to be expanded
        if let Some(catalog) = &self.catalog {
            if let Ok(Some(view_def)) = catalog.get_view(table_name) {
                // This is a view - tokenize, parse and plan its query
                let mut lexer = crate::sql::lexer::Lexer::new(&view_def.query);
                let tokens = lexer.tokenize()?;
                let mut parser = crate::sql::parser::Parser::new(tokens);
                let parsed_statements = parser.parse()?;

                if parsed_statements.is_empty() {
                    return Err(crate::error::RustgreSQLError::Parse(
                        format!("View '{}' has empty query", table_name)
                    ));
                }

                // The view query should be a SELECT statement
                match &parsed_statements[0] {
                    crate::sql::ast::Statement::Select(select_stmt) => {
                        // Plan the view's query
                        let view_plan = self.plan_select(select_stmt)?;

                        // If the view has an alias, we might need to wrap it in a projection
                        // to rename the output columns. For now, just return the plan.
                        // The alias handling can be improved in the future.
                        return Ok(view_plan.root);
                    }
                    _ => {
                        return Err(crate::error::RustgreSQLError::Internal(
                            format!("View '{}' query is not a SELECT statement", table_name)
                        ));
                    }
                }
            }
        }

        // Not a view, or catalog not available - create a regular table scan
        Ok(PlanNode::Scan {
            table_name: table_name.to_string(),
            columns: vec![], // Empty means all columns
            alias: alias.cloned(),
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
        match expr {
            Expression::Function { name, .. } => {
                matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
            }
            Expression::Subquery(subquery_stmt) => {
                // Check if the subquery contains aggregate functions
                let contains_agg = self.subquery_contains_aggregate(subquery_stmt);
                if std::env::var("DEBUG_SUBQUERY").is_ok() {
                    println!("DEBUG: Subquery contains aggregate: {}", contains_agg);
                }
                contains_agg
            }
            _ => false,
        }
    }

    /// Check if a subquery statement contains aggregate functions
    fn subquery_contains_aggregate(&self, subquery_stmt: &crate::sql::ast::Statement) -> bool {
        if let crate::sql::ast::Statement::Select(select_stmt) = subquery_stmt {
            match select_stmt {
                crate::sql::ast::SelectStatement::Simple { columns, having, group_by, .. } => {
                    // Check if any column is an aggregate function
                    for col in columns {
                        if self.expression_contains_aggregate(&col.expr) {
                            return true;
                        }
                    }

                    // Check HAVING clause for aggregates
                    if let Some(having_expr) = having {
                        if self.expression_contains_aggregate(having_expr) {
                            return true;
                        }
                    }

                    // If there are aggregates, it's a scalar aggregate subquery (regardless of GROUP BY)
                    self.expression_contains_aggregate_in_columns(columns)
                }
                _ => false,
            }
        } else {
            false
        }
    }

    /// Check if expression contains aggregate functions (recursive)
    fn expression_contains_aggregate(&self, expr: &Expression) -> bool {
        match expr {
            Expression::Function { name, .. } => {
                matches!(name.to_uppercase().as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
            }
            Expression::BinaryOp { left, right, .. } => {
                self.expression_contains_aggregate(left) || self.expression_contains_aggregate(right)
            }
            Expression::UnaryOp { expr, .. } => {
                self.expression_contains_aggregate(expr)
            }
            Expression::Subquery(_) => {
                // Nested subqueries - handle recursively
                self.is_aggregate_function(expr)
            }
            _ => false,
        }
    }

    /// Check if any column in the list contains aggregate functions
    fn expression_contains_aggregate_in_columns(&self, columns: &[crate::sql::ast::ColumnSpec]) -> bool {
        columns.iter().any(|col| self.expression_contains_aggregate(&col.expr))
    }

    /// Plan aggregation (GROUP BY and aggregate functions)
    fn plan_aggregation(&self, input: PlanNode, select: &SelectStatement) -> Result<PlanNode> {
        match select {
            SelectStatement::Simple { group_by, columns, having, .. } => {
                // Extract aggregate functions from SELECT list
                let mut aggregate_functions = Vec::new();
                let mut group_by_columns = group_by.clone();

                // Add GROUP BY columns from SELECT list that aren't aggregate functions
                for col_spec in columns.iter() {
                    let expr = &col_spec.expr;
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
                for (i, col_spec) in columns.iter().enumerate() {
                    let expr = &col_spec.expr;
                    if self.is_aggregate_function(expr) {
                        let alias = if let Some(alias) = &col_spec.alias {
                            alias.clone()
                        } else {
                            match expr {
                                Expression::Function { name, .. } => {
                                    format!("{}_{}", name.to_lowercase(), i)
                                }
                                Expression::Subquery(_) => {
                                    format!("subquery_agg_{}", i)
                                }
                                _ => format!("aggregate{}", i),
                            }
                        };
                        aggregate_functions.push((alias, expr.clone()));
                    } else if matches!(expr, Expression::Star) && !group_by.is_empty() {
                        // SELECT * with GROUP BY - this is complex, for now treat as COUNT(*)
                        let count_star = Expression::Function {
                            name: "COUNT".to_string(),
                            args: vec![Expression::Star],
                            distinct: false,
                        };
                        let alias = if let Some(alias) = &col_spec.alias {
                            alias.clone()
                        } else {
                            format!("count_star_{}", i)
                        };
                        aggregate_functions.push((alias, count_star));
                    }
                }

                // Handle COUNT(*) case
                if columns.len() == 1 && matches!(&columns[0].expr, Expression::Star) && group_by.is_empty() {
                    let count_star = Expression::Function {
                        name: "COUNT".to_string(),
                        args: vec![Expression::Star],
                        distinct: false,
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
            (Expression::Function { name: name_a, args: args_a, .. }, Expression::Function { name: name_b, args: args_b, .. }) => {
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
            Expression::Function { name, args, distinct } => {
                let planned_args: Result<Vec<Expression>> = args
                    .iter()
                    .map(|arg| self.plan_subqueries_in_expression(arg))
                    .collect();
                Ok(Expression::Function {
                    name: name.clone(),
                    args: planned_args?,
                    distinct: *distinct,
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

    /// Helper function to strip WITH clause from a SelectStatement to prevent infinite recursion
    fn strip_with_clause(select: &SelectStatement) -> SelectStatement {
        match select {
            SelectStatement::Simple {
                with_clause: _,
                distinct,
                columns,
                from,
                joins,
                where_clause,
                group_by,
                having,
                order_by,
                limit,
                offset,
                named_windows,
            } => SelectStatement::Simple {
                with_clause: None, // Strip the WITH clause
                distinct: *distinct,
                columns: columns.clone(),
                from: from.clone(),
                joins: joins.clone(),
                where_clause: where_clause.clone(),
                group_by: group_by.clone(),
                having: having.clone(),
                order_by: order_by.clone(),
                limit: *limit,
                offset: *offset,
                named_windows: named_windows.clone(),
            },
            SelectStatement::SetOperation(set_op) => {
                // Recursively strip WITH clause from left and right queries
                SelectStatement::SetOperation(crate::sql::ast::SetOperation {
                    operator: set_op.operator.clone(),
                    left: Box::new(Self::strip_with_clause(&set_op.left)),
                    right: Box::new(Self::strip_with_clause(&set_op.right)),
                    all: set_op.all,
                })
            }
        }
    }

    /// Plan a CTE SELECT statement
    fn plan_cte_select(&self, with_clause: &WithClause, select: &SelectStatement) -> Result<ExecutionPlan> {
        // Strip the WITH clause from the main query to prevent infinite recursion
        // The WITH clause is already being handled at this level, so the main query
        // should not contain it when it's planned later
        let stripped_select = Self::strip_with_clause(select);

        // Create a CTE plan node that will handle the entire WITH clause
        let plan_node = PlanNode::CTE {
            with_clause: with_clause.clone(),
            main_query: Box::new(crate::sql::ast::Statement::Select(stripped_select)),
        };

        // For now, we'll use the column names from the main query
        // In a full implementation, we'd need to derive the output schema from the main query
        let output_schema = Vec::new();

        Ok(ExecutionPlan {
            root: plan_node,
            output_schema,
        })
    }

    /// Check if any expressions contain window functions
    fn has_window_functions(&self, expressions: &[Expression]) -> bool {
        expressions.iter().any(|expr| self.is_window_function(expr))
    }

    /// Check if an expression is a window function
    fn is_window_function(&self, expr: &Expression) -> bool {
        matches!(expr, Expression::WindowFunction(_))
    }

    /// Extract window functions and non-window expressions from projections
    fn separate_window_functions(&self, projections: &[(String, Expression)]) -> (Vec<(String, Expression)>, Vec<(String, Expression)>) {
        let mut window_functions = Vec::new();
        let mut regular_projections = Vec::new();

        for (name, expr) in projections {
            if self.is_window_function(expr) {
                window_functions.push((name.clone(), expr.clone()));
            } else {
                regular_projections.push((name.clone(), expr.clone()));
            }
        }

        (window_functions, regular_projections)
    }
}
