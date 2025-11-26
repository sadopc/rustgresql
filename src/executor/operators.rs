//! Execution operators
//!
//! Physical operators for executing query plans

use crate::{Result, sql::ast::{Expression, BinaryOperator, SetOperator as SetOperatorType, OrderBy, SortDirection, NullsPosition}, executor::planner::PlanNode, types::{Value, ValueKind, DataTypeKind, DataType}};
use crate::executor::{TableScanner, ExpressionEvaluator, EvaluationContext, ThreeValuedLogic, RowData, AggregateState};
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use serde_json;

/// Compare two Values for sorting
fn compare_values(a: &Value, b: &Value, nulls_position: NullsPosition) -> std::cmp::Ordering {
    match (&a.kind, &b.kind) {
        (ValueKind::Null(_), ValueKind::Null(_)) => std::cmp::Ordering::Equal,
        (ValueKind::Null(_), _) => {
            // Determine NULL position based on specification
            match nulls_position {
                NullsPosition::First => std::cmp::Ordering::Less,
                NullsPosition::Last => std::cmp::Ordering::Greater,
                NullsPosition::Default => std::cmp::Ordering::Less, // Default: NULLs first
            }
        }
        (_, ValueKind::Null(_)) => {
            match nulls_position {
                NullsPosition::First => std::cmp::Ordering::Greater,
                NullsPosition::Last => std::cmp::Ordering::Less,
                NullsPosition::Default => std::cmp::Ordering::Greater, // Default: NULLs first
            }
        }
        (ValueKind::Integer(a_val), ValueKind::Integer(b_val)) => a_val.cmp(b_val),
        (ValueKind::Float(a_val), ValueKind::Float(b_val)) => a_val.partial_cmp(b_val).unwrap_or(std::cmp::Ordering::Equal),
        (ValueKind::String(a_val), ValueKind::String(b_val)) => a_val.cmp(b_val),
        (ValueKind::Boolean(a_val), ValueKind::Boolean(b_val)) => a_val.cmp(b_val),
        (ValueKind::Timestamp(a_val), ValueKind::Timestamp(b_val)) => a_val.cmp(b_val),
        // Different types - establish a priority order
        (ValueKind::Boolean(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::Boolean(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Integer(_), ValueKind::Float(_)) => std::cmp::Ordering::Less,
        (ValueKind::Float(_), ValueKind::Integer(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Integer(_), ValueKind::String(_)) => std::cmp::Ordering::Less,
        (ValueKind::String(_), ValueKind::Integer(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Float(_), ValueKind::String(_)) => std::cmp::Ordering::Less,
        (ValueKind::String(_), ValueKind::Float(_)) => std::cmp::Ordering::Greater,
        (ValueKind::List(_), ValueKind::List(_)) => std::cmp::Ordering::Equal,
        (ValueKind::List(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::List(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Timestamp(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::Timestamp(_)) => std::cmp::Ordering::Greater,
    }
}

/// Query result row
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub rows: Vec<Vec<Value>>,
    pub column_names: Vec<String>,
}

/// Scan operator
#[derive(Debug)]
pub struct ScanOperator {
    pub table_name: String,
    pub scanner: Option<TableScanner>,
}

impl ScanOperator {
    pub fn new(table_name: String) -> Self {
        Self {
            table_name,
            scanner: None,
        }
    }

    pub fn with_scanner(table_name: String, scanner: TableScanner) -> Self {
        Self {
            table_name,
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Scanning table: {}", self.table_name));

        // Get scanner from context
        let scanner = if let (Some(ref catalog), Some(ref buffer_manager)) = (&context.catalog, &context.buffer_manager) {
            match TableScanner::new(catalog.clone(), buffer_manager.clone(), &self.table_name) {
                Ok(s) => s,
                Err(e) => {
                    context.log(&format!("Failed to create scanner for table {}: {}", self.table_name, e));
                    return Err(e);
                }
            }
        } else {
            return Err(crate::error::RustgreSQLError::Execution(format!("No scanner available for table {}", self.table_name)));
        };

        let mut row_iterator = scanner.scan_all()?;
        let mut rows = Vec::new();
        let mut column_names = Vec::new();

        // Get column names from the first iteration or from table definition
        if let Some(first_row) = row_iterator.next_row()? {
            column_names = row_iterator.get_column_names();

            // Convert RowData to Vec<Value> format and include the first row
            rows.push(first_row.values);

            // Process remaining rows
            while let Some(row_data) = row_iterator.next_row()? {
                rows.push(row_data.values);
            }
        } else {
            // No rows, but we still need column names from table definition
            column_names = row_iterator.get_column_names();
        }

        context.log(&format!("Scanned {} rows from table {}", rows.len(), self.table_name));

        Ok(QueryResult {
            rows,
            column_names,
        })
    }
}

/// Filter operator
#[derive(Debug)]
pub struct FilterOperator {
    pub input: Box<PlanNode>,
    pub condition: Expression,
    pub scanner: Option<TableScanner>, // For column name resolution
}

impl FilterOperator {
    pub fn new(input: PlanNode, condition: Expression) -> Self {
        Self {
            input: Box::new(input),
            condition,
            scanner: None,
        }
    }

    pub fn with_scanner(input: PlanNode, condition: Expression, scanner: TableScanner) -> Self {
        Self {
            input: Box::new(input),
            condition,
            scanner: None,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let input_result = self.input.execute(context)?;

        // Filter rows based on condition using the real expression evaluator
        let total_rows = input_result.rows.len();
        let column_names = input_result.column_names.clone();

        // Try to create a scanner for proper column resolution
        let scanner = self.create_scanner_from_input_helper(context);

        let filtered_rows: Vec<Vec<Value>> = if let Some(ref scanner) = scanner {
            // Use scanner for proper column name resolution
            input_result.rows
                .into_iter()
                .filter(|row| {
                    let eval_context = self.create_evaluation_context(scanner, &column_names, row, context);
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(&self.condition, &eval_context) {
                        Ok(result) => {
                            match ThreeValuedLogic::from_value(&result) {
                                ThreeValuedLogic::True => true,
                                ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                            }
                        }
                        Err(e) => {
                            context.log(&format!("Error evaluating filter condition: {}", e));
                            false // Treat evaluation errors as false (exclude row)
                        }
                    }
                })
                .collect()
        } else {
            // Fallback: basic evaluation without proper column resolution
            input_result.rows
                .into_iter()
                .filter(|row| {
                    let eval_context = self.create_basic_evaluation_context(&column_names, row, context);
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(&self.condition, &eval_context) {
                        Ok(result) => {
                            match ThreeValuedLogic::from_value(&result) {
                                ThreeValuedLogic::True => true,
                                ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                            }
                        }
                        Err(e) => {
                            context.log(&format!("Error evaluating filter condition (fallback): {}", e));
                            false
                        }
                    }
                })
                .collect()
        };

        context.log(&format!("Filtered {} rows from {} total",
                          filtered_rows.len(), total_rows));

        Ok(QueryResult {
            rows: filtered_rows,
            column_names,
        })
    }

    /// Extract table alias from the input plan node
    fn extract_table_alias(&self) -> Option<String> {
        match &*self.input {
            PlanNode::Scan { alias, .. } => alias.clone(),
            PlanNode::Project { table_aliases, .. } => {
                // For Project nodes, check if there's only one table alias
                if table_aliases.len() == 1 {
                    table_aliases.values().next().cloned()
                } else {
                    None
                }
            }
            PlanNode::CTEScan { alias, cte_name } => alias.clone().or_else(|| Some(cte_name.clone())),
            _ => None,
        }
    }

    /// Create evaluation context with proper column name resolution
    fn create_evaluation_context(&self, scanner: &TableScanner, column_names: &[String], row: &[Value], context: &ExecutionContext) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Extract table alias information from the input plan
        let table_alias = self.extract_table_alias();

        // Map column names to values, including qualified names if table alias exists
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                let value = &row[i];

                // Store unqualified column name (existing behavior)
                columns.insert(column_name.clone(), value.clone());

                // Also store qualified column name if we have a table alias
                if let Some(ref alias) = table_alias {
                    let qualified_name = format!("{}.{}", alias, column_name);
                    columns.insert(qualified_name, value.clone());
                }
            }
        }

        // Include outer context values for correlated subqueries
        if let Some(outer_values) = context.get_outer_context_values() {
            for (column_name, value) in outer_values {
                columns.insert(column_name.clone(), value.clone());
            }
        }

        let mut eval_context = EvaluationContext::with_columns(columns);

        // Pass catalog, buffer_manager, and materialized_ctes for subquery execution
        if let Some(catalog) = context.get_catalog() {
            eval_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            eval_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            eval_context.set_materialized_ctes(materialized_ctes.clone());
        }

        eval_context
    }

    /// Create basic evaluation context (fallback when no scanner available)
    fn create_basic_evaluation_context(&self, column_names: &[String], row: &[Value], context: &ExecutionContext) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Extract table alias information from the input plan
        let table_alias = self.extract_table_alias();

        // Map column names to values, including qualified names if table alias exists
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                let value = &row[i];

                // Store unqualified column name (existing behavior)
                columns.insert(column_name.clone(), value.clone());

                // Also store qualified column name if we have a table alias
                if let Some(ref alias) = table_alias {
                    let qualified_name = format!("{}.{}", alias, column_name);
                    columns.insert(qualified_name, value.clone());
                }
            }
        }

        // Include outer context values for correlated subqueries
        if let Some(outer_values) = context.get_outer_context_values() {
            for (column_name, value) in outer_values {
                columns.insert(column_name.clone(), value.clone());
            }
        }

        let mut eval_context = EvaluationContext::with_columns(columns);

        // Pass catalog, buffer_manager, and materialized_ctes for subquery execution
        if let Some(catalog) = context.get_catalog() {
            eval_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            eval_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            eval_context.set_materialized_ctes(materialized_ctes.clone());
        }

        eval_context
    }
}

impl FilterOperator {
    /// Create a scanner from the input PlanNode if it's a table scan
    fn create_scanner_from_input_helper(&self, context: &ExecutionContext) -> Option<TableScanner> {
        // Recursively find the table name from the input plan
        let table_name = self.extract_table_name_from_plan_helper(&self.input);

        if let Some(table_name) = table_name {
            if let (Some(ref catalog), Some(ref buffer_manager)) = (&context.catalog, &context.buffer_manager) {
                match TableScanner::new(catalog.clone(), buffer_manager.clone(), &table_name) {
                    Ok(scanner) => Some(scanner),
                    Err(e) => {
                        eprintln!("Failed to create scanner for table {}: {}", table_name, e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Recursively extract table name from a plan node
    fn extract_table_name_from_plan_helper(&self, plan: &PlanNode) -> Option<String> {
        match plan {
            PlanNode::Scan { table_name, .. } => Some(table_name.clone()),
            PlanNode::IndexScan { table_name, .. } => Some(table_name.clone()),
            PlanNode::IndexOnlyScan { table_name, .. } => Some(table_name.clone()),
            PlanNode::Filter { input, .. } => self.extract_table_name_from_plan_helper(input),
            PlanNode::Project { input, .. } => self.extract_table_name_from_plan_helper(input),
            PlanNode::Join { left, .. } => self.extract_table_name_from_plan_helper(left),
            _ => None,
        }
    }
}

/// Project operator
#[derive(Debug)]
pub struct ProjectOperator {
    pub input: Box<PlanNode>,
    pub columns: Vec<(String, Expression)>,
    pub scanner: Option<TableScanner>, // For column name resolution
    pub left_columns: Option<Vec<String>>,
    pub right_columns: Option<Vec<String>>,
}

impl ProjectOperator {
    pub fn new(input: PlanNode, columns: Vec<(String, Expression)>) -> Self {
        Self {
            input: Box::new(input),
            columns,
            scanner: None,
            left_columns: None,
            right_columns: None,
        }
    }

    pub fn with_scanner(input: PlanNode, columns: Vec<(String, Expression)>, scanner: TableScanner) -> Self {
        Self {
            input: Box::new(input),
            columns,
            scanner: None,
            left_columns: None,
            right_columns: None,
        }
    }

    /// Extract table alias from the input plan node (for ProjectOperator)
    fn extract_table_alias(&self) -> Option<String> {
        match &*self.input {
            PlanNode::Scan { alias, .. } => alias.clone(),
            PlanNode::Project { table_aliases, .. } => {
                // For Project nodes, check if there's only one table alias
                if table_aliases.len() == 1 {
                    table_aliases.values().next().cloned()
                } else {
                    None
                }
            }
            PlanNode::CTEScan { alias, cte_name } => alias.clone().or_else(|| Some(cte_name.clone())),
            _ => None,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let input_result = self.input.execute(context)?;
        println!("DEBUG: ProjectOperator executing with {} input columns: {:?}", input_result.column_names.len(), input_result.column_names);
        println!("DEBUG: ProjectOperator has {} output columns:", self.columns.len());
        for (i, (name, expr)) in self.columns.iter().enumerate() {
            println!("DEBUG:   Output column {}: {} = {:?}", i, name, match expr {
                Expression::Column { name, .. } => format!("Column({})", name),
                Expression::BinaryOp { .. } => "BinaryOp(...)".to_string(),
                Expression::WindowFunction(_) => "WindowFunction".to_string(),
                _ => format!("{:?}", expr).chars().take(30).collect::<String>(),
            });
        }

        // Extract column names and compute projected values using real expression evaluator
        let column_names: Vec<String> = self.columns.iter().map(|(name, _)| name.clone()).collect();
        let input_column_names = input_result.column_names.clone();

        let projected_rows: Vec<Vec<Value>> = if let Some(ref scanner) = self.scanner {
            // Use scanner for proper column name resolution
            input_result.rows
                .into_iter()
                .map(|row| {
                    let eval_context = self.create_evaluation_context(scanner, &input_column_names, &row, context);
                    self.columns
                        .iter()
                        .map(|(_, expr)| {
                            match { let evaluator = ExpressionEvaluator; evaluator.evaluate(expr, &eval_context) } {
                                Ok(value) => value,
                                Err(_) => Value { kind: ValueKind::Null(crate::types::NullValue) },
                            }
                        })
                        .collect()
                })
                .collect()
        } else {
            // Use basic evaluation - this works correctly for both JOIN and non-JOIN results
            // because JOIN results have qualified column names (e.g., "e.name", "d.name")
            // in input_column_names, which get properly mapped to values
            input_result.rows
                .into_iter()
                .map(|row| {
                    let eval_context = self.create_basic_evaluation_context(&input_column_names, &row, context);
                    self.columns
                        .iter()
                        .map(|(_, expr)| {
                            match { let evaluator = ExpressionEvaluator; evaluator.evaluate(expr, &eval_context) } {
                                Ok(value) => value,
                                Err(_) => Value { kind: ValueKind::Null(crate::types::NullValue) },
                            }
                        })
                        .collect()
                })
                .collect()
        };

        context.log(&format!("Projected {} columns into {} rows",
                          column_names.len(), projected_rows.len()));

        Ok(QueryResult {
            rows: projected_rows,
            column_names,
        })
    }

    /// Create evaluation context with proper column name resolution
    fn create_evaluation_context(&self, scanner: &TableScanner, column_names: &[String], row: &[Value], context: &ExecutionContext) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Map column names to values
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        // Include outer context values for correlated subqueries
        if let Some(outer_values) = context.get_outer_context_values() {
            for (column_name, value) in outer_values {
                columns.insert(column_name.clone(), value.clone());
            }
        }

        let mut eval_context = EvaluationContext::with_columns(columns);

        // Pass catalog, buffer_manager, and materialized_ctes for subquery execution
        if let Some(catalog) = context.get_catalog() {
            eval_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            eval_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            eval_context.set_materialized_ctes(materialized_ctes.clone());
        }

        eval_context
    }

    /// Create basic evaluation context (fallback when no scanner available)
    fn create_basic_evaluation_context(&self, column_names: &[String], row: &[Value], context: &ExecutionContext) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Extract table alias information from the input plan
        let table_alias = self.extract_table_alias();

        // Map column names to values, including qualified names if table alias exists
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                let value = &row[i];

                // Store unqualified column name (existing behavior)
                columns.insert(column_name.clone(), value.clone());

                // Also store qualified column name if we have a table alias
                if let Some(ref alias) = table_alias {
                    let qualified_name = format!("{}.{}", alias, column_name);
                    columns.insert(qualified_name, value.clone());
                }
            }
        }

        // Include outer context values for correlated subqueries
        if let Some(outer_values) = context.get_outer_context_values() {
            for (column_name, value) in outer_values {
                columns.insert(column_name.clone(), value.clone());
            }
        }

        let mut eval_context = EvaluationContext::with_columns(columns);

        // Pass catalog, buffer_manager, and materialized_ctes for subquery execution
        if let Some(catalog) = context.get_catalog() {
            eval_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            eval_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            eval_context.set_materialized_ctes(materialized_ctes.clone());
        }

        eval_context
    }

    /// Handle SELECT * expansion
    pub fn expand_star(input: PlanNode, input_column_names: Vec<String>) -> Self {
        let columns: Vec<(String, Expression)> = input_column_names
            .into_iter()
            .map(|name| {
                let expr = Expression::Column {
                    table: None,
                    name: name.clone(),
                };
                (name, expr)
            })
            .collect();

        Self {
            input: Box::new(input),
            columns,
            scanner: None,
            left_columns: None,
            right_columns: None,
        }
    }
}

/// Join operator
#[derive(Debug)]
pub struct JoinOperator {
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub condition: Option<Expression>,
    pub join_type: crate::sql::ast::JoinType,
    pub left_alias: Option<String>,
    pub right_alias: Option<String>,
}

impl JoinOperator {
    pub fn new(left: PlanNode, right: PlanNode, condition: Option<Expression>, join_type: crate::sql::ast::JoinType, left_alias: Option<String>, right_alias: Option<String>) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            condition,
            join_type,
            left_alias,
            right_alias,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let left_result = self.left.execute(context)?;
        let right_result = self.right.execute(context)?;

        context.log(&format!("Executing {} join between {} and {} rows",
                          self.join_type_display(), left_result.rows.len(), right_result.rows.len()));

        let (joined_rows, column_names) = match self.join_type {
            crate::sql::ast::JoinType::Inner | crate::sql::ast::JoinType::Left |
            crate::sql::ast::JoinType::Right | crate::sql::ast::JoinType::Full |
            crate::sql::ast::JoinType::Cross => {
                self.execute_outer_joins(&left_result, &right_result)?
            }
            crate::sql::ast::JoinType::LeftSemi | crate::sql::ast::JoinType::RightSemi => {
                self.execute_semi_join(&left_result, &right_result)?
            }
            crate::sql::ast::JoinType::LeftAnti | crate::sql::ast::JoinType::RightAnti => {
                self.execute_anti_join(&left_result, &right_result)?
            }
        };

        context.log(&format!("Join completed, produced {} rows", joined_rows.len()));

        Ok(QueryResult {
            rows: joined_rows,
            column_names,
        })
    }

    /// Execute inner, left, right, and full joins
    fn execute_outer_joins(&self, left_result: &QueryResult, right_result: &QueryResult) -> Result<(Vec<Vec<Value>>, Vec<String>)> {
        let mut joined_rows = Vec::new();

        // Create qualified column names to avoid ambiguity
        // For nested joins, preserve already-qualified column names from previous joins
        let left_table_name = self.left_alias.as_ref().map(|alias| alias.clone()).unwrap_or_else(|| "left".to_string());
        let right_table_name = self.right_alias.as_ref().map(|alias| alias.clone()).unwrap_or_else(|| "right".to_string());

        let mut joined_column_names = Vec::new();
        for col_name in &left_result.column_names {
            if col_name.contains('.') {
                // Already qualified from a previous JOIN - preserve as-is
                joined_column_names.push(col_name.clone());
            } else {
                // Unqualified - add table alias
                joined_column_names.push(format!("{}.{}", left_table_name, col_name));
            }
        }
        for col_name in &right_result.column_names {
            if col_name.contains('.') {
                // Already qualified from a previous JOIN - preserve as-is
                joined_column_names.push(col_name.clone());
            } else {
                // Unqualified - add table alias
                joined_column_names.push(format!("{}.{}", right_table_name, col_name));
            }
        }

        // Track which rows have been matched for outer joins
        let mut left_matched = vec![false; left_result.rows.len()];
        let mut right_matched = vec![false; right_result.rows.len()];

        // Find matching rows
        for (left_idx, left_row) in left_result.rows.iter().enumerate() {
            for (right_idx, right_row) in right_result.rows.iter().enumerate() {
                let join_condition_satisfied = self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)?;

                if join_condition_satisfied {
                    let mut joined_row = left_row.clone();
                    joined_row.extend(right_row.clone());
                    joined_rows.push(joined_row);

                    left_matched[left_idx] = true;
                    right_matched[right_idx] = true;
                }
            }
        }

        // Add unmatched rows for outer joins
        match self.join_type {
            crate::sql::ast::JoinType::Left => {
                // LEFT JOIN: Add unmatched left rows with NULL for right columns
                for (left_idx, left_row) in left_result.rows.iter().enumerate() {
                    if !left_matched[left_idx] {
                        let mut joined_row = left_row.clone();
                        joined_row.extend(vec![Value { kind: crate::types::ValueKind::Null(crate::types::NullValue) }; right_result.column_names.len()]);
                        joined_rows.push(joined_row);
                    }
                }
            }
            crate::sql::ast::JoinType::Right => {
                // RIGHT JOIN: Add unmatched right rows with NULL for left columns
                for (right_idx, right_row) in right_result.rows.iter().enumerate() {
                    if !right_matched[right_idx] {
                        let mut joined_row = Vec::new();
                        joined_row.extend(vec![Value { kind: crate::types::ValueKind::Null(crate::types::NullValue) }; left_result.column_names.len()]);
                        joined_row.extend(right_row.clone());
                        joined_rows.push(joined_row);
                    }
                }
            }
            crate::sql::ast::JoinType::Full => {
                // FULL JOIN: Add both unmatched left rows and unmatched right rows
                for (left_idx, left_row) in left_result.rows.iter().enumerate() {
                    if !left_matched[left_idx] {
                        let mut joined_row = left_row.clone();
                        joined_row.extend(vec![Value { kind: crate::types::ValueKind::Null(crate::types::NullValue) }; right_result.column_names.len()]);
                        joined_rows.push(joined_row);
                    }
                }
                for (right_idx, right_row) in right_result.rows.iter().enumerate() {
                    if !right_matched[right_idx] {
                        let mut joined_row = Vec::new();
                        joined_row.extend(vec![Value { kind: crate::types::ValueKind::Null(crate::types::NullValue) }; left_result.column_names.len()]);
                        joined_row.extend(right_row.clone());
                        joined_rows.push(joined_row);
                    }
                }
            }
            _ => {} // No outer rows to add for inner join
        }

        Ok((joined_rows, joined_column_names))
    }

    /// Execute semi-joins (EXISTS/IN semantics)
    fn execute_semi_join(&self, left_result: &QueryResult, right_result: &QueryResult) -> Result<(Vec<Vec<Value>>, Vec<String>)> {
        let mut joined_rows = Vec::new();
        let column_names = match self.join_type {
            crate::sql::ast::JoinType::LeftSemi => left_result.column_names.clone(),
            crate::sql::ast::JoinType::RightSemi => right_result.column_names.clone(),
            _ => return Err(crate::error::RustgreSQLError::Internal("Invalid semi-join type".to_string())),
        };

        match self.join_type {
            crate::sql::ast::JoinType::LeftSemi => {
                // For each left row, check if it has at least one match in right
                for left_row in &left_result.rows {
                    for right_row in &right_result.rows {
                        if self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)? {
                            joined_rows.push(left_row.clone());
                            break; // Only need one match for semi-join
                        }
                    }
                }
            }
            crate::sql::ast::JoinType::RightSemi => {
                // For each right row, check if it has at least one match in left
                for right_row in &right_result.rows {
                    for left_row in &left_result.rows {
                        if self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)? {
                            joined_rows.push(right_row.clone());
                            break; // Only need one match for semi-join
                        }
                    }
                }
            }
            _ => {}
        }

        Ok((joined_rows, column_names))
    }

    /// Execute anti-joins (NOT EXISTS/NOT IN semantics)
    fn execute_anti_join(&self, left_result: &QueryResult, right_result: &QueryResult) -> Result<(Vec<Vec<Value>>, Vec<String>)> {
        let mut joined_rows = Vec::new();
        let column_names = match self.join_type {
            crate::sql::ast::JoinType::LeftAnti => left_result.column_names.clone(),
            crate::sql::ast::JoinType::RightAnti => right_result.column_names.clone(),
            _ => return Err(crate::error::RustgreSQLError::Internal("Invalid anti-join type".to_string())),
        };

        match self.join_type {
            crate::sql::ast::JoinType::LeftAnti => {
                // For each left row, include it only if it has NO matches in right
                'outer: for left_row in &left_result.rows {
                    for right_row in &right_result.rows {
                        if self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)? {
                            continue 'outer; // Skip this left row - it has a match
                        }
                    }
                    joined_rows.push(left_row.clone()); // No matches found, include it
                }
            }
            crate::sql::ast::JoinType::RightAnti => {
                // For each right row, include it only if it has NO matches in left
                'outer: for right_row in &right_result.rows {
                    for left_row in &left_result.rows {
                        if self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)? {
                            continue 'outer; // Skip this right row - it has a match
                        }
                    }
                    joined_rows.push(right_row.clone()); // No matches found, include it
                }
            }
            _ => {}
        }

        Ok((joined_rows, column_names))
    }

    /// Evaluate join condition for two rows
    fn evaluate_join_condition(&self, left_row: &[Value], right_row: &[Value],
                                left_columns: &[String], right_columns: &[String]) -> Result<bool> {
        match &self.condition {
            Some(condition) => {
                // Create qualified column names for the evaluation context
                let left_table_name = self.left_alias.as_ref().map(|alias| alias.clone()).unwrap_or_else(|| "left".to_string());
                let right_table_name = self.right_alias.as_ref().map(|alias| alias.clone()).unwrap_or_else(|| "right".to_string());

                let mut columns = std::collections::HashMap::new();

                // Add qualified column names with their values
                // Preserve already-qualified column names from nested JOINs
                for (i, col_name) in left_columns.iter().enumerate() {
                    if i < left_row.len() {
                        let qualified_name = if col_name.contains('.') {
                            // Already qualified from previous JOIN
                            col_name.clone()
                        } else {
                            format!("{}.{}", left_table_name, col_name)
                        };
                        columns.insert(qualified_name, left_row[i].clone());
                        // Also add unqualified name only if not already present (to avoid overwriting in self-joins)
                        if !columns.contains_key(col_name) {
                            columns.insert(col_name.clone(), left_row[i].clone());
                        }
                    }
                }

                for (i, col_name) in right_columns.iter().enumerate() {
                    if i < right_row.len() {
                        let qualified_name = if col_name.contains('.') {
                            // Already qualified from previous JOIN
                            col_name.clone()
                        } else {
                            format!("{}.{}", right_table_name, col_name)
                        };
                        columns.insert(qualified_name, right_row[i].clone());
                        // Don't add unqualified name for right side to avoid ambiguity in self-joins
                        // Use qualified names (e.g., d2.id) to access right side columns
                    }
                }

                let context = EvaluationContext::with_join_data(
                    left_row.to_vec(),
                    right_row.to_vec(),
                    left_columns.to_vec(),
                    right_columns.to_vec(),
                );

                // Replace the empty columns map with our qualified columns
                let context = EvaluationContext {
                    columns,
                    row_data: None,
                    left_row: Some(left_row.to_vec()),
                    right_row: Some(right_row.to_vec()),
                    left_columns: Some(left_columns.to_vec()),
                    right_columns: Some(right_columns.to_vec()),
                    catalog: None,  // TODO: Pass catalog/buffer_manager for subqueries in JOIN conditions
                    buffer_manager: None,
                    subquery_context: None,
                    having_aggregates: None,
                    materialized_ctes: None,
                };

                let result = { let evaluator = ExpressionEvaluator; evaluator.evaluate(condition, &context) }?;
                match result.kind {
                    crate::types::ValueKind::Boolean(val) => Ok(val),
                    _ => Ok(false), // Non-boolean conditions are false
                }
            }
            None => Ok(true), // Cross join - always true
        }
    }

    /// Get display name for join type
    fn join_type_display(&self) -> &'static str {
        match self.join_type {
            crate::sql::ast::JoinType::Inner => "INNER",
            crate::sql::ast::JoinType::Left => "LEFT",
            crate::sql::ast::JoinType::Right => "RIGHT",
            crate::sql::ast::JoinType::Full => "FULL",
            crate::sql::ast::JoinType::Cross => "CROSS",
            crate::sql::ast::JoinType::LeftSemi => "LEFT SEMI",
            crate::sql::ast::JoinType::RightSemi => "RIGHT SEMI",
            crate::sql::ast::JoinType::LeftAnti => "LEFT ANTI",
            crate::sql::ast::JoinType::RightAnti => "RIGHT ANTI",
        }
    }
}

