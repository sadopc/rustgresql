//! Query optimizer
//!
//! Provides cost-based query optimization with index selection and plan caching.

use crate::{Result, sql::ast::{*, BinaryOperator}, executor::planner::{ExecutionPlan, PlanNode, QueryPlanner}, catalog::{IndexDef, IndexType}, optimizer::{cost_model::{CostModel, CostEstimate}, statistics::StatisticsManager, index_selection::{IndexSelector, IndexAccessPath}, rules::{RuleEngine, PredicatePushdownRule, ProjectionPushdownRule, ConstantFoldingRule, AggregationPushdownRule, ParallelPlanSelectionRule, ParallelJoinOrderingRule, ParallelAggregationOptimizationRule}}};

/// Optimized query planner that considers index usage, costs, and optimization rules
#[derive(Debug)]
pub struct OptimizedQueryPlanner {
    base_planner: QueryPlanner,
    cost_model: CostModel,
    stats_manager: StatisticsManager,
    index_selector: IndexSelector,
    rule_engine: RuleEngine,
}

impl OptimizedQueryPlanner {
    /// Create new optimized query planner
    pub fn new() -> Self {
        let cost_model = CostModel::new();
        let stats_manager = StatisticsManager::new();
        let index_selector = IndexSelector::new(cost_model.clone(), stats_manager.clone());
        let mut rule_engine = RuleEngine::new();

        // Add optimization rules in order of priority
        rule_engine.add_rule(Box::new(ConstantFoldingRule));                     // First: simplify expressions
        rule_engine.add_rule(Box::new(ParallelPlanSelectionRule::new(cost_model.clone()))); // Second: consider parallel execution
        rule_engine.add_rule(Box::new(PredicatePushdownRule));                   // Third: push filters down
        rule_engine.add_rule(Box::new(ParallelJoinOrderingRule::new(cost_model.clone())));   // Fourth: optimize join order for parallelism
        rule_engine.add_rule(Box::new(ProjectionPushdownRule));                  // Fifth: push projections down
        rule_engine.add_rule(Box::new(AggregationPushdownRule));                 // Sixth: optimize aggregations
        rule_engine.add_rule(Box::new(ParallelAggregationOptimizationRule::new(cost_model.clone()))); // Seventh: parallel-specific aggregation optimization

        Self {
            base_planner: QueryPlanner::new(),
            cost_model,
            stats_manager,
            index_selector,
            rule_engine,
        }
    }

    /// Create optimized query planner with custom components
    pub fn with_components(
        cost_model: CostModel,
        stats_manager: StatisticsManager,
        index_selector: IndexSelector,
    ) -> Self {
        Self {
            base_planner: QueryPlanner::new(),
            cost_model,
            stats_manager,
            index_selector,
            rule_engine: crate::optimizer::rules::RuleEngine::new(),
        }
    }

