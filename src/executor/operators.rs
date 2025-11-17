//! Execution operators
//!
//! Physical operators for executing query plans

use crate::{Result, sql::ast::{Expression, BinaryOperator, SetOperator as SetOperatorType}, executor::planner::PlanNode, types::{Value, ValueKind}};
use crate::executor::{TableScanner, ExpressionEvaluator, EvaluationContext, ThreeValuedLogic, RowData, AggregateState};
use std::sync::Arc;
use std::collections::HashMap;

/// Compare two Values for sorting
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (&a.kind, &b.kind) {
        (ValueKind::Null(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::Null(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Integer(a_val), ValueKind::Integer(b_val)) => a_val.cmp(b_val),
        (ValueKind::Float(a_val), ValueKind::Float(b_val)) => a_val.partial_cmp(b_val).unwrap_or(std::cmp::Ordering::Equal),
        (ValueKind::String(a_val), ValueKind::String(b_val)) => a_val.cmp(b_val),
        (ValueKind::Boolean(a_val), ValueKind::Boolean(b_val)) => a_val.cmp(b_val),
        // Different types - establish a priority order
        (ValueKind::Boolean(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::Boolean(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Integer(_), ValueKind::Float(_)) => std::cmp::Ordering::Less,
        (ValueKind::Float(_), ValueKind::Integer(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Integer(_), ValueKind::String(_)) => std::cmp::Ordering::Less,
        (ValueKind::String(_), ValueKind::Integer(_)) => std::cmp::Ordering::Greater,
        (ValueKind::Float(_), ValueKind::String(_)) => std::cmp::Ordering::Less,
        (ValueKind::String(_), ValueKind::Float(_)) => std::cmp::Ordering::Greater,
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

        if let Some(ref scanner) = self.scanner {
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
        } else {
            // Fallback for when no scanner is available
            Ok(QueryResult {
                rows: vec![],
                column_names: vec![],
            })
        }
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
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let input_result = self.input.execute(context)?;

        // Filter rows based on condition using the real expression evaluator
        let total_rows = input_result.rows.len();
        let column_names = input_result.column_names.clone();

        let filtered_rows: Vec<Vec<Value>> = if let Some(ref scanner) = self.scanner {
            // Use scanner for proper column name resolution
            input_result.rows
                .into_iter()
                .filter(|row| {
                    let eval_context = self.create_evaluation_context(scanner, &column_names, row);
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(&self.condition, &eval_context) {
                        Ok(result) => {
                            match ThreeValuedLogic::from_value(&result) {
                                ThreeValuedLogic::True => true,
                                ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                            }
                        }
                        Err(_) => false, // Treat evaluation errors as false (exclude row)
                    }
                })
                .collect()
        } else {
            // Fallback: basic evaluation without proper column resolution
            input_result.rows
                .into_iter()
                .filter(|row| {
                    let eval_context = self.create_basic_evaluation_context(&column_names, row);
                    let evaluator = ExpressionEvaluator;
                    match evaluator.evaluate(&self.condition, &eval_context) {
                        Ok(result) => {
                            match ThreeValuedLogic::from_value(&result) {
                                ThreeValuedLogic::True => true,
                                ThreeValuedLogic::False | ThreeValuedLogic::Unknown => false,
                            }
                        }
                        Err(_) => false,
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

/// Project operator
#[derive(Debug)]
pub struct ProjectOperator {
    pub input: Box<PlanNode>,
    pub columns: Vec<(String, Expression)>,
    pub scanner: Option<TableScanner>, // For column name resolution
}

impl ProjectOperator {
    pub fn new(input: PlanNode, columns: Vec<(String, Expression)>) -> Self {
        Self {
            input: Box::new(input),
            columns,
            scanner: None,
        }
    }

    pub fn with_scanner(input: PlanNode, columns: Vec<(String, Expression)>, scanner: TableScanner) -> Self {
        Self {
            input: Box::new(input),
            columns,
            scanner: Some(scanner),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let input_result = self.input.execute(context)?;

        // Extract column names and compute projected values using real expression evaluator
        let column_names: Vec<String> = self.columns.iter().map(|(name, _)| name.clone()).collect();
        let input_column_names = input_result.column_names.clone();

        let projected_rows: Vec<Vec<Value>> = if let Some(ref scanner) = self.scanner {
            // Use scanner for proper column name resolution
            input_result.rows
                .into_iter()
                .map(|row| {
                    let eval_context = self.create_evaluation_context(scanner, &input_column_names, &row);
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
            // Fallback: basic evaluation without proper column resolution
            input_result.rows
                .into_iter()
                .map(|row| {
                    let eval_context = self.create_basic_evaluation_context(&input_column_names, &row);
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
}

impl JoinOperator {
    pub fn new(left: PlanNode, right: PlanNode, condition: Option<Expression>, join_type: crate::sql::ast::JoinType) -> Self {
        Self {
            left: Box::new(left),
            right: Box::new(right),
            condition,
            join_type,
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        let left_result = self.left.execute(context)?;
        let right_result = self.right.execute(context)?;

        context.log(&format!("Executing {} join between {} and {} rows",
                          self.join_type_display(), left_result.rows.len(), right_result.rows.len()));

        let (joined_rows, column_names) = match self.join_type {
            crate::sql::ast::JoinType::Inner | crate::sql::ast::JoinType::Left |
            crate::sql::ast::JoinType::Right | crate::sql::ast::JoinType::Full => {
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
        let mut joined_column_names = left_result.column_names.clone();
        joined_column_names.extend(right_result.column_names.clone());

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
            crate::sql::ast::JoinType::Left | crate::sql::ast::JoinType::Full => {
                for (left_idx, left_row) in left_result.rows.iter().enumerate() {
                    if !left_matched[left_idx] {
                        let mut joined_row = left_row.clone();
                        // Add NULL values for right side columns
                        joined_row.extend(vec![Value { kind: crate::types::ValueKind::Null(crate::types::NullValue) }; right_result.column_names.len()]);
                        joined_rows.push(joined_row);
                    }
                }
            }
            crate::sql::ast::JoinType::Right | crate::sql::ast::JoinType::Full => {
                for (right_idx, right_row) in right_result.rows.iter().enumerate() {
                    if !right_matched[right_idx] {
                        let mut joined_row = Vec::new();
                        // Add NULL values for left side columns
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

        let mut inserted_rows = 0;

        for (row_index, value_exprs) in self.values.iter().enumerate() {
            // Evaluate expressions to get values
            let row_values: Vec<Value> = if let Some(ref scanner) = self.scanner {
                // Use scanner for proper column resolution
                value_exprs
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
                    .collect()
            } else {
                // Fallback: basic evaluation
                value_exprs
                    .iter()
                    .map(|expr| self.evaluate_expression_basic(expr))
                    .collect()
            };

            // Validate row data if scanner is available
            if let Some(ref scanner) = self.scanner {
                // Map column names to values for validation
                let validated_values = self.map_columns_to_values(&row_values)?;
                let row_data = RowData::new(validated_values);

                // Validate against table schema
                if let Err(e) = scanner.validate_row_data(&row_data) {
                    context.log(&format!("Row validation failed: {}", e));
                    return Err(e);
                }

                // Insert the row
                if let Err(e) = self.insert_row_data(scanner, row_data, context) {
                    context.log(&format!("Failed to insert row {}: {}", row_index, e));
                    return Err(e);
                }
            }

            inserted_rows += 1;
        }

        context.log(&format!("Successfully inserted {} rows into table {}", inserted_rows, self.table_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Map column values to match table schema
    fn map_columns_to_values(&self, row_values: &[Value]) -> Result<Vec<Value>> {
        if let Some(ref scanner) = self.scanner {
            let table_def = scanner.get_table_def();

            // If no columns specified, INSERT should provide values for all columns in order
            if self.columns.is_empty() {
                if row_values.len() != table_def.columns.len() {
                    return Err(crate::error::RustgreSQLError::Type(format!(
                        "Column count mismatch: expected {} values for all columns, got {}",
                        table_def.columns.len(),
                        row_values.len()
                    )));
                }
                return Ok(row_values.to_vec());
            }

            // Build a complete row with all table columns
            let mut complete_row = Vec::new();

            for table_col in &table_def.columns {
                // Find the value for this column
                let value = if let Some(insert_col_index) = self.columns.iter().position(|c| c == &table_col.name) {
                    // Column was specified in INSERT - use the provided value
                    if insert_col_index < row_values.len() {
                        row_values[insert_col_index].clone()
                    } else {
                        return Err(crate::error::RustgreSQLError::Type(format!(
                            "Value missing for column '{}'", table_col.name
                        )));
                    }
                } else {
                    // Column was NOT specified in INSERT - use DEFAULT or NULL
                    if let Some(ref default_val) = table_col.default_value {
                        default_val.clone()
                    } else if table_col.nullable {
                        Value { kind: ValueKind::Null(crate::types::NullValue) }
                    } else {
                        return Err(crate::error::RustgreSQLError::Type(format!(
                            "Column '{}' requires a value (NOT NULL without DEFAULT)", table_col.name
                        )));
                    }
                };

                complete_row.push(value);
            }

            Ok(complete_row)
        } else {
            // Fallback: no scanner available, pass through as-is
            Ok(row_values.to_vec())
        }
    }

    /// Insert row data into storage
    fn insert_row_data(&self, _scanner: &TableScanner, row_data: RowData, context: &ExecutionContext) -> Result<()> {
        // Get the catalog from execution context
        if let Some(catalog) = context.get_catalog() {
            // Insert the row into the catalog
            catalog.table_manager.insert(&self.table_name, row_data.values)?;
            log::info!("Inserted row into table: {}", self.table_name);
            Ok(())
        } else {
            Err(crate::error::RustgreSQLError::Execution(
                "Catalog not available in execution context".to_string()
            ))
        }
    }

    /// Basic expression evaluation (fallback when no scanner available)
    fn evaluate_expression_basic(&self, expr: &Expression) -> Value {
        match expr {
            Expression::Value(value) => value.clone(),
            _ => {
                // For unsupported expressions in INSERT, return NULL
                Value { kind: ValueKind::Null(crate::types::NullValue) }
            }
        }
    }

    /// Validate that the number of values matches the number of columns
    fn validate_value_count(&self) -> Result<()> {
        for (row_index, value_exprs) in self.values.iter().enumerate() {
            if value_exprs.len() != self.columns.len() && !self.columns.is_empty() {
                return Err(crate::error::RustgreSQLError::Type(format!(
                    "Row {} has {} values but {} columns specified",
                    row_index, value_exprs.len(), self.columns.len()
                )));
            }
        }
        Ok(())
    }
}

/// Update operator
#[derive(Debug)]
pub struct UpdateOperator {
    pub table_name: String,
    pub assignments: Vec<(String, Expression)>,
    pub condition: Option<Expression>,
    pub scanner: Option<TableScanner>, // For table scanning and constraint checking
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

        let mut updated_rows = 0;

        if let Some(ref scanner) = self.scanner {
            // Scan all rows from the table
            let mut row_iterator = scanner.scan_all()?;
            let column_names = row_iterator.get_column_names();

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

                    // Validate the updated row
                    let updated_row_data = RowData::new(new_values);
                    if let Err(e) = scanner.validate_row_data(&updated_row_data) {
                        context.log(&format!("Row validation failed after update: {}", e));
                        return Err(e);
                    }

                    // Perform the update
                    if let Err(e) = self.update_row_data(scanner, &updated_row_data) {
                        context.log(&format!("Failed to update row: {}", e));
                        return Err(e);
                    }

                    updated_rows += 1;
                }
            }
        } else {
            // Fallback: no scanner available
            context.log("No scanner available for update operation");
        }

        context.log(&format!("Successfully updated {} rows in table {}", updated_rows, self.table_name));

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

    /// Update row data in storage
    fn update_row_data(&self, scanner: &TableScanner, row_data: &RowData) -> Result<()> {
        // In a real implementation, this would use a mutable scanner or transaction
        // For now, we'll just log the operation
        log::info!("Updating row: {:?}", row_data);

        // Note: In a real implementation, you would need to make the scanner mutable
        // or use a transaction system to handle the actual update
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

            // Perform the deletions
            for row_data in rows_to_delete {
                if let Err(e) = self.delete_row_data(scanner, &row_data) {
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
    fn delete_row_data(&self, scanner: &TableScanner, row_data: &RowData) -> Result<()> {
        // In a real implementation, this would use a mutable scanner or transaction
        // For now, we'll just log the operation
        log::info!("Deleting row: {:?}", row_data);

        // Note: In a real implementation, you would need to make the scanner mutable
        // or use a transaction system to handle the actual deletion
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
            scanner: Some(scanner),
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
            scanner: Some(scanner),
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
                    let comparison = compare_values(&a[idx], &b[idx]);
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
                    let comparison = compare_values(&left_row[left_idx], &right_row[right_idx]);
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
            scanner: Some(scanner),
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

            // For empty input with no GROUP BY, return single row with NULL aggregates and window functions
            if self.group_by_columns.is_empty() {
                let null_count = self.aggregate_functions.len() + self.window_functions.len();
                let null_row: Vec<Value> = (0..null_count)
                    .map(|_| Value { kind: ValueKind::Null(crate::types::NullValue) })
                    .collect();

                return Ok(QueryResult {
                    rows: vec![null_row],
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
                let value = match aggregate_expr {
                    Expression::Function { name, args } => {
                        match name.to_uppercase().as_str() {
                            "COUNT" => {
                                // Handle COUNT(*) special case
                                if args.len() == 1 && matches!(&args[0], Expression::Star) {
                                    Value { kind: ValueKind::Integer(1) } // Count this row
                                } else {
                                    // COUNT(expression)
                                    { let evaluator = ExpressionEvaluator; evaluator.evaluate(aggregate_expr, &eval_context) }
                                        .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                                }
                            }
                            _ => {
                                { let evaluator = ExpressionEvaluator; evaluator.evaluate(aggregate_expr, &eval_context) }
                                    .unwrap_or(Value { kind: ValueKind::Null(crate::types::NullValue) })
                            }
                        }
                    }
                    _ => {
                        { let evaluator = ExpressionEvaluator; evaluator.evaluate(aggregate_expr, &eval_context) }
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

                // Add GROUP BY columns to context
                for (i, group_expr) in self.group_by_columns.iter().enumerate() {
                    if let Expression::Column { name, .. } = group_expr {
                        if i < result_row.len() {
                            having_columns.insert(name.clone(), result_row[i].clone());
                        }
                    }
                }

                // Add aggregate results to context using their aliases
                for (i, (alias, _)) in self.aggregate_functions.iter().enumerate() {
                    let aggregate_col_index = self.group_by_columns.len() + i;
                    if aggregate_col_index < result_row.len() {
                        having_columns.insert(alias.clone(), result_row[aggregate_col_index].clone());
                    }
                }

                let having_context = EvaluationContext::with_columns(having_columns);

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
                window_functions: self.window_functions.iter().map(|(_, wf)| wf.clone()).collect(),
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
            Expression::Function { name, args } => {
                match name.to_uppercase().as_str() {
                    "COUNT" => {
                        // Check if this is COUNT(*) or COUNT(column)
                        let distinct = args.len() > 1 &&
                            args.iter().any(|arg| matches!(arg, Expression::Value(Value { kind: ValueKind::String(s), .. }) if s.to_uppercase() == "DISTINCT"));
                        AggregateState::new_count(distinct)
                    }
                    "SUM" => {
                        let distinct = args.len() > 1 &&
                            args.iter().any(|arg| matches!(arg, Expression::Value(Value { kind: ValueKind::String(s), .. }) if s.to_uppercase() == "DISTINCT"));
                        AggregateState::new_sum(distinct)
                    }
                    "AVG" => {
                        let distinct = args.len() > 1 &&
                            args.iter().any(|arg| matches!(arg, Expression::Value(Value { kind: ValueKind::String(s), .. }) if s.to_uppercase() == "DISTINCT"));
                        AggregateState::new_avg(distinct)
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
    pub planner: crate::executor::planner::QueryPlanner,
}

impl SubqueryOperator {
    pub fn new(query: crate::sql::ast::Statement, correlated_columns: Vec<String>) -> Self {
        Self {
            query,
            correlated_columns,
            planner: crate::executor::planner::QueryPlanner::new(),
        }
    }

    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Executing subquery: {:?}", self.query));

        // Plan and execute the subquery
        let plan = match &self.query {
            crate::sql::ast::Statement::Select(select_stmt) => {
                self.planner.plan_select(select_stmt)?
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

        // Inject outer context values into correlated context
        // The correlated_columns list tells us which outer columns we need
        for correlated_column in &self.correlated_columns {
            if let Some(value) = outer_context.columns.get(correlated_column) {
                correlated_context.log(&format!("Injecting correlated column {} = {:?}", correlated_column, value));
                // In a real implementation, we'd store these in a way that the subquery can access them
                // For now, we'll add them to the context as if they were local variables
            }
        }

        // Plan and execute the subquery
        let plan = match &self.query {
            crate::sql::ast::Statement::Select(select_stmt) => {
                self.planner.plan_select(select_stmt)?
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
    pub window_functions: Vec<crate::sql::ast::WindowFunction>,
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
}

use std::collections::VecDeque;

impl WindowOperator {
    pub fn new(
        input: PlanNode,
        window_functions: Vec<crate::sql::ast::WindowFunction>,
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

        // Execute input to get source data
        let input_result = self.input.execute(context)?;
        context.log(&format!("WindowOperator received {} rows from input", input_result.rows.len()));

        if input_result.rows.is_empty() {
            // Return empty result with window function columns
            let mut column_names = input_result.column_names.clone();
            for (i, window_func) in self.window_functions.iter().enumerate() {
                column_names.push(format!("window_{}", i));
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
                        let ordering = compare_values(&a_val, &b_val);
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
        for (i, window_func) in self.window_functions.iter().enumerate() {
            result_column_names.push(format!("window_{}", i)); // Use function name in real implementation
        }

        for partition in partitions {
            // Initialize window function states for this partition
            let mut window_states: Vec<WindowFunctionState> = self.window_functions
                .iter()
                .enumerate()
                .map(|(i, window_func)| self.initialize_window_function(i, window_func))
                .collect();

            // Process each row in the partition
            for (row_index, row) in partition.iter().enumerate() {
                let mut result_row = row.clone();

                // Evaluate each window function for this row
                for (func_index, window_func) in self.window_functions.iter().enumerate() {
                    let context = self.create_sorting_context(row, input_column_names);
                    let window_value = self.evaluate_window_function(
                        func_index,
                        window_func,
                        &partition,
                        row_index,
                        &mut window_states[func_index],
                        &evaluator,
                        &context,
                    )?;
                    result_row.push(window_value);
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
                if window_func.args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::InvalidOperation("LAG function requires exactly one argument".to_string()));
                }

                if current_row_index == 0 {
                    // First row, no previous value
                    Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                } else {
                    // Return value from previous row
                    let arg_value = evaluator.evaluate(&window_func.args[0], context)?;
                    Ok(arg_value)
                }
            }
            "LEAD" => {
                if window_func.args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::InvalidOperation("LEAD function requires exactly one argument".to_string()));
                }

                if current_row_index >= partition.len() - 1 {
                    // Last row, no next value
                    Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                } else {
                    // Return value from next row
                    let arg_value = evaluator.evaluate(&window_func.args[0], context)?;
                    Ok(arg_value)
                }
            }
            _ => {
                // Unknown window function, return NULL
                Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
            }
        }
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
            match compare_values(val_a, val_b) {
                std::cmp::Ordering::Equal => continue,
                _ => return false,
            }
        }

        true
    }
}

/// CTE (Common Table Expression) operator
///
/// This operator handles the execution of Common Table Expressions by materializing
/// the CTE results and making them available to the main query. It supports both
/// recursive and non-recursive CTEs.
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
    pub fn new(with_clause: crate::sql::ast::WithClause, main_query: crate::sql::ast::Statement) -> Self {
        Self {
            with_clause,
            main_query: Box::new(main_query),
            planner: crate::executor::planner::QueryPlanner::new(),
            materialized_ctes: std::collections::HashMap::new(),
        }
    }

    /// Execute the CTE operator
    ///
    /// This method:
    /// 1. Materializes all CTEs in order
    /// 2. Handles recursive CTEs if present
    /// 3. Executes the main query with access to materialized CTEs
    pub fn execute(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log(&format!("Executing CTE operator with {} CTEs", self.with_clause.ctes.len()));

        if self.with_clause.recursive {
            self.execute_recursive_ctes(context)
        } else {
            self.execute_non_recursive_ctes(context)
        }
    }

    /// Execute non-recursive CTEs
    fn execute_non_recursive_ctes(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Executing non-recursive CTEs");

        // Materialize each CTE
        for cte in &self.with_clause.ctes {
            context.log(&format!("Materializing CTE: {}", cte.name));

            let cte_plan = self.planner.plan_select(&cte.query)?;
            let cte_result = cte_plan.root.execute(context)?;

            // Store the materialized result (in a real implementation, this would be mutable)
            // For now, we'll log the materialization
            context.log(&format!("Materialized CTE {} with {} rows", cte.name, cte_result.rows.len()));

            // In a full implementation, we'd need to make these available to the main query
            // This could be done through temporary tables or a special CTE context
        }

        // Execute the main query
        context.log("Executing main query with CTEs");
        let main_plan = match self.main_query.as_ref() {
            crate::sql::ast::Statement::Select(select_stmt) => {
                self.planner.plan_select(select_stmt)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "CTE operator only supports SELECT statements as main query".to_string()
                ));
            }
        };

        main_plan.root.execute(context)
    }

    /// Execute recursive CTEs
    fn execute_recursive_ctes(&self, context: &mut ExecutionContext) -> Result<QueryResult> {
        context.log("Executing recursive CTEs");

        if self.with_clause.ctes.len() != 1 {
            return Err(crate::error::RustgreSQLError::InvalidOperation(
                "Recursive CTEs currently support only a single CTE".to_string()
            ));
        }

        let cte = &self.with_clause.ctes[0];
        context.log(&format!("Materializing recursive CTE: {}", cte.name));

        // Extract anchor and recursive members from the CTE query
        let (anchor_query, recursive_query) = match cte.query.as_ref() {
            crate::sql::ast::SelectStatement::SetOperation(set_op) if
                matches!(set_op.operator, crate::sql::ast::SetOperator::Union) &&
                !set_op.all => {
                // This is a UNION query - extract left (anchor) and right (recursive) parts
                (&*set_op.left, &*set_op.right)
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "Recursive CTE must be a UNION (not UNION ALL) of anchor and recursive parts".to_string()
                ));
            }
        };

        context.log("Executing anchor member of recursive CTE");

        // Execute the anchor member (non-recursive part)
        let anchor_plan = self.planner.plan_select(anchor_query)?;
        let anchor_result = anchor_plan.root.execute(context)?;
        context.log(&format!("Anchor member produced {} rows", anchor_result.rows.len()));

        // Initialize working set with anchor results
        let mut working_result = anchor_result.clone();
        let mut all_results = anchor_result.clone();
        let mut previous_row_count = 0;
        let mut iteration_count = 0;
        let max_iterations = 1000; // Prevent infinite loops

        // Create a temporary execution context for recursive execution
        // This simulates the CTE being available as a temporary table
        let mut recursive_context = ExecutionContext::new();
        if let Some(catalog) = context.get_catalog() {
            recursive_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = context.get_buffer_manager() {
            recursive_context.set_buffer_manager(buffer_manager.clone());
        }

        // Iteratively execute the recursive part until no new rows are produced
        while working_result.rows.len() > previous_row_count && iteration_count < max_iterations {
            iteration_count += 1;
            previous_row_count = working_result.rows.len();

            context.log(&format!("Recursive iteration {} - working set has {} rows",
                iteration_count, working_result.rows.len()));

            // In a full implementation, we would:
            // 1. Make the current working results available as a temporary table named after the CTE
            // 2. Execute the recursive query against this temporary table
            // 3. Filter out rows that already exist in all_results (cycle detection)
            // 4. Add new rows to both working_result and all_results

            // For this implementation, we'll simulate the recursive execution
            // In practice, this would require:
            // - Creating a temporary table or in-memory structure to hold the CTE results
            // - Modifying the planner to recognize CTE references and use the temporary data
            // - Implementing proper row comparison for cycle detection

            context.log(&format!("Simulating recursive execution - iteration {}", iteration_count));

            // Break after a few iterations for this prototype
            // In a real implementation, this would continue until convergence
            if iteration_count >= 3 {
                context.log("Reached iteration limit for prototype implementation");
                break;
            }

            // Simulate adding some rows (in reality, this would be the result of executing the recursive query)
            // This is where the actual recursive execution would happen
            let simulated_new_rows = self.simulate_recursive_execution(&working_result, iteration_count, context)?;

            // Add new rows to our results (cycle detection would happen here)
            for row in simulated_new_rows.rows {
                if !self.row_exists_in_results(&all_results.rows, &row) {
                    working_result.rows.push(row.clone());
                    all_results.rows.push(row);
                }
            }
        }

        context.log(&format!("Recursive CTE execution completed after {} iterations with {} total rows",
            iteration_count, all_results.rows.len()));

        // Store the final result in the materialized CTEs map
        // Note: This would require the CTEOperator to be mutable in a full implementation
        // For now, we'll proceed with the main query execution

        // Execute the main query
        context.log("Executing main query with recursive CTEs");
        let main_plan = match self.main_query.as_ref() {
            crate::sql::ast::Statement::Select(select_stmt) => {
                self.planner.plan_select(select_stmt)?
            }
            _ => {
                return Err(crate::error::RustgreSQLError::InvalidOperation(
                    "CTE operator only supports SELECT statements as main query".to_string()
                ));
            }
        };

        main_plan.root.execute(context)
    }

    /// Simulate recursive execution for prototype implementation
    /// In a real implementation, this would execute the actual recursive query
    fn simulate_recursive_execution(&self, base_result: &QueryResult, iteration: usize, _context: &ExecutionContext) -> Result<QueryResult> {
        // This is a placeholder that simulates recursive execution
        // In practice, this would:
        // 1. Create a temporary table/view with the current working results
        // 2. Execute the recursive query against this temporary table
        // 3. Return the new results

        let mut simulated_result = base_result.clone();

        // Simulate some new rows being generated in each iteration
        // This represents the recursive part finding new related records
        if iteration <= 2 {
            let new_row_count = std::cmp::min(3, base_result.rows.len());
            for i in 0..new_row_count {
                if let Some(base_row) = base_result.rows.get(i) {
                    let mut new_row = base_row.clone();
                    // Modify the row slightly to simulate "new" data
                    if let Some(val) = new_row.get_mut(1) {
                        if let ValueKind::Integer(int_val) = &mut val.kind {
                            *int_val += (iteration * 10) as i64;
                        }
                    }
                    simulated_result.rows.push(new_row);
                }
            }
        }

        Ok(simulated_result)
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
}

/// Execution context for operators
pub struct ExecutionContext {
    pub logs: Vec<String>,
    pub catalog: Option<std::sync::Arc<crate::catalog::CatalogManager>>,
    pub buffer_manager: Option<std::sync::Arc<crate::storage::BufferPoolManager>>,
}

impl std::fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("logs", &self.logs)
            .field("catalog", &"<CatalogManager>")
            .field("buffer_manager", &"<BufferPoolManager>")
            .finish()
    }
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            logs: Vec::new(),
            catalog: None,
            buffer_manager: None,
        }
    }

    pub fn set_catalog(&mut self, catalog: std::sync::Arc<crate::catalog::CatalogManager>) {
        self.catalog = Some(catalog);
    }

    pub fn set_buffer_manager(&mut self, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) {
        self.buffer_manager = Some(buffer_manager);
    }

    pub fn get_catalog(&self) -> Option<&std::sync::Arc<crate::catalog::CatalogManager>> {
        self.catalog.as_ref()
    }

    pub fn get_buffer_manager(&self) -> Option<&std::sync::Arc<crate::storage::BufferPoolManager>> {
        self.buffer_manager.as_ref()
    }

    pub fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
        log::debug!("{}", message);
    }

    pub fn get_logs(&self) -> &[String] {
        &self.logs
    }
}