/// Insert operator
#[derive(Debug)]
pub struct InsertOperator {
    pub table_name: String,
    pub columns: Vec<String>,
    pub values: Vec<Vec<Expression>>,
    pub scanner: Option<TableScanner>, // For table metadata and constraint checking
}

impl InsertOperator {
    pub fn new(table_name: String, columns: Vec<String>, values: Vec<Vec<Expression>>) -> Self {
        Self {
            table_name,
            columns,
            values,
            scanner: None,
        }
    }

    pub fn with_scanner(table_name: String, columns: Vec<String>, values: Vec<Vec<Expression>>, scanner: TableScanner) -> Self {
        Self {
            table_name,
            columns,
            values,
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Starting insert into table {}", self.table_name));

        // Get scanner from context (we need mutable access, so we create a new one)
        let mut scanner = if let (Some(ref catalog), Some(ref buffer_manager)) = (&context.catalog, &context.buffer_manager) {
            match TableScanner::new(catalog.clone(), buffer_manager.clone(), &self.table_name) {
                Ok(s) => Some(s),
                Err(e) => {
                    context.log(&format!("Failed to create scanner for table {}: {}", self.table_name, e));
                    None
                }
            }
        } else {
            None
        };

        let mut inserted_rows = 0;

        for (row_index, value_exprs) in self.values.iter().enumerate() {
            // Evaluate expressions to get values
            let provided_values: Vec<Value> = value_exprs
                .iter()
                .map(|expr| {
                    let eval_context = EvaluationContext::new(); // Empty context for INSERT values
                    match { let evaluator = ExpressionEvaluator; evaluator.evaluate(expr, &eval_context) } {
                        Ok(value) => value,
                        Err(e) => {
                            context.log(&format!("Error evaluating expression in row {}: {}", row_index, e));
                            Value { kind: ValueKind::Null(crate::types::NullValue) }
                        }
                    }
                })
                .collect();

            // Map provided values to full table schema
            let row_values = if let Some(scanner) = &scanner {
                self.map_values_to_table_schema(scanner, &provided_values)?
            } else {
                provided_values
            };

            // Insert row data if scanner is available
            if let Some(scanner) = &mut scanner {
                let row_data = RowData::new(row_values);

                // Insert the row
                if let Err(e) = scanner.insert_row(row_data) {
                    context.log(&format!("Failed to insert row {}: {}", row_index, e));
                    return Err(e);
                }
            } else {
                context.log(&format!("No scanner available for table {}, skipping insert for row {}", self.table_name, row_index));
            }

            inserted_rows += 1;
        }

        context.log(&format!("Successfully inserted {} rows into table {}", inserted_rows, self.table_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec!["message".to_string()],
        })
    }

    /// Map provided values to full table schema, handling auto-increment and default values
    fn map_values_to_table_schema(&self, scanner: &TableScanner, provided_values: &[Value]) -> Result<Vec<Value>> {
        let table_def = scanner.get_table_def();
        let mut full_row = vec![Value { kind: ValueKind::Null(crate::types::NullValue) }; table_def.columns.len()];

        if self.columns.is_empty() {
            // Implicit column list: values map to columns in order
            for (i, value) in provided_values.iter().enumerate() {
                if i < full_row.len() {
                    full_row[i] = value.clone();
                }
            }
        } else {
            // Map provided columns to their positions in the table
            for (i, column_name) in self.columns.iter().enumerate() {
                if let Some(column_index) = scanner.get_column_index(column_name) {
                    if i < provided_values.len() {
                        full_row[column_index] = provided_values[i].clone();
                    }
                }
            }
        }

        // Handle auto-increment columns (SERIAL) and DEFAULT values
        for (i, column) in table_def.columns.iter().enumerate() {
            if full_row[i].kind == ValueKind::Null(crate::types::NullValue) {
                // Check if this is a SERIAL column
                if column.data_type.kind == crate::types::DataTypeKind::Serial {
                    // Generate auto-increment value (simplified - should use proper sequence)
                    let next_id = self.generate_auto_increment_value(scanner, &column.name)?;
                    full_row[i] = Value { kind: ValueKind::Integer(next_id) };
                } else if let Some(ref default_value) = column.default_value {
                    // Apply DEFAULT value if column has one
                    full_row[i] = default_value.clone();
                }
            }
        }

        Ok(full_row)
    }

    /// Generate auto-increment value for SERIAL columns
    fn generate_auto_increment_value(&self, scanner: &TableScanner, column_name: &str) -> Result<i64> {
        // Simplified auto-increment - in a real implementation, this would use sequences
        // For now, just return a timestamp-based value to ensure uniqueness
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        // Make it positive and reasonable
        Ok(timestamp % 1000000 + 1)
    }
}

/// Update operator
#[derive(Debug)]
pub struct UpdateOperator {
    pub table_name: String,
    pub assignments: Vec<(String, Expression)>,
    pub condition: Option<Expression>,
    pub scanner: Option<TableScanner>,
}

impl UpdateOperator {
    pub fn new(table_name: String, assignments: Vec<(String, Expression)>, condition: Option<Expression>) -> Self {
        Self {
            table_name,
            assignments,
            condition,
            scanner: None,
        }
    }