    /// Create optimized execution plan for SELECT statement
    pub fn plan_select(&self, select: &SelectStatement, table_indexes: &[(String, Vec<IndexDef>)]) -> Result<ExecutionPlan> {
        match select {
            SelectStatement::Simple {
                with_clause: _,
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
                // Extract required columns for the query
                let required_columns = self.extract_required_columns(select);

                // Start with table scans (with optimization)
                let mut plan = self.plan_optimized_from_clause(from, &required_columns, table_indexes)?;

                // Apply joins
                for join in joins {
                    let right_plan = self.plan_optimized_table_scan(&join.table.name, &required_columns, table_indexes)?;
                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: join.condition.clone(),
                        join_type: join.join_type.clone(),
                        left_alias: None,
                        right_alias: join.table.alias.clone(),
                    };
                }

                // Apply WHERE clause with index-aware optimization
                if let Some(ref where_clause) = where_clause {
                    plan = self.optimize_filter_clause(plan, where_clause, &required_columns, table_indexes)?;
                }

                // Apply GROUP BY and aggregation if present
                if !group_by.is_empty() || self.has_aggregate_functions(&columns.iter().map(|c| c.expr.clone()).collect::<Vec<_>>()) {
                    plan = self.plan_aggregation_optimized(plan, select)?;
                }

                // Apply column projection
                if columns.len() == 1 && matches!(columns[0].expr, Expression::Star) {
                    // SELECT * - no explicit projection needed
                } else {
                        // Check if we have aggregation
                        let has_aggregation = !group_by.is_empty() || self.has_aggregate_functions(&columns.iter().map(|c| c.expr.clone()).collect::<Vec<_>>());

                        let projections: Vec<(String, Expression)> = columns
                            .iter()
                            .enumerate()
                            .map(|(i, col_spec)| {
                                let expr = &col_spec.expr;

                                let final_expr = if has_aggregation {
                                    if self.is_aggregate_function(expr) {
                                         let alias = if let Some(alias) = &col_spec.alias {
                                            alias.clone()
                                        } else {
                                            match expr {
                                                Expression::Function { name, .. } => {
                                                    format!("{}_{}", name.to_lowercase(), i)
                                                }
                                                _ => format!("aggregate{}", i),
                                            }
                                        };
                                        Expression::Column { table: None, name: alias }
                                    } else if matches!(expr, Expression::Star) && !group_by.is_empty() {
                                         let alias = if let Some(alias) = &col_spec.alias {
                                            alias.clone()
                                        } else {
                                            format!("count_star_{}", i)
                                        };
                                        Expression::Column { table: None, name: alias }
                                    } else {
                                        expr.clone()
                                    }
                                } else {
                                    expr.clone()
                                };

                                let column_name = if let Some(alias) = &col_spec.alias {
                                    alias.clone()
                                } else {
                                    match expr {
                                        Expression::Column { name, .. } => name.clone(),
                                        Expression::Star => "*".to_string(),
                                        _ => format!("col{}", i),
                                    }
                                };
                                (column_name, final_expr)
                            })
                            .collect();

                        plan = PlanNode::Project {
                            input: Box::new(plan),
                            columns: projections,
                            table_aliases: std::collections::HashMap::new(),
                            left_columns: None,
                            right_columns: None,
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

                // Apply optimization rules to the final plan
                // plan = self.rule_engine.optimize(&plan)?; // Disabled for now

                // Create output schema (simplified)
                let output_schema = self.create_output_schema(&plan, columns);

                Ok(ExecutionPlan {
                    root: plan,
                    output_schema,
                })
            }
            SelectStatement::SetOperation(_) => {
                // For now, set operations are not optimized
                // Fall back to the non-optimized planner
                Err(crate::error::RustgreSQLError::Internal(
                    "Optimized planning for set operations is not yet supported".to_string()
                ))
            }
        }
    }

    /// Create optimized plan for FROM clause with index consideration
    fn plan_optimized_from_clause(
        &self,
        from: &[TableRef],
        required_columns: &[String],
        table_indexes: &[(String, Vec<IndexDef>)],
    ) -> Result<PlanNode> {
        match from {
            [] => Err(crate::error::RustgreSQLError::Parse("No tables in FROM clause".to_string())),
            [table] => self.plan_optimized_table_scan(&table.name, required_columns, table_indexes),
            tables => {
                // Multiple tables without explicit joins - create cross joins
                let mut plan = self.plan_optimized_table_scan(&tables[0].name, required_columns, table_indexes)?;
                for table in &tables[1..] {
                    let right_plan = self.plan_optimized_table_scan(&table.name, required_columns, table_indexes)?;
                    plan = PlanNode::Join {
                        left: Box::new(plan),
                        right: Box::new(right_plan),
                        condition: None,
                        join_type: JoinType::Inner,
                        left_alias: None,
                        right_alias: None,
                    };
                }
                Ok(plan)
            }
        }
    }

    /// Create optimized table scan with index selection
    fn plan_optimized_table_scan(
        &self,
        table_name: &str,
        required_columns: &[String],
        table_indexes: &[(String, Vec<IndexDef>)],
    ) -> Result<PlanNode> {
        // Get indexes for this table
        let indexes = table_indexes
            .iter()
            .find(|(name, _)| name == table_name)
            .map(|(_, indexes)| indexes.as_slice())
            .unwrap_or(&[]);

        // For now, create a regular scan (optimization will be applied in filter clauses)
        Ok(PlanNode::Scan {
            table_name: table_name.to_string(),
            columns: required_columns.to_vec(),
            alias: None,
        })
    }

    /// Optimize filter clause with index selection
    fn optimize_filter_clause(
        &self,
        input_plan: PlanNode,
        where_clause: &Expression,
        required_columns: &[String],
        table_indexes: &[(String, Vec<IndexDef>)],
    ) -> Result<PlanNode> {
        // Extract conditions and table names from the WHERE clause
        let (table_conditions, remaining_condition) = self.extract_table_conditions(where_clause);

        // If the input is a simple table scan, we can optimize with indexes
        if let PlanNode::Scan { table_name, columns, .. } = &input_plan {
            if let Some(conditions) = table_conditions.get(table_name) {
                if let Some(indexes) = table_indexes.iter().find(|(name, _)| name == table_name) {
                    // Try to select best index for these conditions
                    if let Some(best_index_path) = self.index_selector.select_best_index(
                        table_name,
                        &indexes.1,
                        conditions,
                        required_columns,
                    ) {
                        return Ok(self.create_index_scan_node(
                            table_name,
                            &best_index_path,
                            required_columns,
                            remaining_condition,
                        ));
                    }
                }
            }
        }

        // Fall back to regular filter
        Ok(PlanNode::Filter {
            input: Box::new(input_plan),
            condition: where_clause.clone(),
        })
    }

    /// Extract table-specific conditions from WHERE clause
    pub fn extract_table_conditions(&self, where_clause: &Expression) -> (std::collections::HashMap<String, Vec<Expression>>, Option<Expression>) {
        let mut table_conditions = std::collections::HashMap::new();
        let mut remaining_conditions = Vec::new();

        // Simple extraction - in a real implementation, this would be more sophisticated
        self.extract_conditions_recursive(where_clause, &mut table_conditions, &mut remaining_conditions);

        // Combine remaining conditions with AND
        let remaining_condition = if remaining_conditions.is_empty() {
            None
        } else if remaining_conditions.len() == 1 {
            Some(remaining_conditions.into_iter().next().unwrap())
        } else {
            let mut combined = remaining_conditions[0].clone();
            for condition in remaining_conditions.into_iter().skip(1) {
                combined = Expression::BinaryOp {
                    left: Box::new(combined),
                    op: BinaryOperator::And,
                    right: Box::new(condition),
                };
            }
            Some(combined)
        };

        (table_conditions, remaining_condition)
    }

    /// Recursively extract conditions from expression tree
    fn extract_conditions_recursive(
        &self,
        expr: &Expression,
        table_conditions: &mut std::collections::HashMap<String, Vec<Expression>>,
        remaining_conditions: &mut Vec<Expression>,
    ) {
        match expr {
            Expression::BinaryOp { left, op: BinaryOperator::And, right } => {
                // Recursively process both sides of AND
                self.extract_conditions_recursive(left, table_conditions, remaining_conditions);
                self.extract_conditions_recursive(right, table_conditions, remaining_conditions);
            }
            Expression::BinaryOp { left, op, right } if op.is_comparison_operator() => {
                // Check if this is a simple column comparison with constant
                if let (Some(table_name), _is_constant) = self.is_simple_column_condition(expr) {
                    table_conditions
                        .entry(table_name)
                        .or_insert_with(Vec::new)
                        .push(expr.clone());
                } else {
                    remaining_conditions.push(expr.clone());
                }
            }
            Expression::BinaryOp { left, op: BinaryOperator::Like, right } => {
                if let Expression::Column { table, .. } = &**left {
                    if let Some(table_name) = table {
                        table_conditions
                            .entry(table_name.clone())
                            .or_insert_with(Vec::new)
                            .push(expr.clone());
                    } else {
                        remaining_conditions.push(expr.clone());
                    }
                } else {
                    remaining_conditions.push(expr.clone());
                }
            }
            Expression::BinaryOp { left, op: BinaryOperator::In, right } => {
                if let Expression::Column { table, .. } = &**left {
                    if let Some(table_name) = table {
                        table_conditions
                            .entry(table_name.clone())
                            .or_insert_with(Vec::new)
                            .push(expr.clone());
                    } else {
                        remaining_conditions.push(expr.clone());
                    }
                } else {
                    remaining_conditions.push(expr.clone());
                }
            }
            _ => {
                remaining_conditions.push(expr.clone());
            }
        }
    }

    /// Check if expression is a simple column condition and return table name
    fn is_simple_column_condition(&self, expr: &Expression) -> (Option<String>, bool) {
        match expr {
            Expression::BinaryOp { left, op, right } if op.is_comparison_operator() => {
                match (left.as_ref(), right.as_ref()) {
                    (Expression::Column { table, .. }, Expression::Value(_)) => {
                        (table.clone(), true)
                    }
                    (Expression::Value(_), Expression::Column { table, .. }) => {
                        (table.clone(), true)
                    }
                    (Expression::Column { table, .. }, Expression::Parameter(_)) => {
                        (table.clone(), true)
                    }
                    (Expression::Parameter(_), Expression::Column { table, .. }) => {
                        (table.clone(), true)
                    }
                    _ => (None, false),
                }
            }
            _ => (None, false),
        }
    }

    /// Create index scan plan node based on index access path
    fn create_index_scan_node(
        &self,
        table_name: &str,
        index_path: &IndexAccessPath,
        required_columns: &[String],
        residual_condition: Option<Expression>,
    ) -> PlanNode {
        // Create the appropriate index scan node
        let base_scan = match index_path.access_type {
            crate::optimizer::index_selection::IndexAccessType::IndexOnlyScan => {
                PlanNode::IndexOnlyScan {
                    table_name: table_name.to_string(),
                    index_name: index_path.index_name.clone(),
                    index_condition: index_path.index_condition.clone(),
                    columns: required_columns.to_vec(),
                }
            }
            _ => {
                PlanNode::IndexScan {
                    table_name: table_name.to_string(),
                    index_name: index_path.index_name.clone(),
                    index_condition: index_path.index_condition.clone(),
                    columns: required_columns.to_vec(),
                }
            }
        };

        // Apply residual conditions if any
        if let Some(residual) = residual_condition {
            PlanNode::Filter {
                input: Box::new(base_scan),
                condition: residual,
            }
        } else {
            base_scan
        }
    }

    /// Extract required columns from SELECT statement
    pub fn extract_required_columns(&self, select: &SelectStatement) -> Vec<String> {
        match select {
            SelectStatement::Simple { columns, .. } => {
                if columns.len() == 1 && matches!(columns[0].expr, Expression::Star) {
                    // SELECT * - we can't determine exact columns without schema info
                    Vec::new() // Empty means all columns
                } else {
                    // Extract column names from expressions
                    let mut required_columns = Vec::new();
                    for col_spec in columns {
                        self.extract_columns_from_expression(&col_spec.expr, &mut required_columns);
                    }
                    required_columns.sort();
                    required_columns.dedup();
                    required_columns
                }
            }
            SelectStatement::SetOperation(_) => {
                // For set operations, we can't easily determine required columns
                // This would require schema analysis of both sides
                Vec::new()
            }
        }
    }

    /// Extract column names from expression
    fn extract_columns_from_expression(&self, expr: &Expression, columns: &mut Vec<String>) {
        match expr {
            Expression::Column { name, .. } => {
                columns.push(name.clone());
            }
            Expression::BinaryOp { left, right, .. } => {
                self.extract_columns_from_expression(left, columns);
                self.extract_columns_from_expression(right, columns);
            }
            Expression::UnaryOp { expr, .. } => {
                self.extract_columns_from_expression(expr, columns);
            }
            Expression::Function { args, .. } => {
                for arg in args {
                    self.extract_columns_from_expression(arg, columns);
                }
            }
            Expression::List(expressions) => {
                for expr in expressions {
                    self.extract_columns_from_expression(expr, columns);
                }
            }
            _ => {
                // Other expression types don't contain columns
            }
        }
    }

    /// Create output schema for execution plan
    fn create_output_schema(&self, plan: &PlanNode, select_columns: &[ColumnSpec]) -> Vec<(String, crate::types::DataType)> {
        // Simplified schema creation - in a real implementation, this would
        // query the catalog for actual column types
        if select_columns.len() == 1 && matches!(select_columns[0].expr, Expression::Star) {
            match plan {
                PlanNode::Scan { table_name, .. } => {
                    vec![("column1".to_string(), crate::types::DataType::new(crate::types::DataTypeKind::Text))]
                }
                PlanNode::IndexScan { table_name, .. } => {
                    vec![("column1".to_string(), crate::types::DataType::new(crate::types::DataTypeKind::Text))]
                }
                PlanNode::IndexOnlyScan { table_name, .. } => {
                    vec![("column1".to_string(), crate::types::DataType::new(crate::types::DataTypeKind::Text))]
                }
                _ => vec![],
            }
        } else {
            select_columns
                .iter()
                .enumerate()
                .map(|(i, col_spec)| {
                    let column_name = if let Some(alias) = &col_spec.alias {
                        alias.clone()
                    } else {
                        match &col_spec.expr {
                            Expression::Column { name, .. } => name.clone(),
                            _ => format!("col{}", i),
                        }
                    };
                    (column_name, crate::types::DataType::new(crate::types::DataTypeKind::Text))
                })
                .collect()
        }
    }

    /// Create execution plan for INSERT statement
    pub fn plan_insert(&self, insert: &InsertStatement) -> Result<ExecutionPlan> {
        self.base_planner.plan_insert(insert)
    }

    /// Create execution plan for UPDATE statement
    pub fn plan_update(&self, update: &UpdateStatement) -> Result<ExecutionPlan> {
        self.base_planner.plan_update(update)
    }

    /// Create execution plan for DELETE statement
    pub fn plan_delete(&self, delete: &DeleteStatement) -> Result<ExecutionPlan> {
        self.base_planner.plan_delete(delete)
    }

    /// Get cost model reference
    pub fn cost_model(&self) -> &CostModel {
        &self.cost_model
    }

    /// Get statistics manager reference
    pub fn stats_manager(&self) -> &StatisticsManager {
        &self.stats_manager
    }

    /// Get index selector reference
    pub fn index_selector(&self) -> &IndexSelector {
        &self.index_selector
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

    /// Plan aggregation with optimization support
    fn plan_aggregation_optimized(&self, input: PlanNode, select: &SelectStatement) -> Result<PlanNode> {
        match select {
            SelectStatement::Simple { group_by, columns, having, .. } => {
                // Extract aggregate functions with their aliases
                let mut aggregate_functions = Vec::new();
                let mut group_by_columns = group_by.clone();

                // Add GROUP BY columns from SELECT list that aren't aggregate functions
                for col_spec in columns {
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
                                _ => format!("aggregate{}", i),
                            }
                        };
                        aggregate_functions.push((alias, expr.clone()));
                    } else if matches!(expr, Expression::Star) && !group_by.is_empty() {
                        // SELECT * with GROUP BY - this is complex, for now treat as COUNT(*)
                        let count_star = Expression::Function {
                            name: "COUNT".to_string(),
                            args: vec![Expression::Star],
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
                if columns.len() == 1 && matches!(&columns[0].expr, Expression::Star) {
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
}

// Extension trait for binary operators
trait BinaryOperatorExt {
    fn is_comparison_operator(&self) -> bool;
}

impl BinaryOperatorExt for BinaryOperator {
    fn is_comparison_operator(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Equals
                | BinaryOperator::NotEquals
                | BinaryOperator::LessThan
                | BinaryOperator::LessThanOrEquals
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanOrEquals
                | BinaryOperator::Like
                | BinaryOperator::In
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::*;

    #[test]
    fn test_optimized_planner_creation() {
        let planner = OptimizedQueryPlanner::new();
        assert!(planner.cost_model().config().seq_page_cost > 0.0);
    }

    #[test]
    fn test_required_columns_extraction() {
        let planner = OptimizedQueryPlanner::new();

        let select = SelectStatement::Simple {
            with_clause: None,
            distinct: false,
            columns: vec![
                Expression::Column { name: "id".to_string(), table: Some("users".to_string()) },
                Expression::Column { name: "name".to_string(), table: Some("users".to_string()) },
            ],
            from: vec![TableRef {
                name: "users".to_string(),
                alias: None,
            }],
            joins: Vec::new(),
            where_clause: None,
            group_by: Vec::new(),
            having: None,
            order_by: Vec::new(),
            limit: None,
            offset: None,
            named_windows: Vec::new(),
        };

        let required_columns = planner.extract_required_columns(&select);
        assert_eq!(required_columns, vec!["id", "name"]);
    }
}