    pub fn with_scanner(table_name: String, assignments: Vec<(String, Expression)>, condition: Option<Expression>, scanner: TableScanner) -> Self {
        Self {
            table_name,
            assignments,
            condition,
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Starting update on table {}", self.table_name));

        // Get scanner from context
        let scanner = if let (Some(ref catalog), Some(ref buffer_manager)) = (&context.catalog, &context.buffer_manager) {
            match TableScanner::new(catalog.clone(), buffer_manager.clone(), &self.table_name) {
                Ok(s) => s,
                Err(e) => {
                    context.log(&format!("Failed to create scanner for table {}: {}", self.table_name, e));
                    return Err(e);
                }
            }
        } else {
            context.log(&format!("No catalog or buffer manager available for scanning table {}", self.table_name));
            return Ok(QueryResult {
                rows: vec![],
                column_names: vec![],
            });
        };

        let mut row_iterator = scanner.scan_all()?;
        let column_names = row_iterator.get_column_names();

        let mut updated_rows = 0;

        // Process each row
        while let Some(row_data) = row_iterator.next_row()? {
            // Create evaluation context for this row
            let eval_context = self.create_evaluation_context(&column_names, &row_data.values);

            // Check WHERE condition if present
            let should_update = if let Some(ref condition) = self.condition {
                let evaluator = ExpressionEvaluator;
                match evaluator.evaluate(condition, &eval_context) {
                    Ok(result) => {
                        match ThreeValuedLogic::from_value(&result) {
                            ThreeValuedLogic::True => true,
                            ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                        }
                    }
                    Err(e) => {
                        context.log(&format!("Error evaluating WHERE condition: {}", e));
                        false
                    }
                }
            } else {
                true // No WHERE clause, update all rows
            };

            if should_update {
                // Apply assignments to create new row values
                let mut new_values = row_data.values.clone();

                for (column_name, assignment_expr) in &self.assignments {
                    match { let evaluator = ExpressionEvaluator; evaluator.evaluate(assignment_expr, &eval_context) } {
                        Ok(new_value) => {
                            // Find column index and update value
                            if let Some(column_index) = column_names.iter().position(|name| name == column_name) {
                                if column_index < new_values.len() {
                                    new_values[column_index] = new_value;
                                }
                            } else {
                                context.log(&format!("Warning: Column '{}' not found for update", column_name));
                            }
                        }
                        Err(e) => {
                            context.log(&format!("Error evaluating assignment for column '{}': {}", column_name, e));
                            return Err(e);
                        }
                    }
                }

                // Validate the updated row (preserve row_id from original row_data)
                let updated_row_data = if let Some(row_id) = row_data.row_id {
                    RowData::with_id(new_values, row_id)
                } else {
                    RowData::new(new_values)
                };
                if let Err(e) = scanner.validate_row_data(&updated_row_data) {
                    context.log(&format!("Row validation failed after update: {}", e));
                    return Err(e);
                }

                // Perform the update
                if let Err(e) = self.update_row_data(&scanner, &updated_row_data) {
                    context.log(&format!("Failed to update row: {}", e));
                    return Err(e);
                }

                updated_rows += 1;
            }
        }

        context.log(&format!("Successfully updated {} rows in table {}", updated_rows, self.table_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec!["message".to_string()],
        })
    }

    /// Create evaluation context for row data
    fn create_evaluation_context(&self, column_names: &[String], row_values: &[Value]) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        for (i, column_name) in column_names.iter().enumerate() {
            if i < row_values.len() {
                columns.insert(column_name.clone(), row_values[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Update row data in storage
    fn update_row_data(&self, scanner: &TableScanner, row_data: &RowData) -> Result<()> {
        // Extract the row index from row_id
        let row_index = row_data.row_id
            .ok_or_else(|| crate::error::RustgreSQLError::Execution(
                "Cannot update row: row_id not set".to_string()
            ))? as usize;

        // Use the catalog manager to update the row
        scanner.get_catalog_manager().table_manager.update_row(
            &self.table_name,
            row_index,
            row_data.values.clone()
        )?;

        log::info!("Updated row at index {} in table {}", row_index, self.table_name);
        Ok(())
    }
}

/// Delete operator
#[derive(Debug)]
pub struct DeleteOperator {
    pub table_name: String,
    pub condition: Option<Expression>,
    pub scanner: Option<TableScanner>, // For table scanning
}

impl DeleteOperator {
    pub fn new(table_name: String, condition: Option<Expression>) -> Self {
        Self {
            table_name,
            condition,
            scanner: None,
        }
    }

    pub fn with_scanner(table_name: String, condition: Option<Expression>, scanner: TableScanner) -> Self {
        Self {
            table_name,
            condition,
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Starting delete from table {}", self.table_name));

        let mut deleted_rows = 0;

        if let Some(ref scanner) = self.scanner {
            // Scan all rows from the table
            let mut row_iterator = scanner.scan_all()?;
            let column_names = row_iterator.get_column_names();

            // Collect rows to delete (in a real implementation, you'd mark them for deletion)
            let mut rows_to_delete = Vec::new();

            while let Some(row_data) = row_iterator.next_row()? {
                // Create evaluation context for this row
                let eval_context = self.create_evaluation_context(&column_names, &row_data.values);

                // Check WHERE condition if present
                let should_delete = if let Some(ref condition) = self.condition {
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(condition, &eval_context) {
                        Ok(result) => {
                            match ThreeValuedLogic::from_value(&result) {
                                ThreeValuedLogic::True => true,
                                ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                            }
                        }
                        Err(e) => {
                            context.log(&format!("Error evaluating WHERE condition: {}", e));
                            false
                        }
                    }
                } else {
                    true // No WHERE clause, delete all rows
                };

                if should_delete {
                    rows_to_delete.push(row_data);
                }
            }

            // Sort rows by row_id in descending order to avoid index shifting issues
            // when deleting multiple rows
            rows_to_delete.sort_by(|a, b| {
                b.row_id.cmp(&a.row_id)
            });

            // Perform the deletions (in reverse order of row_id)
            for row_data in rows_to_delete {
                if let Err(e) = self.delete_row_data(scanner, &row_data, context) {
                    context.log(&format!("Failed to delete row: {}", e));
                    return Err(e);
                }
                deleted_rows += 1;
            }
        } else {
            // Fallback: no scanner available
            context.log("No scanner available for delete operation");
        }

        context.log(&format!("Successfully deleted {} rows from table {}", deleted_rows, self.table_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Create evaluation context for row data
    fn create_evaluation_context(&self, column_names: &[String], row_values: &[Value]) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        for (i, column_name) in column_names.iter().enumerate() {
            if i < row_values.len() {
                columns.insert(column_name.clone(), row_values[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Delete row data from storage
    fn delete_row_data(&self, scanner: &TableScanner, row_data: &RowData, context: &mut ExecutionContext) -> Result<()> {
        // Extract the row index from row_id
        let row_index = row_data.row_id
            .ok_or_else(|| crate::error::RustgreSQLError::Execution(
                "Cannot delete row: row_id not set".to_string()
            ))? as usize;

        // Get the transaction ID and manager
        let transaction_id = context.transaction_id;
        let transaction_manager = context.get_transaction_manager();

        // If we're in a transaction and have a transaction manager, log the deletion
        if let (Some(tx_id), Some(tm)) = (transaction_id, transaction_manager) {
            // Get the old row data before deletion for rollback
            let old_row = scanner.get_catalog_manager().table_manager.get_row(&self.table_name, row_index)
                .map_err(|e| crate::error::RustgreSQLError::Execution(
                    format!("Failed to get old row data for transaction logging: {}", e)
                ))?;

            if let Some(ref old_row_data) = old_row {
                // Log the deletion to the transaction manager with table name and row data
                tm.log_delete(tx_id, self.table_name.clone(), row_index, old_row_data.clone())
                    .map_err(|e| crate::error::RustgreSQLError::Execution(
                        format!("Failed to log deletion to transaction manager: {}", e)
                    ))?;

                log::debug!("Logged deletion to transaction manager for tx_id={}, table={}, row_index={}", tx_id, self.table_name, row_index);
            }
        }

        // Use the catalog manager to delete the row
        let old_row = scanner.get_catalog_manager().table_manager.delete_row(
            &self.table_name,
            row_index
        )?;

        if let Some(ref old_row_data) = old_row {
            log::info!("Deleted row at index {} from table {} ({} columns)", row_index, self.table_name, old_row_data.len());
        } else {
            log::info!("Deleted row at index {} from table {}", row_index, self.table_name);
        }

        Ok(())
    }
}

/// Index scan operator
#[derive(Debug)]
pub struct IndexScanOperator {
    pub table_name: String,
    pub index_name: String,
    pub index_condition: Option<Expression>,
    pub scanner: Option<TableScanner>,
    pub columns: Vec<String>, // Columns to return (empty means all)
}

impl IndexScanOperator {
    pub fn new(table_name: String, index_name: String, index_condition: Option<Expression>, columns: Vec<String>) -> Self {
        Self {
            table_name,
            index_name,
            index_condition,
            scanner: None,
            columns,
        }
    }

    pub fn with_scanner(table_name: String, index_name: String, index_condition: Option<Expression>, columns: Vec<String>, scanner: TableScanner) -> Self {
        Self {
            table_name,
            index_name,
            index_condition,
            scanner: None,
            columns,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Performing index scan on table: {} using index: {}", self.table_name, self.index_name));

        if let Some(ref scanner) = self.scanner {
            // In a real implementation, this would:
            // 1. Use the index to find matching row IDs
            // 2. Fetch heap pages for those row IDs
            // 3. Apply any remaining index conditions
            // 4. Return the requested columns

            // For now, we'll simulate with a full scan and log that we're using an index
            let mut row_iterator = scanner.scan_all()?;
            let mut rows = Vec::new();
            let mut column_names = Vec::new();

            // Get column names from the first iteration
            if let Some(first_row) = row_iterator.next_row()? {
                column_names = row_iterator.get_column_names();

                // Filter columns based on requested columns
                let filtered_columns = if self.columns.is_empty() {
                    column_names.clone()
                } else {
                    self.columns.clone()
                };

                let mut filtered_values = Vec::new();
                for col_name in &filtered_columns {
                    if let Some(index) = column_names.iter().position(|name| name == col_name) {
                        filtered_values.push(first_row.values[index].clone());
                    }
                }

                rows.push(filtered_values);

                // Process remaining rows
                while let Some(row_data) = row_iterator.next_row()? {
                    let mut filtered_values = Vec::new();
                    for col_name in &filtered_columns {
                        if let Some(index) = column_names.iter().position(|name| name == col_name) {
                            filtered_values.push(row_data.values[index].clone());
                        }
                    }
                    rows.push(filtered_values);
                }

                Ok(QueryResult {
                    rows,
                    column_names: filtered_columns,
                })
            } else {
                Ok(QueryResult {
                    rows: vec![],
                    column_names: if self.columns.is_empty() {
                        scanner.get_table_def().columns.iter().map(|c| c.name.clone()).collect()
                    } else {
                        self.columns.clone()
                    },
                })
            }
        } else {
            context.log("No table scanner available for index scan");
            Ok(QueryResult {
                rows: vec![],
                column_names: self.columns.clone(),
            })
        }
    }
}

/// Index-only scan operator (covering index)
#[derive(Debug)]
pub struct IndexOnlyScanOperator {
    pub table_name: String,
    pub index_name: String,
    pub index_condition: Option<Expression>,
    pub scanner: Option<TableScanner>,
    pub columns: Vec<String>, // All requested columns must be in the index
}

impl IndexOnlyScanOperator {
    pub fn new(table_name: String, index_name: String, index_condition: Option<Expression>, columns: Vec<String>) -> Self {
        Self {
            table_name,
            index_name,
            index_condition,
            scanner: None,
            columns,
        }
    }

    pub fn with_scanner(table_name: String, index_name: String, index_condition: Option<Expression>, columns: Vec<String>, scanner: TableScanner) -> Self {
        Self {
            table_name,
            index_name,
            index_condition,
            scanner: None,
            columns,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Performing index-only scan on table: {} using index: {}", self.table_name, self.index_name));

        if let Some(ref scanner) = self.scanner {
            // In a real implementation, this would:
            // 1. Use the index to find matching entries
            // 2. Return data directly from the index without accessing heap pages
            // 3. All requested columns must be present in the index (covering index)

            // For now, simulate with a regular scan but log that we're avoiding heap access
            let mut row_iterator = scanner.scan_all()?;
            let mut rows = Vec::new();
            let mut column_names = Vec::new();

            if let Some(first_row) = row_iterator.next_row()? {
                column_names = row_iterator.get_column_names();

                // Filter columns based on requested columns
                let filtered_columns = if self.columns.is_empty() {
                    column_names.clone()
                } else {
                    self.columns.clone()
                };

                let mut filtered_values = Vec::new();
                for col_name in &filtered_columns {
                    if let Some(index) = column_names.iter().position(|name| name == col_name) {
                        filtered_values.push(first_row.values[index].clone());
                    }
                }

                rows.push(filtered_values);

                // Process remaining rows
                while let Some(row_data) = row_iterator.next_row()? {
                    let mut filtered_values = Vec::new();
                    for col_name in &filtered_columns {
                        if let Some(index) = column_names.iter().position(|name| name == col_name) {
                            filtered_values.push(row_data.values[index].clone());
                        }
                    }
                    rows.push(filtered_values);
                }

                Ok(QueryResult {
                    rows,
                    column_names: filtered_columns,
                })
            } else {
                Ok(QueryResult {
                    rows: vec![],
                    column_names: if self.columns.is_empty() {
                        scanner.get_table_def().columns.iter().map(|c| c.name.clone()).collect()
                    } else {
                        self.columns.clone()
                    },
                })
            }
        } else {
            context.log("No table scanner available for index-only scan");
            Ok(QueryResult {
                rows: vec![],
                column_names: self.columns.clone(),
            })
        }
    }
}

/// Hash join operator
#[derive(Debug)]
pub struct HashJoinOperator {
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub condition: Option<Expression>,
    pub join_type: crate::sql::ast::JoinType,
    pub hash_key_columns: Vec<String>, // Columns to hash on for equi-joins
}

impl HashJoinOperator {
    pub fn new(
        left: PlanNode,
        right: PlanNode,
        condition: Option<Expression>,
        join_type: crate::sql::ast::JoinType,
        hash_key_columns: Vec<String>,
    ) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            condition,
            join_type,
            hash_key_columns,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Starting hash join operation");

        // Execute both input plans
        let left_result = self.left.execute(context)?;
        let right_result = self.right.execute(context)?;

        // Build hash table from right input (usually the smaller relation)
        let hash_table = self.build_hash_table(&right_result, context)?;

        // Probe hash table with left input
        let mut joined_rows = Vec::new();
        let mut joined_column_names = left_result.column_names.clone();
        joined_column_names.extend(right_result.column_names.clone());

        for left_row in &left_result.rows {
            let hash_key = self.extract_hash_key(left_row, &left_result.column_names);

            if let Some(right_matches) = hash_table.get(&hash_key) {
                for right_row in right_matches {
                    // Check full join condition if present
                    let join_condition_satisfied = self.evaluate_join_condition(
                        left_row,
                        right_row,
                        &left_result.column_names,
                        &right_result.column_names,
                    )?;

                    if join_condition_satisfied {
                        let mut joined_row = left_row.clone();
                        joined_row.extend(right_row.iter().cloned());
                        joined_rows.push(joined_row);
                    }
                }
            }
        }

        context.log(&format!("Hash join: built hash table with {} entries, matched {} rows",
                          hash_table.len(), joined_rows.len()));

        Ok(QueryResult {
            rows: joined_rows,
            column_names: joined_column_names,
        })
    }

    fn build_hash_table(&self, input: &QueryResult, _context: &ExecutionContext) -> Result<std::collections::HashMap<String, Vec<Vec<Value>>>> {
        let mut hash_table: std::collections::HashMap<String, Vec<Vec<Value>>> = std::collections::HashMap::new();

        for row in &input.rows {
            let hash_key = self.extract_hash_key(row, &input.column_names);
            hash_table.entry(hash_key).or_insert_with(Vec::new).push(row.clone());
        }

        Ok(hash_table)
    }

    fn extract_hash_key(&self, row: &[Value], column_names: &[String]) -> String {
        let mut key_parts = Vec::new();

        for col_name in &self.hash_key_columns {
            if let Some(col_index) = column_names.iter().position(|name| name == col_name) {
                if col_index < row.len() {
                    key_parts.push(format!("{:?}", row[col_index]));
                }
            }
        }

        key_parts.join("|")
    }

    fn evaluate_join_condition(
        &self,
        left_row: &[Value],
        right_row: &[Value],
        left_columns: &[String],
        right_columns: &[String],
    ) -> Result<bool> {
        match &self.condition {
            Some(condition) => {
                // Create evaluation context with both rows
                let mut all_columns = left_columns.to_vec();
                all_columns.extend(right_columns.to_vec());

                let mut all_values = left_row.to_vec();
                all_values.extend(right_row.to_vec());

                // Create HashMap for column names to values
                use std::collections::HashMap;
                let mut column_map = HashMap::new();
                for (col_name, value) in all_columns.iter().zip(all_values.iter()) {
                    column_map.insert(col_name.clone(), value.clone());
                }

                let context = EvaluationContext::with_columns(column_map);

                let result = { let evaluator = ExpressionEvaluator; evaluator.evaluate(condition, &context) }?;
                match result.kind {
                    crate::types::ValueKind::Boolean(val) => Ok(val),
                    _ => Ok(false), // Non-boolean conditions are false
                }
            }
            None => Ok(true), // Cross join
        }
    }
}

/// Merge join operator
#[derive(Debug)]
pub struct MergeJoinOperator {
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,
    pub condition: Option<Expression>,
    pub join_type: crate::sql::ast::JoinType,
    pub sort_columns: Vec<String>, // Columns to sort on for merge join
}

impl MergeJoinOperator {
    pub fn new(
        left: PlanNode,
        right: PlanNode,
        condition: Option<Expression>,
        join_type: crate::sql::ast::JoinType,
        sort_columns: Vec<String>,
    ) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            condition,
            join_type,
            sort_columns,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Starting merge join operation");

        // Execute both input plans
        let mut left_result = self.left.execute(context)?;
        let mut right_result = self.right.execute(context)?;

        // Sort both inputs on the merge key
        self.sort_input(&mut left_result, &self.sort_columns, context)?;
        self.sort_input(&mut right_result, &self.sort_columns, context)?;

        // Perform merge join
        let mut joined_rows = Vec::new();
        let mut joined_column_names = left_result.column_names.clone();
        joined_column_names.extend(right_result.column_names.clone());

        let mut left_idx = 0;
        let mut right_idx = 0;

        while left_idx < left_result.rows.len() && right_idx < right_result.rows.len() {
            let left_row = &left_result.rows[left_idx];
            let right_row = &right_result.rows[right_idx];

            let comparison = self.compare_rows(left_row, right_row, &left_result.column_names, &right_result.column_names);

            match comparison {
                std::cmp::Ordering::Equal => {
                    // Rows match - check full join condition if present
                    if self.evaluate_join_condition(left_row, right_row, &left_result.column_names, &right_result.column_names)? {
                        let mut joined_row = left_row.clone();
                        joined_row.extend(right_row.clone());
                        joined_rows.push(joined_row);
                    }

                    // Find all matches for current key (handle duplicates)
                    let mut left_end = left_idx;
                    while left_end < left_result.rows.len() &&
                          self.compare_rows(&left_result.rows[left_end], left_row, &left_result.column_names, &left_result.column_names) == std::cmp::Ordering::Equal {
                        left_end += 1;
                    }

                    let mut right_end = right_idx;
                    while right_end < right_result.rows.len() &&
                          self.compare_rows(&right_result.rows[right_end], right_row, &right_result.column_names, &right_result.column_names) == std::cmp::Ordering::Equal {
                        right_end += 1;
                    }

                    // Generate all combinations of matching rows
                    for i in left_idx..left_end {
                        for j in right_idx..right_end {
                            let left_match = &left_result.rows[i];
                            let right_match = &right_result.rows[j];

                            if self.evaluate_join_condition(left_match, right_match, &left_result.column_names, &right_result.column_names)? {
                                let mut joined_row = left_match.clone();
                                joined_row.extend(right_match.clone());
                                joined_rows.push(joined_row);
                            }
                        }
                    }

                    left_idx = left_end;
                    right_idx = right_end;
                }
                std::cmp::Ordering::Less => {
                    left_idx += 1;
                }
                std::cmp::Ordering::Greater => {
                    right_idx += 1;
                }
            }
        }

        context.log(&format!("Merge join: processed {} left rows, {} right rows, produced {} joined rows",
                          left_result.rows.len(), right_result.rows.len(), joined_rows.len()));

        Ok(QueryResult {
            rows: joined_rows,
            column_names: joined_column_names,
        })
    }

    fn sort_input(&self, result: &mut QueryResult, sort_columns: &[String], context: &mut ExecutionContext) -> Result<()> {
        if sort_columns.is_empty() {
            return Ok(());
        }

        context.log(&format!("Sorting {} rows by columns: {:?}", result.rows.len(), sort_columns));

        // Get column indices for sorting
        let sort_indices: Vec<usize> = sort_columns
            .iter()
            .filter_map(|col_name| result.column_names.iter().position(|name| name == col_name))
            .collect();

        if sort_indices.is_empty() {
            return Ok(()); // No valid sort columns found
        }

        // Sort rows using the sort indices
        result.rows.sort_by(|a, b| {
            for &idx in &sort_indices {
                if idx < a.len() && idx < b.len() {
                    let comparison = compare_values(&a[idx], &b[idx], NullsPosition::Default);
                    if comparison != std::cmp::Ordering::Equal {
                        return comparison;
                    }
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(())
    }

    fn compare_rows(
        &self,
        left_row: &[Value],
        right_row: &[Value],
        left_columns: &[String],
        right_columns: &[String],
    ) -> std::cmp::Ordering {
        for col_name in &self.sort_columns {
            let left_idx = left_columns.iter().position(|name| name == col_name);
            let right_idx = right_columns.iter().position(|name| name == col_name);

            if let (Some(left_idx), Some(right_idx)) = (left_idx, right_idx) {
                if left_idx < left_row.len() && right_idx < right_row.len() {
                    let comparison = compare_values(&left_row[left_idx], &right_row[right_idx], NullsPosition::Default);
                    if comparison != std::cmp::Ordering::Equal {
                        return comparison;
                    }
                }
            }
        }
        std::cmp::Ordering::Equal
    }

    fn evaluate_join_condition(
        &self,
        left_row: &[Value],
        right_row: &[Value],
        left_columns: &[String],
        right_columns: &[String],
    ) -> Result<bool> {
        match &self.condition {
            Some(condition) => {
                // Create evaluation context with both rows
                let mut all_columns = left_columns.to_vec();
                all_columns.extend(right_columns.to_vec());

                let mut all_values = left_row.to_vec();
                all_values.extend(right_row.to_vec());

                // Create HashMap for column names to values
                use std::collections::HashMap;
                let mut column_map = HashMap::new();
                for (col_name, value) in all_columns.iter().zip(all_values.iter()) {
                    column_map.insert(col_name.clone(), value.clone());
                }

                let context = EvaluationContext::with_columns(column_map);

                let result = { let evaluator = ExpressionEvaluator; evaluator.evaluate(condition, &context) }?;
                match result.kind {
                    crate::types::ValueKind::Boolean(val) => Ok(val),
                    _ => Ok(false), // Non-boolean conditions are false
                }
            }
            None => Ok(true), // Cross join
        }
    }

    /// Extract hash key columns from join condition for optimization
    pub fn extract_hash_key_columns(&self) -> Vec<String> {
        match &self.condition {
            Some(Expression::BinaryOp { left, op, right }) => {
                if matches!(op, BinaryOperator::Equals) {
                    // Extract equality columns for hash join optimization
                    if let (Expression::Column { name: left_col, .. }, Expression::Column { name: right_col, .. }) = (left.as_ref(), right.as_ref()) {
                        return vec![left_col.clone(), right_col.clone()];
                    }
                }
                vec![]
            }
            _ => vec![]
        }
    }

    /// Check if this join condition is suitable for hash join
    pub fn is_hash_join_suitable(&self) -> bool {
        match &self.condition {
            Some(condition) => self.has_equality_condition(condition),
            None => true // Cross join can use hash join
        }
    }

    /// Check if this join condition is suitable for merge join
    pub fn is_merge_join_suitable(&self) -> bool {
        match &self.condition {
            Some(condition) => self.has_equality_condition(condition),
            None => true // Cross join can use merge join
        }
    }

    /// Recursively check if condition contains equality operators
    fn has_equality_condition(&self, expr: &Expression) -> bool {
        match expr {
            Expression::BinaryOp { op, .. } if matches!(op, BinaryOperator::Equals) => true,
            Expression::BinaryOp { left, right, .. } => {
                self.has_equality_condition(left) || self.has_equality_condition(right)
            }
            _ => false
        }
    }

    /// Check if join condition is a non-equi join (>, <, BETWEEN, LIKE, etc.)
    pub fn is_non_equi_join(&self) -> bool {
        match &self.condition {
            Some(condition) => self.contains_non_equi_operator(condition),
            None => false
        }
    }

    /// Recursively check if condition contains non-equality operators
    fn contains_non_equi_operator(&self, expr: &Expression) -> bool {
        match expr {
            Expression::BinaryOp { left, op, right } => {
                if matches!(op, BinaryOperator::GreaterThan |
                              BinaryOperator::GreaterThanOrEquals |
                              BinaryOperator::LessThan |
                              BinaryOperator::LessThanOrEquals |
                              BinaryOperator::Like |
                              BinaryOperator::ILike) {
                    return true;
                }
                // Recursively check nested expressions
                self.contains_non_equi_operator(left) || self.contains_non_equi_operator(right)
            }
            _ => false
        }
    }
}

/// Helper function to match aggregate function expressions in HAVING clauses to their computed results
fn match_aggregate_in_having(
    having_expr: &Expression,
    aggregate_functions: &[(String, Expression)],
    result_row: &[Value],
    group_by_columns_len: usize,
) -> Option<Value> {
    match having_expr {
        Expression::Function { name, args, .. } => {
            let func_name = name.to_uppercase();
            if matches!(func_name.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX") {
                // Try to find matching aggregate function
                for (i, (alias, agg_expr)) in aggregate_functions.iter().enumerate() {
                    if expressions_match(having_expr, agg_expr) {
                        let aggregate_col_index = group_by_columns_len + i;
                        if aggregate_col_index < result_row.len() {
                            return Some(result_row[aggregate_col_index].clone());
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Check if two expressions are structurally equivalent for matching purposes
fn expressions_match(expr1: &Expression, expr2: &Expression) -> bool {
    match (expr1, expr2) {
        (Expression::Function { name: name1, args: args1, .. },
         Expression::Function { name: name2, args: args2, .. }) => {
            name1.to_uppercase() == name2.to_uppercase() &&
            args1.len() == args2.len() &&
            args1.iter().zip(args2.iter()).all(|(a1, a2)| expressions_match(a1, a2))
        }
        (Expression::Column { name: name1, .. },
         Expression::Column { name: name2, .. }) => {
            name1.to_uppercase() == name2.to_uppercase()
        }
        (Expression::Literal(val1), Expression::Literal(val2)) => {
            val1 == val2
        }
        _ => false,
    }
}

/// Aggregate operator for GROUP BY and aggregate functions
#[derive(Debug)]
/// Aggregate operator with window function support
pub struct AggregateOperator {
    pub input: Box<PlanNode>,
    pub group_by_columns: Vec<Expression>,
    pub aggregate_functions: Vec<(String, Expression)>, // (alias, aggregate_expr)
    pub having_clause: Option<Expression>, // HAVING clause filter
    pub window_functions: Vec<(String, crate::sql::ast::WindowFunction)>, // (alias, window_function)
    pub scanner: Option<TableScanner>, // For column name resolution
}

impl AggregateOperator {
    pub fn new(
        input: PlanNode,
        group_by_columns: Vec<Expression>,
        aggregate_functions: Vec<(String, Expression)>,
        having_clause: Option<Expression>,
    ) -> Self {
        Self {
            input: Box::new(input),
            group_by_columns,
            aggregate_functions,
            having_clause,
            window_functions: Vec::new(),
            scanner: None,
        }
    }

    pub fn with_scanner(
        input: PlanNode,
        group_by_columns: Vec<Expression>,
        aggregate_functions: Vec<(String, Expression)>,
        having_clause: Option<Expression>,
        scanner: TableScanner,
    ) -> Self {
        Self {
            input: Box::new(input),
            group_by_columns,
            aggregate_functions,
            having_clause,
            window_functions: Vec::new(),
            scanner: None,
        }
    }

    /// Create a new aggregate operator with window functions
    pub fn with_window_functions(
        input: PlanNode,
        group_by_columns: Vec<Expression>,
        aggregate_functions: Vec<(String, Expression)>,
        window_functions: Vec<(String, crate::sql::ast::WindowFunction)>,
        having_clause: Option<Expression>,
        scanner: Option<TableScanner>,
    ) -> Self {
        Self {
            input: Box::new(input),
            group_by_columns,
            aggregate_functions,
            having_clause,
            window_functions,
            scanner,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Starting aggregate operation with window function support");

        let input_result = self.input.execute(context)?;
        let input_column_names = input_result.column_names.clone();

        if input_result.rows.is_empty() {
            // Handle empty input case
            let mut output_column_names: Vec<String> = self.aggregate_functions
                .iter()
                .map(|(alias, _)| alias.clone())
                .collect();

              // Add window function column names
            for (alias, _) in &self.window_functions {
                output_column_names.push(alias.clone());
            }

            // For empty input with no GROUP BY, return single row with initial aggregate values
            if self.group_by_columns.is_empty() {
                let mut row_values: Vec<Value> = Vec::new();

                // Compute initial aggregate results
                for (_, expr) in &self.aggregate_functions {
                    let state = self.create_aggregate_state(expr);
                    let value = state.result().unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) });
                    row_values.push(value);
                }

              // Apply window functions if present (for empty input case)
                if !self.window_functions.is_empty() {
                    // Create a QueryResult with the aggregate values to pass to WindowOperator
                    let aggregate_result = QueryResult {
                        rows: vec![row_values],
                        column_names: output_column_names,
                    };

                    // Create a window operator to apply window functions on the aggregate result
                    let window_op = WindowOperator {
                        input: Box::new(PlanNode::Values {
                            rows: aggregate_result.rows.clone(),
                            column_names: aggregate_result.column_names.clone(),
                        }),
                        window_functions: self.window_functions.iter().map(|(alias, wf)| (alias.clone(), wf.clone())).collect(),
                        partition_by: Vec::new(), // Window functions specify their own partitioning
                        order_by: Vec::new(),      // Window functions specify their own ordering
                        window_frame: None,
                        scanner: None,
                    };

                    return window_op.execute(context);
                }

                return Ok(QueryResult {
                    rows: vec![row_values],
                    column_names: output_column_names,
                });
            } else {
                return Ok(QueryResult {
                    rows: vec![],
                    column_names: output_column_names,
                });
            }
        }

        // Hash-based aggregation: group rows by GROUP BY key
        let mut groups: std::collections::HashMap<String, (Vec<Value>, Vec<AggregateState>)> = std::collections::HashMap::new();

        // Initialize aggregate states for each group
        for row in &input_result.rows {
            // Compute group key
            let group_key = if self.group_by_columns.is_empty() {
                // No GROUP BY - all rows go into one group
                "SINGLE_GROUP".to_string()
            } else {
                let eval_context = if let Some(ref scanner) = self.scanner {
                    self.create_evaluation_context(scanner, &input_column_names, row)
                } else {
                    self.create_basic_evaluation_context(&input_column_names, row)
                };

                let mut key_parts = Vec::new();
                for group_expr in &self.group_by_columns {
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(group_expr, &eval_context) {
                        Ok(value) => key_parts.push(format!("{:?}", value)),
                        Err(_) => key_parts.push("NULL".to_string()),
                    }
                }
                key_parts.join("|")
            };

            // Get or create group entry
            let entry = groups.entry(group_key).or_insert_with(|| {
                // Initialize aggregate states for this group
                let aggregate_states: Vec<AggregateState> = self.aggregate_functions
                    .iter()
                    .map(|(_, expr)| self.create_aggregate_state(expr))
                    .collect();

                // Compute group by values for output
                let group_values = if self.group_by_columns.is_empty() {
                    vec![]
                } else {
                    let eval_context = if let Some(ref scanner) = self.scanner {
                        self.create_evaluation_context(scanner, &input_column_names, row)
                    } else {
                        self.create_basic_evaluation_context(&input_column_names, row)
                    };

                    self.group_by_columns
                        .iter()
                        .map(|expr| {
                            { let evaluator = ExpressionEvaluator; evaluator.evaluate(expr, &eval_context) }
                                .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                        })
                        .collect()
                };

                (group_values, aggregate_states)
            });

            // Update aggregate states with this row
            let eval_context = if let Some(ref scanner) = self.scanner {
                self.create_evaluation_context(scanner, &input_column_names, row)
            } else {
                self.create_basic_evaluation_context(&input_column_names, row)
            };

            for (i, (_, aggregate_expr)) in self.aggregate_functions.iter().enumerate() {
                let evaluator = ExpressionEvaluator;
                let value = match aggregate_expr {
                    Expression::Function { name, args, distinct: _ } => {
                        match name.to_uppercase().as_str() {
                            "COUNT" => {
                                // Handle COUNT(*) special case
                                if args.len() == 1 && matches!(&args[0], Expression::Star) {
                                    Value { kind: ValueKind::Integer(1) } // Count this row
                                } else {
                                    // COUNT(expression) - evaluate the expression argument
                                    if let Some(arg) = args.first() {
                                        evaluator.evaluate(arg, &eval_context)
                                            .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                                    } else {
                                        Value { kind: ValueKind::Null(crate::types::NullValue) }
                                    }
                                }
                            }
                            _ => {
                                // For other aggregate functions (SUM, AVG, MIN, MAX), evaluate the function argument
                                if let Some(arg) = args.first() {
                                    evaluator.evaluate(arg, &eval_context)
                                        .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                                } else {
                                    Value { kind: ValueKind::Null(crate::types::NullValue) }
                                }
                            }
                        }
                    }
                    _ => {
                        // Non-function expressions - evaluate as-is
                        evaluator.evaluate(aggregate_expr, &eval_context)
                            .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                    }
                };

                if let Err(e) = entry.1[i].update(&value) {
                    context.log(&format!("Error updating aggregate {}: {}", i, e));
                }
            }
        }

        // Build result rows from groups
        let mut result_rows = Vec::new();
        let mut result_column_names = Vec::new();

        // Add GROUP BY columns to output
        for group_expr in &self.group_by_columns {
            if let Expression::Column { name, .. } = group_expr {
                result_column_names.push(name.clone());
            } else {
                // For complex expressions, use a generated name
                result_column_names.push("group_expr".to_string());
            }
        }

        // Add aggregate function columns to output
        for (alias, _) in &self.aggregate_functions {
            result_column_names.push(alias.clone());
        }

        // Add window function columns to output
        for (alias, _) in &self.window_functions {
            result_column_names.push(alias.clone());
        }

        // Process each group
        for (_, (group_values, aggregate_states)) in groups {
            let mut result_row = group_values;

            // Finalize all aggregates
            for aggregate_state in aggregate_states {
                match aggregate_state.result() {
                    Ok(value) => result_row.push(value),
                    Err(e) => {
                        context.log(&format!("Error finalizing aggregate: {}", e));
                        result_row.push(Value { kind: ValueKind::Null(crate::types::NullValue) });
                    }
                }
            }

            // Apply HAVING clause filter if present
            if let Some(ref having_expr) = self.having_clause {
                // Create evaluation context with group by columns and aggregate results
                let mut having_columns = std::collections::HashMap::new();
                let mut having_aggregates = std::collections::HashMap::new();

                // Add GROUP BY columns to context
                for (i, group_expr) in self.group_by_columns.iter().enumerate() {
                    if let Expression::Column { name, .. } = group_expr {
                        if i < result_row.len() {
                            having_columns.insert(name.clone(), result_row[i].clone());
                        }
                    }
                }

                // Add aggregate results to context using their aliases and populate having_aggregates
                for (i, (alias, agg_expr)) in self.aggregate_functions.iter().enumerate() {
                    let aggregate_col_index = self.group_by_columns.len() + i;
                    if aggregate_col_index < result_row.len() {
                        let aggregate_value = &result_row[aggregate_col_index];

                        // Add by alias for backward compatibility
                        having_columns.insert(alias.clone(), aggregate_value.clone());

                        // Add by function expression for direct aggregate function calls in HAVING
                        if let Expression::Function { name, args, .. } = agg_expr {
                            let mut key_parts = vec![name.to_uppercase()];
                            for arg in args {
                                match arg {
                                    Expression::Column { name, .. } => key_parts.push(name.to_uppercase()),
                                    Expression::Literal(_) => key_parts.push("LITERAL".to_string()),
                                    _ => key_parts.push("EXPR".to_string()),
                                }
                            }
                            let key = format!("{}({})", key_parts[0], key_parts[1..].join(","));
                            having_aggregates.insert(key, aggregate_value.clone());
                        }
                    }
                }

                let mut having_context = EvaluationContext::with_columns(having_columns);
                having_context.set_having_aggregates(having_aggregates);

                // Evaluate HAVING clause
                let evaluator = ExpressionEvaluator;
                match evaluator.evaluate(having_expr, &having_context) {
                    Ok(having_value) => {
                        let should_include = match having_value.kind {
                            ValueKind::Boolean(b) => b,
                            ValueKind::Null(crate::types::NullValue) => false, // NULL evaluates to FALSE in HAVING
                            _ => true, // Non-boolean values are truthy
                        };

                        if !should_include {
                            context.log("Group filtered out by HAVING clause");
                            continue; // Skip this group
                        }
                    }
                    Err(e) => {
                        context.log(&format!("Error evaluating HAVING clause: {}", e));
                        // On error, include the group (conservative approach)
                    }
                }
            }

            result_rows.push(result_row);
        }

        context.log(&format!("Aggregated {} input rows into {} output groups",
                          input_result.rows.len(), result_rows.len()));

        // Apply window functions if present
        if !self.window_functions.is_empty() {
            let aggregated_result = QueryResult {
                rows: result_rows,
                column_names: result_column_names,
            };

            // Create a window operator to apply window functions on the aggregated result
            let window_op = WindowOperator {
                input: Box::new(PlanNode::Values {
                    rows: aggregated_result.rows.clone(),
                    column_names: aggregated_result.column_names.clone(),
                }),
                window_functions: self.window_functions.iter().map(|(alias, wf)| (alias.clone(), wf.clone())).collect(),
                partition_by: Vec::new(), // Window functions specify their own partitioning
                order_by: Vec::new(),      // Window functions specify their own ordering
                window_frame: None,
                scanner: None,
            };

            return window_op.execute(context);
        }

        Ok(QueryResult {
            rows: result_rows,
            column_names: result_column_names,
        })
    }

    /// Create initial aggregate state for an expression
    fn create_aggregate_state(&self, expr: &Expression) -> AggregateState {
        match expr {
            Expression::Function { name, distinct, .. } => {
                match name.to_uppercase().as_str() {
                    "COUNT" => {
                        // Use the distinct flag from the parsed expression
                        AggregateState::new_count(*distinct)
                    }
                    "SUM" => {
                        AggregateState::new_sum(*distinct)
                    }
                    "AVG" => {
                        AggregateState::new_avg(*distinct)
                    }
                    "MIN" => AggregateState::new_min(),
                    "MAX" => AggregateState::new_max(),
                    _ => {
                        // Unknown aggregate function - default to COUNT
                        AggregateState::new_count(false)
                    }
                }
            }
            _ => {
                // Non-function expressions in aggregate context - treat as COUNT
                AggregateState::new_count(false)
            }
        }
    }

    /// Create evaluation context with proper column name resolution
    fn create_evaluation_context(&self, scanner: &TableScanner, column_names: &[String], row: &[Value]) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Map column names to values
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }

    /// Create basic evaluation context (fallback when no scanner available)
    fn create_basic_evaluation_context(&self, column_names: &[String], row: &[Value]) -> EvaluationContext {
        let mut columns = std::collections::HashMap::new();

        // Simple column name to value mapping
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                columns.insert(column_name.clone(), row[i].clone());
            }
        }

        EvaluationContext::with_columns(columns)
    }
}

/// Set operation operator (UNION, INTERSECT, EXCEPT)
#[derive(Debug)]
pub struct SetOperationOperator {
    pub operator: SetOperatorType,
    pub left: PlanNode,
    pub right: PlanNode,
    pub all: bool,
}

impl SetOperationOperator {
    pub fn new(operator: SetOperatorType, left: PlanNode, right: PlanNode, all: bool) -> Self {
        Self {
            operator,
            left,
            right,
            all,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Executing set operation: {:?} (ALL: {})", self.operator, self.all));

        // Execute left and right subqueries
        let left_result = self.left.execute(context)?;
        let right_result = self.right.execute(context)?;

        context.log(&format!("Left result: {} rows", left_result.rows.len()));
        context.log(&format!("Right result: {} rows", right_result.rows.len()));

        // Validate that schemas are compatible
        if left_result.column_names.len() != right_result.column_names.len() {
            return Err(crate::error::RustgreSQLError::Internal(
                "Set operations require compatible column counts".to_string()
            ));
        }

        // Perform the set operation
        let result_rows = match self.operator {
            SetOperatorType::Union => self.perform_union(&left_result.rows, &right_result.rows),
            SetOperatorType::Intersect => self.perform_intersect(&left_result.rows, &right_result.rows),
            SetOperatorType::Except => self.perform_except(&left_result.rows, &right_result.rows),
        };

        context.log(&format!("Set operation result: {} rows", result_rows.len()));

        Ok(QueryResult {
            rows: result_rows,
            column_names: left_result.column_names, // Use left schema for result
        })
    }

    /// Check if two Values are equal
    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (&a.kind, &b.kind) {
            (ValueKind::Null(_), ValueKind::Null(_)) => true,
            (ValueKind::Integer(a_val), ValueKind::Integer(b_val)) => a_val == b_val,
            (ValueKind::Float(a_val), ValueKind::Float(b_val)) => a_val == b_val,
            (ValueKind::String(a_val), ValueKind::String(b_val)) => a_val == b_val,
            (ValueKind::Boolean(a_val), ValueKind::Boolean(b_val)) => a_val == b_val,
            _ => false, // Different types are not equal
        }
    }

    /// Check if two rows are equal
    fn rows_equal(&self, a: &[Value], b: &[Value]) -> bool {
        if a.len() != b.len() {
            return false;
        }

        for (val_a, val_b) in a.iter().zip(b.iter()) {
            if !self.values_equal(val_a, val_b) {
                return false;
            }
        }
        true
    }

    /// Check if a vector of rows contains a specific row
    fn rows_contains(&self, rows: &[Vec<Value>], target: &[Value]) -> bool {
        rows.iter().any(|row| self.rows_equal(row, target))
    }

    /// Count occurrences of a row in a vector of rows
    fn count_row_occurrences(&self, rows: &[Vec<Value>], target: &[Value]) -> usize {
        rows.iter().filter(|row| self.rows_equal(row, target)).count()
    }

    /// Perform UNION operation
    fn perform_union(&self, left_rows: &[Vec<Value>], right_rows: &[Vec<Value>]) -> Vec<Vec<Value>> {
        let mut result = Vec::new();

        // Add all left rows
        result.extend_from_slice(left_rows);

        // Add right rows (considering ALL flag)
        if self.all {
            result.extend_from_slice(right_rows);
        } else {
            // UNION without ALL - remove duplicates
            for right_row in right_rows {
                if !self.rows_contains(&result, right_row) {
                    result.push(right_row.clone());
                }
            }
        }

        result
    }

    /// Perform INTERSECT operation
    fn perform_intersect(&self, left_rows: &[Vec<Value>], right_rows: &[Vec<Value>]) -> Vec<Vec<Value>> {
        let mut result: Vec<Vec<Value>> = Vec::new();

        for left_row in left_rows {
            if self.all {
                // INTERSECT ALL - count occurrences
                let left_count = self.count_row_occurrences(left_rows, left_row);
                let right_count = self.count_row_occurrences(right_rows, left_row);
                let count = left_count.min(right_count);

                // Remove existing occurrences of this row
                result.retain(|r| !self.rows_equal(r, left_row));

                // Add the row count times
                for _ in 0..count {
                    result.push(left_row.clone());
                }
            } else {
                // INTERSECT without ALL - unique intersection
                if self.rows_contains(right_rows, left_row) && !self.rows_contains(&result, left_row) {
                    result.push(left_row.clone());
                }
            }
        }

        result
    }

    /// Perform EXCEPT operation
    fn perform_except(&self, left_rows: &[Vec<Value>], right_rows: &[Vec<Value>]) -> Vec<Vec<Value>> {
        let mut result: Vec<Vec<Value>> = Vec::new();

        for left_row in left_rows {
            if self.all {
                // EXCEPT ALL - subtract occurrences
                let left_count = self.count_row_occurrences(left_rows, left_row);
                let right_count = self.count_row_occurrences(right_rows, left_row);

                if left_count > right_count {
                    let remaining = left_count - right_count;
                    let current_result_count = self.count_row_occurrences(&result, left_row);

                    if current_result_count < remaining {
                        result.push(left_row.clone());
                    }
                }
            } else {
                // EXCEPT without ALL - remove all occurrences
                if !self.rows_contains(right_rows, left_row) && !self.rows_contains(&result, left_row) {
                    result.push(left_row.clone());
                }
            }
        }

        result
    }
}

/// Subquery operator
#[derive(Debug)]
pub struct SubqueryOperator {
    pub query: crate::sql::ast::Statement,
    pub correlated_columns: Vec<String>,
}

impl SubqueryOperator {
    pub fn new(query: crate::sql::ast::Statement, correlated_columns: Vec<String>) -> Self {
        Self {
            query,
            correlated_columns,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Executing subquery: {:?}", self.query));

        // Create a planner that has access to materialized CTEs from the context
        let catalog = context.get_catalog().cloned();
        let materialized_ctes = context.get_materialized_ctes()
            .cloned()
            .unwrap_or_else(|| std::collections::HashMap::new());

        // Always use with_ctes constructor which accepts optional catalog
        let planner = crate::executor::planner::QueryPlanner::with_ctes(catalog, materialized_ctes);

        // Plan and execute the subquery
        let plan = match &self.query {
            crate::sql::ast::Statement::Select(select_stmt) => {
                planner.plan_select(select_stmt)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "Only SELECT statements are supported in subqueries".to_string()
                ));
            }
        };

        // Execute the subquery plan
        plan.root.execute(context)
    }

    /// Check if this is a correlated subquery
    pub fn is_correlated(&self) -> bool {
        !self.correlated_columns.is_empty()
    }

    /// Execute subquery for each row of outer query (for correlated subqueries)
    pub fn execute_correlated(&self, outer_context: &EvaluationContext, context: &mut ExecutionContext) -> Result<QueryResult> {
        if !self.is_correlated() {
            return self.execute(context);
        }

        context.log("Executing correlated subquery");

        // For correlated subqueries, we need to create an enhanced execution context
        // that includes values from the outer query context
        let mut correlated_context = ExecutionContext::new();

        // Copy all logs from the original context
        correlated_context.logs = context.logs.clone();

        // Copy catalog, buffer_manager, and materialized_ctes from parent context so subquery can access tables and CTEs
        if let Some(catalog) = context.get_catalog() {
            correlated_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            correlated_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            correlated_context.set_materialized_ctes(materialized_ctes.clone());
        }

        // Inject outer context values into correlated context
        // The correlated_columns list tells us which outer columns we need
        let mut outer_values = std::collections::HashMap::new();

        for correlated_column in &self.correlated_columns {
            if let Some(value) = outer_context.columns.get(correlated_column) {
                correlated_context.log(&format!("Injecting correlated column {} = {:?}", correlated_column, value));
                outer_values.insert(correlated_column.clone(), value.clone());
            } else {
                // Fallback: if qualified name not found, try to extract unqualified part
                if let Some(dot_pos) = correlated_column.find('.') {
                    let unqualified_name = &correlated_column[dot_pos + 1..];
                    if let Some(value) = outer_context.columns.get(unqualified_name) {
                        correlated_context.log(&format!("Injecting correlated column {} (found as {})", correlated_column, unqualified_name));
                        outer_values.insert(correlated_column.clone(), value.clone());
                    }
                }
            }
        }

        // Store outer context values in the execution context so FilterOperator can access them
        if !outer_values.is_empty() {
            correlated_context.set_outer_context_values(outer_values);
        }

        // Create a planner that has access to materialized CTEs from the context
        let catalog = correlated_context.get_catalog().cloned();
        let materialized_ctes = correlated_context.get_materialized_ctes()
            .cloned()
            .unwrap_or_else(|| std::collections::HashMap::new());

        // Always use with_ctes constructor which accepts optional catalog
        let planner = crate::executor::planner::QueryPlanner::with_ctes(catalog, materialized_ctes);

        // Plan and execute the subquery
        let plan = match &self.query {
            crate::sql::ast::Statement::Select(select_stmt) => {
                planner.plan_select(select_stmt)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "Only SELECT statements are supported in subqueries".to_string()
                ));
            }
        };

        // Execute the subquery plan with correlated context
        // Note: In a full implementation, we'd need to modify the execution engine to use
        // the correlated_context when evaluating column references that match correlated_columns
        let result = plan.root.execute(&mut correlated_context);

        // Copy logs back to the original context
        context.logs.extend(correlated_context.logs);

        result
    }
}

/// Window function operator for SQL window operations
#[derive(Debug)]
pub struct WindowOperator {
    pub input: Box<PlanNode>,
    pub window_functions: Vec<(String, crate::sql::ast::WindowFunction)>,
    pub partition_by: Vec<Expression>,
    pub order_by: Vec<crate::sql::ast::OrderBy>,
    pub window_frame: Option<crate::sql::ast::WindowFrame>,
    pub scanner: Option<TableScanner>, // For column name resolution
}

/// Window function state for incremental computation
#[derive(Debug, Clone)]
pub enum WindowFunctionState {
    RowNumber { count: usize },
    Rank { current_rank: usize, prev_values: Option<Vec<Value>> },
    DenseRank { current_rank: usize, prev_values: Option<Vec<Value>> },
    Lag { buffer: VecDeque<Value> },
    Lead { buffer: VecDeque<Value> },
    Aggregate(AggregateState),
}

use std::collections::VecDeque;

impl WindowOperator {
    pub fn new(
        input: PlanNode,
        window_functions: Vec<(String, crate::sql::ast::WindowFunction)>,
        partition_by: Vec<Expression>,
        order_by: Vec<crate::sql::ast::OrderBy>,
        window_frame: Option<crate::sql::ast::WindowFrame>,
    ) -> Self {
        Self {
            input: Box::new(input),
            window_functions,
            partition_by,
            order_by,
            window_frame,
            scanner: None,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Executing WindowOperator");
        println!("DEBUG: WindowOperator executing with {} window functions", self.window_functions.len());
        for (i, (alias, _)) in self.window_functions.iter().enumerate() {
            println!("DEBUG: Window function {} has alias '{}'", i, alias);
        }

        // Execute input to get source data
        let input_result = self.input.execute(context)?;
        context.log(&format!("WindowOperator received {} rows from input", input_result.rows.len()));
        println!("DEBUG: WindowOperator input columns: {:?}", input_result.column_names);

        if input_result.rows.is_empty() {
            // Return empty result with window function columns
            let mut column_names = input_result.column_names.clone();
            for (alias, _) in &self.window_functions {
                column_names.push(alias.clone());
            }
            return Ok(QueryResult {
                rows: Vec::new(),
                column_names,
            });
        }

        // Sort input data if ORDER BY is specified
        let sorted_rows = self.sort_input_rows(&input_result, context)?;

        // Partition the rows based on PARTITION BY clause
        let partitions = self.partition_rows(&sorted_rows, &input_result.column_names)?;

        // Apply window functions to each partition
        let (result_rows, result_column_names) = self.apply_window_functions(partitions, &input_result.column_names)?;
        println!("DEBUG: WindowOperator producing {} output columns: {:?}", result_column_names.len(), result_column_names);
        for (i, col_name) in result_column_names.iter().enumerate() {
            println!("DEBUG: Output column {}: '{}'", i, col_name);
        }

        context.log(&format!("WindowOperator returning {} rows", result_rows.len()));

        Ok(QueryResult {
            rows: result_rows,
            column_names: result_column_names,
        })
    }

    /// Sort input rows based on ORDER BY clause
    fn sort_input_rows(&self, input_result: &QueryResult, _context: &mut ExecutionContext) -> Result<Vec<Vec<Value>>> {
        let mut rows = input_result.rows.clone();

        if self.order_by.is_empty() {
            // No ORDER BY clause, maintain original order
            return Ok(rows);
        }

        // Note: In a real implementation, we would use the context parameter for logging
        // For now, we'll proceed with the sorting logic

        // Create expression evaluator for sorting
        let evaluator = ExpressionEvaluator;

        rows.sort_by(|a, b| {
            for order_by_expr in &self.order_by {
                // Create evaluation contexts for both rows
                let context_a = self.create_sorting_context(a, &input_result.column_names);
                let context_b = self.create_sorting_context(b, &input_result.column_names);

                // Evaluate the ORDER BY expression for both rows
                let val_a = evaluator.evaluate(&order_by_expr.expr, &context_a);
                let val_b = evaluator.evaluate(&order_by_expr.expr, &context_b);

                match (val_a, val_b) {
                    (Ok(a_val), Ok(b_val)) => {
                        let ordering = compare_values(&a_val, &b_val, order_by_expr.nulls);
                        if ordering != std::cmp::Ordering::Equal {
                            return if order_by_expr.direction == crate::sql::ast::SortDirection::Desc {
                                ordering.reverse()
                            } else {
                                ordering
                            };
                        }
                    }
                    _ => {
                        // Handle evaluation errors by treating as NULL
                        return std::cmp::Ordering::Equal;
                    }
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(rows)
    }

    /// Create evaluation context for sorting
    fn create_sorting_context(&self, row: &[Value], column_names: &[String]) -> EvaluationContext {
        let mut context = EvaluationContext::new();
        for (i, column_name) in column_names.iter().enumerate() {
            if i < row.len() {
                context.columns.insert(column_name.clone(), row[i].clone());
            }
        }
        context
    }

    /// Partition rows based on PARTITION BY clause
    fn partition_rows(&self, rows: &[Vec<Value>], column_names: &[String]) -> Result<Vec<Vec<Vec<Value>>>> {
        if self.partition_by.is_empty() {
            // No partitioning, all rows in one partition
            return Ok(vec![rows.to_vec()]);
        }

        let evaluator = ExpressionEvaluator;
        let mut partitions: HashMap<String, Vec<Vec<Value>>> = HashMap::new();

        for row in rows {
            // Calculate partition key by evaluating all PARTITION BY expressions
            let mut partition_key = String::new();
            for (i, partition_expr) in self.partition_by.iter().enumerate() {
                if i > 0 {
                    partition_key.push('|');
                }

                let context = self.create_sorting_context(row, column_names);
                match evaluator.evaluate(partition_expr, &context) {
                    Ok(value) => {
                        partition_key.push_str(&format!("{:?}", value));
                    }
                    Err(_) => {
                        partition_key.push_str("NULL");
                    }
                }
            }

            partitions
                .entry(partition_key)
                .or_insert_with(Vec::new)
                .push(row.clone());
        }

        let partition_list: Vec<Vec<Vec<Value>>> = partitions.into_values().collect();
        Ok(partition_list)
    }

    /// Apply window functions to each partition
    fn apply_window_functions(&self, partitions: Vec<Vec<Vec<Value>>>, input_column_names: &[String]) -> Result<(Vec<Vec<Value>>, Vec<String>)> {
        let mut result_rows = Vec::new();
        let evaluator = ExpressionEvaluator;

        // Prepare result column names
        let mut result_column_names = input_column_names.to_vec();
        for (alias, _window_func) in self.window_functions.iter() {
            result_column_names.push(alias.clone());
        }

        for partition in partitions {
            // For each window function, we need to process in its specific sorted order
            // Collect all window function results, then combine with original rows
            let num_funcs = self.window_functions.len();
            let num_rows = partition.len();

            // window_results[func_index][original_row_index] = Value
            let mut window_results: Vec<Vec<Value>> = vec![vec![Value { kind: ValueKind::Null(crate::types::NullValue) }; num_rows]; num_funcs];

            for (func_index, (_alias, window_func)) in self.window_functions.iter().enumerate() {
                // Sort partition according to THIS window function's ORDER BY
                // Keep track of original indices so we can map results back
                let mut sorted_with_indices: Vec<(usize, Vec<Value>)> = partition
                    .iter()
                    .enumerate()
                    .map(|(i, row)| (i, row.clone()))
                    .collect();

                // Sort by this window function's ORDER BY clause
                let func_order_by = &window_func.window_clause.order_by;
                if !func_order_by.is_empty() {
                    sorted_with_indices.sort_by(|(_, a), (_, b)| {
                        for order_by_expr in func_order_by {
                            let context_a = self.create_sorting_context(a, input_column_names);
                            let context_b = self.create_sorting_context(b, input_column_names);

                            let val_a = evaluator.evaluate(&order_by_expr.expr, &context_a);
                            let val_b = evaluator.evaluate(&order_by_expr.expr, &context_b);

                            match (val_a, val_b) {
                                (Ok(a_val), Ok(b_val)) => {
                                    let ordering = compare_values(&a_val, &b_val, order_by_expr.nulls);
                                    if ordering != std::cmp::Ordering::Equal {
                                        return if order_by_expr.direction == crate::sql::ast::SortDirection::Desc {
                                            ordering.reverse()
                                        } else {
                                            ordering
                                        };
                                    }
                                }
                                _ => {}
                            }
                        }
                        std::cmp::Ordering::Equal
                    });
                }

                // Now evaluate this window function on the sorted partition
                let sorted_partition: Vec<Vec<Value>> = sorted_with_indices.iter().map(|(_, row)| row.clone()).collect();
                let mut state = self.initialize_window_function(func_index, window_func);

                for (sorted_row_index, (original_index, row)) in sorted_with_indices.iter().enumerate() {
                    let context = self.create_sorting_context(row, input_column_names);
                    let window_value = self.evaluate_window_function_with_order(
                        func_index,
                        window_func,
                        &sorted_partition,
                        sorted_row_index,
                        &mut state,
                        &evaluator,
                        &context,
                        input_column_names,
                    )?;
                    // Store result at original row index
                    window_results[func_index][*original_index] = window_value;
                }
            }

            // Combine original rows with all window function results
            for (row_idx, row) in partition.iter().enumerate() {
                let mut result_row = row.clone();
                for func_results in &window_results {
                    result_row.push(func_results[row_idx].clone());
                }
                result_rows.push(result_row);
            }
        }

        Ok((result_rows, result_column_names))
    }

    /// Initialize window function state
    fn initialize_window_function(&self, func_index: usize, window_func: &crate::sql::ast::WindowFunction) -> WindowFunctionState {
        match window_func.name.to_uppercase().as_str() {
            "ROW_NUMBER" => WindowFunctionState::RowNumber { count: 0 },
            "RANK" => WindowFunctionState::Rank { current_rank: 0, prev_values: None },
            "DENSE_RANK" => WindowFunctionState::DenseRank { current_rank: 0, prev_values: None },
            "LAG" => WindowFunctionState::Lag { buffer: VecDeque::new() },
            "LEAD" => WindowFunctionState::Lead { buffer: VecDeque::new() },
            "COUNT" => WindowFunctionState::Aggregate(AggregateState::new_count(false)),
            "SUM" => WindowFunctionState::Aggregate(AggregateState::new_sum(false)),
            "AVG" => WindowFunctionState::Aggregate(AggregateState::new_avg(false)),
            "MIN" => WindowFunctionState::Aggregate(AggregateState::new_min()),
            "MAX" => WindowFunctionState::Aggregate(AggregateState::new_max()),
            _ => WindowFunctionState::RowNumber { count: 0 }, // Default to ROW_NUMBER for unknown functions
        }
    }

    /// Evaluate a window function for a specific row
    fn evaluate_window_function(
        &self,
        func_index: usize,
        window_func: &crate::sql::ast::WindowFunction,
        partition: &[Vec<Value>],
        current_row_index: usize,
        state: &mut WindowFunctionState,
        evaluator: &ExpressionEvaluator,
        context: &EvaluationContext,
        input_column_names: &[String],
    ) -> Result<Value> {
        match window_func.name.to_uppercase().as_str() {
            "ROW_NUMBER" => {
                if let WindowFunctionState::RowNumber { count } = state {
                    *count += 1;
                    Ok(Value { kind: ValueKind::Integer(*count as i64) })
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for ROW_NUMBER".to_string()))
                }
            }
            "RANK" => {
                if let WindowFunctionState::Rank { current_rank, prev_values } = state {
                    let current_values = self.get_order_by_values(partition, current_row_index, evaluator, context)?;

                    if let Some(ref prev_vals) = prev_values {
                        if self.values_equal(&current_values, prev_vals) {
                            // Same values as previous row, same rank
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        } else {
                            // Different values, rank = number of previous rows + 1
                            *current_rank = current_row_index + 1;
                            *prev_values = Some(current_values);
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        }
                    } else {
                        // First row
                        *current_rank = 1;
                        *prev_values = Some(current_values);
                        Ok(Value { kind: ValueKind::Integer(1) })
                    }
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for RANK".to_string()))
                }
            }
            "DENSE_RANK" => {
                if let WindowFunctionState::DenseRank { current_rank, prev_values } = state {
                    let current_values = self.get_order_by_values(partition, current_row_index, evaluator, context)?;

                    if let Some(ref prev_vals) = prev_values {
                        if self.values_equal(&current_values, prev_vals) {
                            // Same values as previous row, same rank
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        } else {
                            // Different values, increment rank
                            *current_rank += 1;
                            *prev_values = Some(current_values);
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        }
                    } else {
                        // First row
                        *current_rank = 1;
                        *prev_values = Some(current_values);
                        Ok(Value { kind: ValueKind::Integer(1) })
                    }
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for DENSE_RANK".to_string()))
                }
            }
            "LAG" => {
                // LAG can have 1-2 arguments: LAG(expr) or LAG(expr, offset)
                if window_func.args.len() < 1 || window_func.args.len() > 2 {
                    return Err(crate::error::RustgreSQLError::InvalidOperation("LAG function requires 1 or 2 arguments".to_string()));
                }

                // Get offset (default to 1 if not specified)
                let offset = if window_func.args.len() == 2 {
                    match evaluator.evaluate(&window_func.args[1], context)? {
                        Value { kind: ValueKind::Integer(offset) } => {
                            if offset <= 0 {
                                return Err(crate::error::RustgreSQLError::InvalidOperation("LAG offset must be positive".to_string()));
                            }
                            offset as usize
                        }
                        _ => {
                            return Err(crate::error::RustgreSQLError::InvalidOperation("LAG offset must be an integer".to_string()));
                        }
                    }
                } else {
                    1
                };

                // Access the buffer mutably to read and add values
                if let WindowFunctionState::Lag { buffer } = state {
                    // Retrieve value based on offset BEFORE adding current value
                    let result = if current_row_index < offset {
                        // Not enough previous rows
                        Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                    } else {
                        // Look back 'offset' rows: buffer[0] is row 0, buffer[1] is row 1, etc.
                        // buffer[current_row_index - offset] gives us the value from 'offset' rows back
                        let target_index = current_row_index - offset;
                        if let Some(target_value) = buffer.iter().nth(target_index) {
                            Ok(target_value.clone())
                        } else {
                            Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                        }
                    };

                    // Evaluate the expression for the current row AFTER reading from buffer
                    let current_value = evaluator.evaluate(&window_func.args[0], context)?;

                    // Add current value to the buffer for future LAG calls
                    buffer.push_back(current_value);

                    result
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for LAG".to_string()))
                }
            }
            "LEAD" => {
                // LEAD can have 1-2 arguments: LEAD(expr) or LEAD(expr, offset)
                if window_func.args.len() < 1 || window_func.args.len() > 2 {
                    return Err(crate::error::RustgreSQLError::InvalidOperation("LEAD function requires 1 or 2 arguments".to_string()));
                }

                // Get offset (default to 1 if not specified)
                let offset = if window_func.args.len() == 2 {
                    match evaluator.evaluate(&window_func.args[1], context)? {
                        Value { kind: ValueKind::Integer(offset) } => {
                            if offset <= 0 {
                                return Err(crate::error::RustgreSQLError::InvalidOperation("LEAD offset must be positive".to_string()));
                            }
                            offset as usize
                        }
                        _ => {
                            return Err(crate::error::RustgreSQLError::InvalidOperation("LEAD offset must be an integer".to_string()));
                        }
                    }
                } else {
                    1
                };

                // For LEAD, we can look forward in the partition directly
                // since we have all rows in the partition
                let target_row_index = current_row_index + offset;

                if target_row_index >= partition.len() {
                    // Beyond the end of partition
                    Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                } else {
                    // Evaluate the expression for the target (future) row
                    let target_row = &partition[target_row_index];
                    let target_context = self.create_sorting_context(target_row, input_column_names);
                    let target_value = evaluator.evaluate(&window_func.args[0], &target_context)?;
                    Ok(target_value)
                }
            }
            "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" => {
                if let WindowFunctionState::Aggregate(agg_state) = state {
                    // For aggregate window functions, compute the aggregate over the window frame
                    // Create a fresh aggregate state for each row to compute frame-specific results
                    let mut frame_agg_state = self.create_fresh_aggregate_state(&window_func.name)?;

                    // Calculate frame boundaries for current row
                    let (frame_start, frame_end) = self.calculate_frame_boundaries(
                        partition.len(),
                        current_row_index,
                        window_func,
                    )?;

                    // Iterate through rows in the frame and update the aggregate state
                    for frame_row_index in frame_start..=frame_end {
                        if frame_row_index < partition.len() {
                            let row = &partition[frame_row_index];

                            // Create context for this row
                            let mut row_context = EvaluationContext::new();
                            for (i, column_name) in input_column_names.iter().enumerate() {
                                if i < row.len() {
                                    row_context.columns.insert(column_name.clone(), row[i].clone());
                                }
                            }

                            // Evaluate the aggregate argument for this row
                            let arg_value = if window_func.args.is_empty() {
                                // COUNT(*) case
                                Value { kind: ValueKind::Integer(1) }
                            } else if matches!(&window_func.args[0], Expression::Star) {
                                // COUNT(*) case
                                Value { kind: ValueKind::Integer(1) }
                            } else {
                                evaluator.evaluate(&window_func.args[0], &row_context)?
                            };

                            // Update the frame aggregate state
                            frame_agg_state.update(&arg_value)?;
                        }
                    }

                    // Return the frame-specific aggregate result
                    frame_agg_state.result()
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for aggregate".to_string()))
                }
            }
            _ => {
                // Unknown window function, return NULL
                Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
            }
        }
    }

    /// Evaluate a window function using the function's own ORDER BY clause
    /// This is used when each window function may have different ORDER BY
    fn evaluate_window_function_with_order(
        &self,
        func_index: usize,
        window_func: &crate::sql::ast::WindowFunction,
        partition: &[Vec<Value>],
        current_row_index: usize,
        state: &mut WindowFunctionState,
        evaluator: &ExpressionEvaluator,
        context: &EvaluationContext,
        input_column_names: &[String],
    ) -> Result<Value> {
        match window_func.name.to_uppercase().as_str() {
            "ROW_NUMBER" => {
                if let WindowFunctionState::RowNumber { count } = state {
                    *count += 1;
                    Ok(Value { kind: ValueKind::Integer(*count as i64) })
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for ROW_NUMBER".to_string()))
                }
            }
            "RANK" => {
                if let WindowFunctionState::Rank { current_rank, prev_values } = state {
                    // Use the window function's own ORDER BY, not self.order_by
                    let current_values = self.get_order_by_values_for_func(
                        partition,
                        current_row_index,
                        evaluator,
                        &window_func.window_clause.order_by,
                        input_column_names,
                    )?;

                    if let Some(ref prev_vals) = prev_values {
                        if self.values_equal(&current_values, prev_vals) {
                            // Same values as previous row, same rank
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        } else {
                            // Different values, rank = number of previous rows + 1
                            *current_rank = current_row_index + 1;
                            *prev_values = Some(current_values);
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        }
                    } else {
                        // First row
                        *current_rank = 1;
                        *prev_values = Some(current_values);
                        Ok(Value { kind: ValueKind::Integer(1) })
                    }
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for RANK".to_string()))
                }
            }
            "DENSE_RANK" => {
                if let WindowFunctionState::DenseRank { current_rank, prev_values } = state {
                    // Use the window function's own ORDER BY, not self.order_by
                    let current_values = self.get_order_by_values_for_func(
                        partition,
                        current_row_index,
                        evaluator,
                        &window_func.window_clause.order_by,
                        input_column_names,
                    )?;

                    if let Some(ref prev_vals) = prev_values {
                        if self.values_equal(&current_values, prev_vals) {
                            // Same values as previous row, same rank
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        } else {
                            // Different values, increment rank
                            *current_rank += 1;
                            *prev_values = Some(current_values);
                            Ok(Value { kind: ValueKind::Integer(*current_rank as i64) })
                        }
                    } else {
                        // First row
                        *current_rank = 1;
                        *prev_values = Some(current_values);
                        Ok(Value { kind: ValueKind::Integer(1) })
                    }
                } else {
                    Err(crate::error::RustgreSQLError::InvalidOperation("Invalid window function state for DENSE_RANK".to_string()))
                }
            }
            // For other window functions, delegate to the original method
            _ => self.evaluate_window_function(
                func_index,
                window_func,
                partition,
                current_row_index,
                state,
                evaluator,
                context,
                input_column_names,
            ),
        }
    }

    /// Get ORDER BY values using a specific ORDER BY clause (not self.order_by)
    fn get_order_by_values_for_func(
        &self,
        partition: &[Vec<Value>],
        row_index: usize,
        evaluator: &ExpressionEvaluator,
        order_by: &[crate::sql::ast::OrderBy],
        input_column_names: &[String],
    ) -> Result<Vec<Value>> {
        let mut values = Vec::new();
        let row = &partition[row_index];

        // Create temporary context with row data
        let mut temp_context = EvaluationContext::new();
        for (i, column_name) in input_column_names.iter().enumerate() {
            if i < row.len() {
                temp_context.columns.insert(column_name.clone(), row[i].clone());
            }
        }

        for order_by_expr in order_by {
            let value = evaluator.evaluate(&order_by_expr.expr, &temp_context)?;
            values.push(value);
        }

        Ok(values)
    }

    /// Get ORDER BY values for a specific row
    fn get_order_by_values(&self, partition: &[Vec<Value>], row_index: usize, evaluator: &ExpressionEvaluator, context: &EvaluationContext) -> Result<Vec<Value>> {
        let mut values = Vec::new();
        let row = &partition[row_index];

        // Create temporary context with row data
        let mut temp_context = EvaluationContext::new();
        for (i, column_name) in context.columns.keys().enumerate() {
            if i < row.len() {
                temp_context.columns.insert(column_name.clone(), row[i].clone());
            }
        }

        for order_by_expr in &self.order_by {
            let value = evaluator.evaluate(&order_by_expr.expr, &temp_context)?;
            values.push(value);
        }

        Ok(values)
    }

    /// Check if two value vectors are equal (for rank/dense_rank)
    fn values_equal(&self, a: &[Value], b: &[Value]) -> bool {
        if a.len() != b.len() {
            return false;
        }

        for (val_a, val_b) in a.iter().zip(b.iter()) {
            match compare_values(val_a, val_b, NullsPosition::Default) {
                std::cmp::Ordering::Equal => continue,
                _ => return false,
            }
        }

        true
    }

    /// Create a fresh aggregate state for the specified window function
    fn create_fresh_aggregate_state(&self, func_name: &str) -> Result<AggregateState> {
        match func_name.to_uppercase().as_str() {
            "COUNT" => Ok(AggregateState::new_count(false)),
            "SUM" => Ok(AggregateState::new_sum(false)),
            "AVG" => Ok(AggregateState::new_avg(false)),
            "MIN" => Ok(AggregateState::new_min()),
            "MAX" => Ok(AggregateState::new_max()),
            _ => Err(crate::error::RustgreSQLError::InvalidOperation(
                format!("Unknown aggregate function for window: {}", func_name)
            ))
        }
    }

    /// Calculate frame boundaries for a given row and window frame specification
    fn calculate_frame_boundaries(
        &self,
        partition_size: usize,
        current_row_index: usize,
        window_func: &crate::sql::ast::WindowFunction,
    ) -> Result<(usize, usize)> {
        let mut frame_start = 0;
        let mut frame_end = current_row_index;

        // If no window frame is specified:
        // - If there's an ORDER BY, default to RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        // - If there's no ORDER BY, default to RANGE BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING (entire partition)
        if let Some(ref frame) = window_func.window_clause.window_frame {
            match frame.mode {
                crate::sql::ast::WindowFrameMode::Rows => {
                    // For ROWS mode, we use row indices
                    frame_start = match frame.start {
                        crate::sql::ast::WindowFrameBound::CurrentRow => current_row_index,
                        crate::sql::ast::WindowFrameBound::UnboundedPreceding => 0,
                        crate::sql::ast::WindowFrameBound::UnboundedFollowing => {
                            return Err(crate::error::RustgreSQLError::InvalidOperation(
                                "Unbounded following not supported as frame start".to_string()
                            ));
                        }
                        crate::sql::ast::WindowFrameBound::Preceding(ref expr) => {
                            // Calculate rows to go back
                            if let Ok(Value { kind: crate::types::ValueKind::Integer(n) }) =
                                self.evaluate_constant_expression(expr) {
                                let preceding_rows = n as usize;
                                if current_row_index >= preceding_rows {
                                    current_row_index - preceding_rows
                                } else {
                                    0
                                }
                            } else {
                                return Err(crate::error::RustgreSQLError::InvalidOperation(
                                    "Only constant integer expressions supported for frame bounds".to_string()
                                ));
                            }
                        }
                        crate::sql::ast::WindowFrameBound::Following(ref expr) => {
                            return Err(crate::error::RustgreSQLError::InvalidOperation(
                                "Following not supported as frame start".to_string()
                            ));
                        }
                    };

                    frame_end = match frame.end {
                        Some(crate::sql::ast::WindowFrameBound::CurrentRow) => current_row_index,
                        Some(crate::sql::ast::WindowFrameBound::UnboundedPreceding) => {
                            return Err(crate::error::RustgreSQLError::InvalidOperation(
                                "Unbounded preceding not supported as frame end".to_string()
                            ));
                        }
                        Some(crate::sql::ast::WindowFrameBound::UnboundedFollowing) => partition_size - 1,
                        Some(crate::sql::ast::WindowFrameBound::Preceding(ref expr)) => {
                            if let Ok(Value { kind: crate::types::ValueKind::Integer(n) }) =
                                self.evaluate_constant_expression(expr) {
                                let preceding_rows = n as usize;
                                if current_row_index >= preceding_rows {
                                    current_row_index - preceding_rows
                                } else {
                                    0
                                }
                            } else {
                                return Err(crate::error::RustgreSQLError::InvalidOperation(
                                    "Only constant integer expressions supported for frame bounds".to_string()
                                ));
                            }
                        }
                        Some(crate::sql::ast::WindowFrameBound::Following(ref expr)) => {
                            if let Ok(Value { kind: crate::types::ValueKind::Integer(n) }) =
                                self.evaluate_constant_expression(expr) {
                                let following_rows = n as usize;
                                std::cmp::min(current_row_index + following_rows, partition_size - 1)
                            } else {
                                return Err(crate::error::RustgreSQLError::InvalidOperation(
                                    "Only constant integer expressions supported for frame bounds".to_string()
                                ));
                            }
                        }
                        None => current_row_index,
                    };
                }
                crate::sql::ast::WindowFrameMode::Range => {
                    // For RANGE mode, we'd need to group by ORDER BY values
                    // This is more complex and we'll start with ROWS mode support
                    return Err(crate::error::RustgreSQLError::InvalidOperation(
                        "RANGE mode not yet implemented for window frames".to_string()
                    ));
                }
            }
        } else {
            // No explicit window frame specified
            if window_func.window_clause.order_by.is_empty() {
                // No ORDER BY: aggregate over entire partition (unbounded preceding to unbounded following)
                frame_end = partition_size - 1;
            } else {
                // Has ORDER BY but no frame: default to unbounded preceding to current row
                // (this is already set by the initialization above)
            }
        }

        // Ensure frame boundaries are within partition bounds
        frame_start = std::cmp::max(frame_start, 0);
        frame_end = std::cmp::min(frame_end, partition_size - 1);

        Ok((frame_start, frame_end))
    }

    /// Evaluate a constant expression (used for frame bounds)
    fn evaluate_constant_expression(&self, expr: &crate::sql::ast::Expression) -> Result<Value> {
        match expr {
            crate::sql::ast::Expression::Value(value) => Ok(value.clone()),
            _ => Err(crate::error::RustgreSQLError::InvalidOperation(
                "Only constant values supported for frame bounds".to_string()
            ))
        }
    }
}

/// CTE Scan operator
///
/// This operator scans materialized CTE results as if they were regular tables
#[derive(Debug)]
pub struct CTEScanOperator {
    pub cte_name: String,
}

impl CTEScanOperator {
    pub fn new(cte_name: String) -> Self {
        Self {
            cte_name,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        // Look up the CTE from the execution context
        if let Some(materialized_ctes) = &context.materialized_ctes {
            if let Some(cte_result) = materialized_ctes.get(&self.cte_name) {
                Ok(cte_result.clone())
            } else {
                Err(crate::error::RustgreSQLError::Execution(
                    format!("CTE '{}' not found in execution context", self.cte_name)
                ))
            }
        } else {
            Err(crate::error::RustgreSQLError::Execution(
                "No materialized CTEs in execution context".to_string()
            ))
        }
    }
}

/// CTE Dependency Graph for managing multiple recursive CTEs
#[derive(Debug, Clone)]
pub struct CTEDependencyGraph {
    pub nodes: std::collections::HashMap<String, CTENode>,
    pub execution_order: Vec<Vec<String>>, // Groups of CTEs that can be executed together
}

#[derive(Debug, Clone)]
pub struct CTENode {
    pub name: String,
    pub is_recursive: bool,
    pub dependencies: std::collections::HashSet<String>,
    pub execution_group: Option<usize>,
}

impl CTEDependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: std::collections::HashMap::new(),
            execution_order: Vec::new(),
        }
    }

    pub fn add_cte(&mut self, cte: &crate::sql::ast::CommonTableExpression) {
        let node = CTENode {
            name: cte.name.clone(),
            is_recursive: cte.recursive,
            dependencies: self.extract_dependencies(&cte.query),
            execution_group: None,
        };
        self.nodes.insert(cte.name.clone(), node);
    }

    pub fn build_execution_order(&mut self) -> Result<()> {
        // Group CTEs by dependency level
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut processed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut remaining: std::collections::HashSet<String> = self.nodes.keys().cloned().collect();

        // First, handle all non-recursive CTEs
        let mut non_recursive_group = Vec::new();
        let mut recursive_ctes = Vec::new();

        for name in &remaining.clone() {
            if let Some(node) = self.nodes.get(name) {
                if !node.is_recursive {
                    non_recursive_group.push(name.clone());
                } else {
                    recursive_ctes.push(name.clone());
                }
            }
        }

        // Execute non-recursive CTEs first (they can depend on each other)
        if !non_recursive_group.is_empty() {
            groups.push(non_recursive_group);
            for name in groups[0].iter() {
                processed.insert(name.clone());
                remaining.remove(name);
            }
        }

        // Handle recursive CTEs - they must be executed one at a time in dependency order
        while !remaining.is_empty() {
            let mut current_group = Vec::new();
            let mut to_remove = Vec::new();

            for name in &remaining.clone() {
                if let Some(node) = self.nodes.get(name) {
                    // Check if all dependencies are processed
                    let deps_processed = node.dependencies.iter().all(|dep| processed.contains(dep));

                    if deps_processed {
                        current_group.push(name.clone());
                        to_remove.push(name.clone());
                    }
                }
            }

            if current_group.is_empty() {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "Circular dependency detected in recursive CTEs".to_string()
                ));
            }

            // For recursive CTEs, execute them individually to avoid mutual recursion
            for name in current_group {
                groups.push(vec![name.clone()]);
                processed.insert(name.clone());
                remaining.remove(&name);
            }
        }

        self.execution_order = groups;
        Ok(())
    }

    fn extract_dependencies(&self, query: &crate::sql::ast::SelectStatement) -> std::collections::HashSet<String> {
        let mut dependencies = std::collections::HashSet::new();
        self.extract_query_dependencies(query, &mut dependencies);
        dependencies
    }

    fn extract_query_dependencies(&self, query: &crate::sql::ast::SelectStatement, deps: &mut std::collections::HashSet<String>) {
        match query {
            crate::sql::ast::SelectStatement::Simple { from, joins, .. } => {
                // Extract table references from FROM clause
                for table_ref in from {
                    self.extract_table_dependencies(table_ref, deps);
                }

                // Extract table references from JOINs
                for join in joins {
                    self.extract_table_dependencies(&join.table, deps);
                }
            }
            crate::sql::ast::SelectStatement::SetOperation(set_op) => {
                self.extract_query_dependencies(&set_op.left, deps);
                self.extract_query_dependencies(&set_op.right, deps);
            }
        }
    }

    fn extract_table_dependencies(&self, table_ref: &crate::sql::ast::TableRef, deps: &mut std::collections::HashSet<String>) {
        match table_ref {
            crate::sql::ast::TableRef::Table { name, .. } => {
                if self.nodes.contains_key(name) {
                    deps.insert(name.clone());
                }
            }
            crate::sql::ast::TableRef::Subquery { subquery, .. } => {
                if let crate::sql::ast::Statement::Select(select) = subquery.as_ref() {
                    self.extract_query_dependencies(select, deps);
                }
            }
        }
    }
}

/// CTE (Common Table Expression) operator
///
/// This operator handles the execution of Common Table Expressions by materializing
/// the CTE results and making them available to the main query. It supports both
/// recursive and non-recursive CTEs, including multiple recursive CTEs with proper
/// dependency management.
#[derive(Debug)]
pub struct CTEOperator {
    /// The WITH clause containing all CTEs
    pub with_clause: crate::sql::ast::WithClause,
    /// The main query that references the CTEs
    pub main_query: Box<crate::sql::ast::Statement>,
    /// Planner for creating execution plans
    pub planner: crate::executor::planner::QueryPlanner,
    /// Materialized CTE results (cte_name -> QueryResult)
    pub materialized_ctes: std::collections::HashMap<String, QueryResult>,
}

impl CTEOperator {
    pub fn new(with_clause: crate::sql::ast::WithClause, main_query: crate::sql::ast::Statement, catalog: std::sync::Arc<crate::catalog::CatalogManager>) -> Self {
        Self {
            with_clause,
            main_query: Box::new(main_query),
            planner: crate::executor::planner::QueryPlanner::with_catalog(catalog),
            materialized_ctes: std::collections::HashMap::new(),
        }
    }

    /// Execute the CTE operator
    ///
    /// This method:
    /// 1. Materializes all CTEs in order
    /// 2. Handles recursive CTEs if present
    /// 3. Executes the main query with access to materialized CTEs
    pub fn execute(&mut self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Executing CTE operator with {} CTEs", self.with_clause.ctes.len()));

        if self.with_clause.recursive {
            self.execute_recursive_ctes(context)
        } else {
            self.execute_non_recursive_ctes(context)
        }
    }

    /// Execute non-recursive CTEs
    fn execute_non_recursive_ctes(&mut self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Executing non-recursive CTEs");

        // Clear any previous CTE results
        self.materialized_ctes.clear();

        // Materialize each CTE
        for cte in &self.with_clause.ctes {
            context.log(&format!("Materializing CTE: {}", cte.name));

            // For materializing CTEs, we need to use a planner that has access to previously materialized CTEs
            let catalog = self.planner.catalog.clone();
            let cte_planner = crate::executor::planner::QueryPlanner::with_ctes(
                catalog,
                self.materialized_ctes.clone()
            );

            let cte_plan = cte_planner.plan_select(&cte.query)?;
            context.log(&format!("Created execution plan for CTE {}", cte.name));

            // Set the materialized CTEs in the context so subqueries can access them
            context.set_materialized_ctes(self.materialized_ctes.clone());

            let cte_result = cte_plan.root.execute(context)?;

            // Store the materialized result for use by the main query
            context.log(&format!("Materialized CTE {} with {} rows", cte.name, cte_result.rows.len()));
            self.materialized_ctes.insert(cte.name.clone(), cte_result);
        }

        // Execute the main query with access to materialized CTEs
        context.log("Executing main query with CTEs");

        // Set all materialized CTEs in the context for the main query
        context.set_materialized_ctes(self.materialized_ctes.clone());

        let main_plan = match self.main_query.as_ref() {
            crate::sql::ast::Statement::Select(select_stmt) => {
                self.plan_select_with_ctes(select_stmt, context)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "CTE operator only supports SELECT statements as main query".to_string()
                ));
            }
        };

        main_plan.root.execute(context)
    }

    /// Execute multiple recursive CTEs with proper dependency management
    fn execute_recursive_ctes(&mut self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Executing multiple recursive CTEs with dependency management");

        // Clear any previous CTE results
        self.materialized_ctes.clear();

        // Build dependency graph for all CTEs
        let mut dependency_graph = CTEDependencyGraph::new();
        for cte in &self.with_clause.ctes {
            dependency_graph.add_cte(cte);
        }

        // Build execution order based on dependencies
        dependency_graph.build_execution_order()?;
        context.log(&format!("Built execution order with {} groups", dependency_graph.execution_order.len()));

        // Execute CTEs in dependency order
        for (group_index, group) in dependency_graph.execution_order.iter().enumerate() {
            context.log(&format!("Executing group {} with {} CTEs", group_index, group.len()));

            if group.len() == 1 {
                let cte_name = &group[0];
                if let Some(cte_node) = dependency_graph.nodes.get(cte_name) {
                    if cte_node.is_recursive {
                        // Execute single recursive CTE
                        self.execute_single_recursive_cte(cte_name, context)?;
                    } else {
                        // Execute non-recursive CTE
                        self.execute_single_non_recursive_cte(cte_name, context)?;
                    }
                }
            } else {
                // Execute multiple non-recursive CTEs in parallel (they have no circular dependencies)
                self.execute_non_recursive_cte_group(group, context)?;
            }
        }

        // Execute the main query
        context.log("Executing main query with all recursive CTEs materialized");

        // Set the materialized CTEs in the context for the main query
        context.set_materialized_ctes(self.materialized_ctes.clone());

        // Create a CTE-aware planner for the main query
        let catalog = self.planner.catalog.clone();
        let main_planner = crate::executor::planner::QueryPlanner::with_ctes(
            catalog,
            self.materialized_ctes.clone()
        );

        let main_plan = match self.main_query.as_ref() {
            crate::sql::ast::Statement::Select(select_stmt) => {
                main_planner.plan_select(select_stmt)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "CTE operator only supports SELECT statements as main query".to_string()
                ));
            }
        };

        main_plan.root.execute(context)
    }

    /// Execute a single recursive CTE with the existing recursive logic
    fn execute_single_recursive_cte(&mut self, cte_name: &str, context: &mut ExecutionContext) -> Result<()> {
        let cte = self.with_clause.ctes.iter()
            .find(|c| c.name == cte_name)
            .ok_or_else(|| crate::error::RustgreSQLError::Internal(
                format!("CTE '{}' not found in WITH clause", cte_name)
            ))?;

        context.log(&format!("Executing recursive CTE: {}", cte_name));

        // Extract anchor and recursive members from the CTE query
        let (anchor_query, recursive_query, is_union_all) = match cte.query.as_ref() {
            crate::sql::ast::SelectStatement::SetOperation(set_op) if
                matches!(set_op.operator, crate::sql::ast::SetOperator::Union) => {
                // This is a UNION query - extract left (anchor) and right (recursive) parts
                (&*set_op.left, &*set_op.right, set_op.all)
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    format!("Recursive CTE '{}' must be a UNION or UNION ALL of anchor and recursive parts", cte_name)
                ));
            }
        };

        context.log(&format!("Executing anchor member of recursive CTE: {}", cte_name));

        // Set the materialized CTEs in the context (includes previously materialized CTEs)
        context.set_materialized_ctes(self.materialized_ctes.clone());

        // Execute the anchor member (non-recursive part)
        // Create a planner that has access to previously materialized CTEs
        let catalog = self.planner.catalog.clone();
        let anchor_planner = crate::executor::planner::QueryPlanner::with_ctes(
            catalog,
            self.materialized_ctes.clone()
        );
        let anchor_plan = anchor_planner.plan_select(anchor_query)?;
        let anchor_result = anchor_plan.root.execute(context)?;
        context.log(&format!("Anchor member of {} produced {} rows", cte_name, anchor_result.rows.len()));

        // Initialize working set with anchor results
        let mut working_result = anchor_result.clone();
        let mut all_results = anchor_result.clone();
        let mut iteration_count = 0;
        let max_iterations = 1000; // Prevent infinite loops

        // Iteratively execute the recursive part until no new rows are produced
        while !working_result.rows.is_empty() && iteration_count < max_iterations {
            iteration_count += 1;

            context.log(&format!("Recursive iteration {} for {} - working set has {} rows",
                iteration_count, cte_name, working_result.rows.len()));

            // Update the materialized CTEs map with the current working result (delta only)
            // This makes the delta from the previous iteration available for recursive queries
            let mut updated_materialized_ctes = self.materialized_ctes.clone();
            updated_materialized_ctes.insert(cte_name.to_string(), working_result.clone());
            context.set_materialized_ctes(updated_materialized_ctes.clone());

            // Execute the actual recursive query
            context.log(&format!("Executing recursive query for {} - iteration {}", cte_name, iteration_count));
            // Create a planner that has access to the updated materialized CTEs (including the CTE being executed)
            let catalog = self.planner.catalog.clone();
            let recursive_planner = crate::executor::planner::QueryPlanner::with_ctes(
                catalog,
                updated_materialized_ctes
            );
            let recursive_plan = recursive_planner.plan_select(recursive_query)?;
            let recursive_result = recursive_plan.root.execute(context)?;

            context.log(&format!("Recursive query for {} produced {} new rows", cte_name, recursive_result.rows.len()));

            // Break if no new rows were produced (convergence)
            if recursive_result.rows.is_empty() {
                context.log(&format!("Recursive CTE {} converged after {} iterations", cte_name, iteration_count));
                break;
            }

            // Create a new working result containing ONLY the delta (new rows from this iteration)
            // This ensures the next iteration only sees new rows, not all accumulated rows
            let mut new_working_result = QueryResult {
                column_names: working_result.column_names.clone(),
                rows: vec![],
            };

            // Add new rows to results (cycle detection only for UNION, not UNION ALL)
            for row in recursive_result.rows {
                if is_union_all {
                    // UNION ALL: Allow all rows, including duplicates
                    new_working_result.rows.push(row.clone());
                    all_results.rows.push(row);
                } else {
                    // UNION: Prevent duplicate rows
                    if !self.row_exists_in_results(&all_results.rows, &row) {
                        new_working_result.rows.push(row.clone());
                        all_results.rows.push(row);
                    }
                }
            }

            // Replace working_result with only the new rows (delta)
            working_result = new_working_result;
        }

        context.log(&format!("Recursive CTE {} execution completed after {} iterations with {} total rows",
            cte_name, iteration_count, all_results.rows.len()));

        // Store the final result in the materialized CTEs map
        self.materialized_ctes.insert(cte_name.to_string(), all_results.clone());

        // Also update the context with the final result so the main query can access it
        context.set_materialized_ctes(self.materialized_ctes.clone());

        Ok(())
    }

    /// Execute a single non-recursive CTE
    fn execute_single_non_recursive_cte(&mut self, cte_name: &str, context: &mut ExecutionContext) -> Result<()> {
        let cte = self.with_clause.ctes.iter()
            .find(|c| c.name == cte_name)
            .ok_or_else(|| crate::error::RustgreSQLError::Internal(
                format!("CTE '{}' not found in WITH clause", cte_name)
            ))?;

        context.log(&format!("Materializing non-recursive CTE: {}", cte_name));

        // Set the materialized CTEs in the context so subqueries can access them
        context.set_materialized_ctes(self.materialized_ctes.clone());

        // Create a planner that has access to previously materialized CTEs
        let catalog = self.planner.catalog.clone();
        let cte_planner = crate::executor::planner::QueryPlanner::with_ctes(
            catalog,
            self.materialized_ctes.clone()
        );

        let cte_plan = cte_planner.plan_select(&cte.query)?;
        context.log(&format!("Created execution plan for CTE {}", cte_name));

        let cte_result = cte_plan.root.execute(context)?;

        // Store the materialized result for use by other CTEs
        context.log(&format!("Materialized CTE {} with {} rows", cte_name, cte_result.rows.len()));
        self.materialized_ctes.insert(cte_name.to_string(), cte_result);

        Ok(())
    }

    /// Execute a group of non-recursive CTEs (they can be executed in any order)
    fn execute_non_recursive_cte_group(&mut self, cte_names: &[String], context: &mut ExecutionContext) -> Result<()> {
        for cte_name in cte_names {
            self.execute_single_non_recursive_cte(cte_name, context)?;
        }
        Ok(())
    }

    /// Check if a row already exists in the results (for cycle detection)
    fn row_exists_in_results(&self, existing_rows: &[Vec<Value>], new_row: &[Value]) -> bool {
        existing_rows.iter().any(|row| self.rows_equal(row, new_row))
    }

    /// Check if two rows are equal (simple value comparison)
    fn rows_equal(&self, row_a: &[Value], row_b: &[Value]) -> bool {
        if row_a.len() != row_b.len() {
            return false;
        }

        for (val_a, val_b) in row_a.iter().zip(row_b.iter()) {
            if !self.values_equal(val_a, val_b) {
                return false;
            }
        }
        true
    }

    /// Check if two values are equal
    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (&a.kind, &b.kind) {
            (ValueKind::Null(_), ValueKind::Null(_)) => true,
            (ValueKind::Integer(a), ValueKind::Integer(b)) => a == b,
            (ValueKind::Float(a), ValueKind::Float(b)) => a == b,
            (ValueKind::String(a), ValueKind::String(b)) => a == b,
            (ValueKind::Boolean(a), ValueKind::Boolean(b)) => a == b,
            _ => false,
        }
    }

    /// Get a reference to the materialized CTE results
    /// This would be used by other operators that need to access CTE data
    pub fn get_materialized_cte(&self, cte_name: &str) -> Option<&QueryResult> {
        self.materialized_ctes.get(cte_name)
    }

    /// Plan a SELECT statement with access to materialized CTEs
    fn plan_select_with_ctes(&self, select: &crate::sql::ast::SelectStatement, context: &mut ExecutionContext) -> Result<crate::executor::planner::ExecutionPlan> {
        use crate::sql::ast::SelectStatement;

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
                distinct,
                offset: _,
                named_windows: _,
            } => {
                // Use the CTE-aware planner with access to materialized CTEs
                let catalog = self.planner.catalog.clone();
                context.log(&format!("Creating CTE-aware planner with {} materialized CTEs", self.materialized_ctes.len()));
                for (cte_name, cte_result) in &self.materialized_ctes {
                    context.log(&format!("  CTE '{}' has {} rows", cte_name, cte_result.rows.len()));
                }

                let planner = crate::executor::planner::QueryPlanner::with_ctes(
                    catalog,
                    self.materialized_ctes.clone()
                );

                // The planner will now automatically check for CTE references first
                context.log("Planning main query with CTEs");
                planner.plan_select(select)
            }
            SelectStatement::SetOperation(_) => {
                // Set operations (UNION, INTERSECT, EXCEPT) are not yet supported in CTE context
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "Set operations in CTE main queries are not yet supported".to_string()
                ));
            }
        }
    }
}

/// Execution context for operators
pub struct ExecutionContext {
    pub logs: Vec<String>,
    pub catalog: Option<std::sync::Arc<crate::catalog::CatalogManager>>,
    pub buffer_manager: Option<std::sync::Arc<crate::storage::BufferPoolManager>>,
    pub transaction_manager: Option<std::sync::Arc<crate::transaction::TransactionManager>>,
    pub auto_increment_counters: std::collections::HashMap<String, i64>,
    /// Outer context values for correlated subqueries (column_name -> value)
    pub outer_context_values: Option<std::collections::HashMap<String, crate::types::Value>>,
    /// Materialized CTEs available in the current execution context
    pub materialized_ctes: Option<std::collections::HashMap<String, QueryResult>>,
    /// Current transaction ID
    pub transaction_id: Option<u64>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("logs", &self.logs)
            .field("catalog", &"<CatalogManager>")
            .field("buffer_manager", &"<BufferPoolManager>")
            .field("transaction_manager", &"<TransactionManager>")
            .finish()
    }
}

impl ExecutionContext {
    pub fn new() -> Self {
        let mut context = Self {
            logs: Vec::new(),
            catalog: None,
            buffer_manager: None,
            transaction_manager: None,
            auto_increment_counters: std::collections::HashMap::new(),
            outer_context_values: None,
            materialized_ctes: None,
            transaction_id: None,
        };
        // Load persistent auto-increment counters
        let _ = context.load_auto_increment_counters();
        context
    }

    pub fn set_catalog(&mut self, catalog: std::sync::Arc<crate::catalog::CatalogManager>) {
        self.catalog = Some(catalog);
    }

    pub fn set_buffer_manager(&mut self, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) {
        self.buffer_manager = Some(buffer_manager);
    }

    pub fn set_transaction_manager(&mut self, transaction_manager: std::sync::Arc<crate::transaction::TransactionManager>) {
        self.transaction_manager = Some(transaction_manager);
    }

    pub fn get_catalog(&self) -> Option<&std::sync::Arc<crate::catalog::CatalogManager>> {
        self.catalog.as_ref()
    }

    pub fn get_buffer_manager(&self) -> Option<&std::sync::Arc<crate::storage::BufferPoolManager>> {
        self.buffer_manager.as_ref()
    }

    pub fn get_transaction_manager(&self) -> Option<&std::sync::Arc<crate::transaction::TransactionManager>> {
        self.transaction_manager.as_ref()
    }

    pub fn set_outer_context_values(&mut self, values: std::collections::HashMap<String, crate::types::Value>) {
        self.outer_context_values = Some(values);
    }

    pub fn get_outer_context_values(&self) -> Option<&std::collections::HashMap<String, crate::types::Value>> {
        self.outer_context_values.as_ref()
    }

    pub fn set_materialized_ctes(&mut self, ctes: std::collections::HashMap<String, QueryResult>) {
        self.materialized_ctes = Some(ctes);
    }

    pub fn get_materialized_ctes(&self) -> Option<&std::collections::HashMap<String, QueryResult>> {
        self.materialized_ctes.as_ref()
    }

    pub fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
        log::debug!("{}", message);
    }

    pub fn get_logs(&self) -> &[String] {
        &self.logs
    }

    /// Load auto-increment counters from persistent storage
    fn load_auto_increment_counters(&mut self) -> Result<()> {
        let path = "auto_increment_counters.json";
        if std::path::Path::new(path).exists() {
            match std::fs::read_to_string(path) {
                Ok(data) => {
                    match serde_json::from_str(&data) {
                        Ok(counters) => self.auto_increment_counters = counters,
                        Err(e) => {
                            self.logs.push(format!("Warning: Failed to parse auto-increment counters: {}", e));
                        }
                    }
                }
                Err(e) => {
                    self.logs.push(format!("Warning: Failed to read auto-increment counters: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Save auto-increment counters to persistent storage
    pub fn save_auto_increment_counters(&mut self) -> Result<()> {
        match serde_json::to_string(&self.auto_increment_counters) {
            Ok(data) => {
                if let Err(e) = std::fs::write("auto_increment_counters.json", data) {
                    self.logs.push(format!("Warning: Failed to save auto-increment counters: {}", e));
                }
            }
            Err(e) => {
                self.logs.push(format!("Warning: Failed to serialize auto-increment counters: {}", e));
            }
        }
        Ok(())
    }
}

/// Sort operator for ORDER BY clause
#[derive(Debug)]
pub struct SortOperator {
    pub input: Box<PlanNode>,
    pub order_by: Vec<OrderBy>,
}

impl SortOperator {
    pub fn new(input: PlanNode, order_by: Vec<OrderBy>) -> Self {
        Self {
            input: Box::new(input),
            order_by,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("SortOperator: Executing sort with {} order by clauses", self.order_by.len()));

        // Execute input plan to get results
        let mut input_result = self.input.execute(context)?;

        context.log(&format!("SortOperator: Got {} rows to sort", input_result.rows.len()));

        // If no rows or no order by clauses, return as-is
        if input_result.rows.is_empty() || self.order_by.is_empty() {
            return Ok(input_result);
        }

        // Create expression evaluator
        let evaluator = ExpressionEvaluator::new();

        // Sort rows using stable sort to maintain relative order for equal keys
        input_result.rows.sort_by(|row_a, row_b| {
            for order_by in &self.order_by {
                // Create evaluation context with column names and values
                let mut context_a = EvaluationContext::new();
                let mut context_b = EvaluationContext::new();

                // Populate contexts with column values
                for (i, col_name) in input_result.column_names.iter().enumerate() {
                    if i < row_a.len() {
                        context_a.set_variable(col_name, row_a[i].clone());
                    }
                    if i < row_b.len() {
                        context_b.set_variable(col_name, row_b[i].clone());
                    }
                }

                // Evaluate the expression for both rows
                let eval_result_a = evaluator.evaluate(&order_by.expr, &context_a);
                let eval_result_b = evaluator.evaluate(&order_by.expr, &context_b);

                match (eval_result_a, eval_result_b) {
                    (Ok(val_a), Ok(val_b)) => {
                        let comparison = compare_values(&val_a, &val_b, order_by.nulls);
                        if comparison != std::cmp::Ordering::Equal {
                            // Reverse comparison for DESC order
                            return match order_by.direction {
                                SortDirection::Asc => comparison,
                                SortDirection::Desc => comparison.reverse(),
                            };
                        }
                    }
                    _ => {
                        // If evaluation fails, treat as equal and continue to next order by
                        continue;
                    }
                }
            }
            std::cmp::Ordering::Equal
        });

        context.log(&format!("SortOperator: Sorted {} rows", input_result.rows.len()));
        Ok(input_result)
    }
}

/// Limit operator for LIMIT and OFFSET clauses
#[derive(Debug)]
pub struct LimitOperator {
    pub input: Box<PlanNode>,
    pub limit: i64,
    pub offset: Option<i64>,
}

impl LimitOperator {
    pub fn new(input: PlanNode, limit: i64, offset: Option<i64>) -> Self {
        Self {
            input: Box::new(input),
            limit,
            offset,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("LimitOperator: Executing limit={} offset={:?}", self.limit, self.offset));

        // Execute input plan to get results
        let mut input_result = self.input.execute(context)?;

        context.log(&format!("LimitOperator: Got {} rows to limit", input_result.rows.len()));

        // Calculate offset and limit bounds
        let offset_val = self.offset.unwrap_or(0);
        let start_idx = if offset_val < 0 {
            context.log(&format!("LimitOperator: Negative offset {}, treating as 0", offset_val));
            0
        } else {
            offset_val as usize
        };

        let end_idx = if self.limit < 0 {
            // Negative limit means no limit (return all remaining rows)
            input_result.rows.len()
        } else {
            let limit_val = self.limit as usize;
            start_idx + limit_val
        };

        // Apply limit and offset
        if start_idx >= input_result.rows.len() {
            // Offset is beyond the end of the result set
            input_result.rows.clear();
            context.log("LimitOperator: Offset beyond result set, returning empty rows");
        } else {
            let end_idx = std::cmp::min(end_idx, input_result.rows.len());
            input_result.rows = input_result.rows[start_idx..end_idx].to_vec();
            context.log(&format!("LimitOperator: Applied limit, returning {} rows", input_result.rows.len()));
        }

        Ok(input_result)
    }
}

/// Distinct operator - removes duplicate rows
pub struct DistinctOperator {
    pub input: Box<PlanNode>,
}

impl DistinctOperator {
    pub fn new(input: PlanNode) -> Self {
        Self {
            input: Box::new(input),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("DistinctOperator: Executing distinct operation");

        // Execute input plan to get results
        let input_result = self.input.execute(context)?;

        context.log(&format!("DistinctOperator: Got {} rows to deduplicate", input_result.rows.len()));

        // Deduplicate rows using HashSet for O(1) lookup
        let mut seen_rows: HashSet<Vec<Value>> = HashSet::new();
        let mut distinct_rows: Vec<Vec<Value>> = Vec::new();

        for row in input_result.rows {
            // HashSet.insert returns true if the value was inserted (not present before)
            if seen_rows.insert(row.clone()) {
                distinct_rows.push(row);
            }
        }

        context.log(&format!("DistinctOperator: Returning {} distinct rows", distinct_rows.len()));

        Ok(QueryResult {
            rows: distinct_rows,
            column_names: input_result.column_names,
        })
    }
}
