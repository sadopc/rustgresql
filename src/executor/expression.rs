//! Expression evaluation engine
//!
//! Provides comprehensive expression evaluation with three-valued logic (TRUE/FALSE/NULL)
//! and support for all SQL expression types.

use crate::{Result, RustgreSQLError, sql::ast::{Expression, BinaryOperator, UnaryOperator}, types::{Value, ValueKind, NullValue}};
use std::collections::HashMap;

/// Evaluation context for expression evaluation
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// Column values for the current row (column_name -> value)
    pub columns: HashMap<String, Value>,
    /// Row data as a list for indexed access
    pub row_data: Option<Vec<Value>>,
    /// For joins: left row data
    pub left_row: Option<Vec<Value>>,
    /// For joins: right row data
    pub right_row: Option<Vec<Value>>,
    /// For joins: left columns
    pub left_columns: Option<Vec<String>>,
    /// For joins: right columns
    pub right_columns: Option<Vec<String>>,
    /// Catalog manager for subquery execution
    pub catalog: Option<std::sync::Arc<crate::catalog::CatalogManager>>,
    /// Buffer manager for subquery execution
    pub buffer_manager: Option<std::sync::Arc<crate::storage::BufferPoolManager>>,
    /// Context tracking for subquery evaluation (e.g., IN clause)
    pub subquery_context: Option<SubqueryContext>,
    /// Aggregate function values for HAVING clause evaluation (expression -> value)
    pub having_aggregates: Option<std::collections::HashMap<String, Value>>,
    /// Materialized CTEs available in the current execution context
    pub materialized_ctes: Option<std::collections::HashMap<String, crate::executor::operators::QueryResult>>,
}

/// Context for subquery evaluation to determine how results should be processed
#[derive(Debug, Clone)]
pub enum SubqueryContext {
    /// Subquery used in IN clause - should return list of values
    InClause,
    /// Scalar subquery - should return single value
    Scalar,
    /// EXISTS subquery - should return boolean
    Exists,
}

impl EvaluationContext {
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
            row_data: None,
            left_row: None,
            right_row: None,
            left_columns: None,
            right_columns: None,
            catalog: None,
            buffer_manager: None,
            subquery_context: None,
            having_aggregates: None,
            materialized_ctes: None,
        }
    }

    pub fn with_columns(columns: HashMap<String, Value>) -> Self {
        Self {
            columns,
            row_data: None,
            left_row: None,
            right_row: None,
            left_columns: None,
            right_columns: None,
            catalog: None,
            buffer_manager: None,
            subquery_context: None,
            having_aggregates: None,
            materialized_ctes: None,
        }
    }

    pub fn with_row_data(row_data: Vec<Value>) -> Self {
        Self {
            columns: HashMap::new(),
            row_data: Some(row_data),
            left_row: None,
            right_row: None,
            left_columns: None,
            right_columns: None,
            catalog: None,
            buffer_manager: None,
            subquery_context: None,
            having_aggregates: None,
            materialized_ctes: None,
        }
    }

    pub fn with_join_data(left_row: Vec<Value>, right_row: Vec<Value>, left_columns: Vec<String>, right_columns: Vec<String>) -> Self {
        Self {
            columns: HashMap::new(),
            row_data: None,
            left_row: Some(left_row),
            right_row: Some(right_row),
            left_columns: Some(left_columns),
            right_columns: Some(right_columns),
            catalog: None,
            buffer_manager: None,
            subquery_context: None,
            having_aggregates: None,
            materialized_ctes: None,
        }
    }

    pub fn get_column_value(&self, column_name: &str) -> Option<&Value> {
        self.columns.get(column_name)
    }

    pub fn set_variable(&mut self, name: &str, value: Value) {
        self.columns.insert(name.to_string(), value);
    }

    pub fn set_catalog(&mut self, catalog: std::sync::Arc<crate::catalog::CatalogManager>) {
        self.catalog = Some(catalog);
    }

    pub fn set_buffer_manager(&mut self, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) {
        self.buffer_manager = Some(buffer_manager);
    }

    pub fn set_subquery_context(&mut self, context: SubqueryContext) {
        self.subquery_context = Some(context);
    }

    pub fn set_having_aggregates(&mut self, aggregates: std::collections::HashMap<String, Value>) {
        self.having_aggregates = Some(aggregates);
    }

    pub fn set_materialized_ctes(&mut self, ctes: std::collections::HashMap<String, crate::executor::operators::QueryResult>) {
        self.materialized_ctes = Some(ctes);
    }

    pub fn get_materialized_ctes(&self) -> Option<&std::collections::HashMap<String, crate::executor::operators::QueryResult>> {
        self.materialized_ctes.as_ref()
    }

    pub fn get_subquery_context(&self) -> Option<&SubqueryContext> {
        self.subquery_context.as_ref()
    }

    /// Create a new context with IN clause subquery context
    pub fn with_in_subquery_context() -> Self {
        Self {
            columns: HashMap::new(),
            row_data: None,
            left_row: None,
            right_row: None,
            left_columns: None,
            right_columns: None,
            catalog: None,
            buffer_manager: None,
            subquery_context: Some(SubqueryContext::InClause),
            having_aggregates: None,
            materialized_ctes: None,
        }
    }
}

/// Three-valued logic result (SQL boolean logic with NULL)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThreeValuedLogic {
    True,
    False,
    Unknown,
}

impl ThreeValuedLogic {
    pub fn from_value(value: &Value) -> Self {
        match &value.kind {
            ValueKind::Boolean(b) => {
                if *b { ThreeValuedLogic::True } else { ThreeValuedLogic::False }
            }
            ValueKind::Null(_) => ThreeValuedLogic::Unknown,
            ValueKind::Integer(i) => {
                if *i != 0 { ThreeValuedLogic::True } else { ThreeValuedLogic::False }
            }
            ValueKind::Float(f) => {
                if *f != 0.0 { ThreeValuedLogic::True } else { ThreeValuedLogic::False }
            }
            ValueKind::String(s) => {
                if !s.is_empty() { ThreeValuedLogic::True } else { ThreeValuedLogic::False }
            }
            ValueKind::Timestamp(_) => ThreeValuedLogic::True,
            ValueKind::List(list) => {
                if !list.is_empty() { ThreeValuedLogic::True } else { ThreeValuedLogic::False }
            }
        }
    }

    pub fn to_value(self) -> Value {
        match self {
            ThreeValuedLogic::True => Value { kind: ValueKind::Boolean(true) },
            ThreeValuedLogic::False => Value { kind: ValueKind::Boolean(false) },
            ThreeValuedLogic::Unknown => Value { kind: ValueKind::Null(NullValue) },
        }
    }

    pub fn and(self, other: ThreeValuedLogic) -> ThreeValuedLogic {
        match (self, other) {
            (ThreeValuedLogic::False, _) => ThreeValuedLogic::False,
            (_, ThreeValuedLogic::False) => ThreeValuedLogic::False,
            (ThreeValuedLogic::Unknown, _) => ThreeValuedLogic::Unknown,
            (_, ThreeValuedLogic::Unknown) => ThreeValuedLogic::Unknown,
            (ThreeValuedLogic::True, ThreeValuedLogic::True) => ThreeValuedLogic::True,
        }
    }

    pub fn or(self, other: ThreeValuedLogic) -> ThreeValuedLogic {
        match (self, other) {
            (ThreeValuedLogic::True, _) => ThreeValuedLogic::True,
            (_, ThreeValuedLogic::True) => ThreeValuedLogic::True,
            (ThreeValuedLogic::Unknown, _) => ThreeValuedLogic::Unknown,
            (_, ThreeValuedLogic::Unknown) => ThreeValuedLogic::Unknown,
            (ThreeValuedLogic::False, ThreeValuedLogic::False) => ThreeValuedLogic::False,
        }
    }

    pub fn not(self) -> ThreeValuedLogic {
        match self {
            ThreeValuedLogic::True => ThreeValuedLogic::False,
            ThreeValuedLogic::False => ThreeValuedLogic::True,
            ThreeValuedLogic::Unknown => ThreeValuedLogic::Unknown,
        }
    }
}

/// Aggregate function state for processing multiple rows
#[derive(Debug, Clone)]
pub enum AggregateState {
    Count { count: i64, distinct: bool, values: std::collections::HashSet<String> },
    Sum { sum: Option<f64>, distinct: bool, values: std::collections::HashSet<String> },
    Avg { sum: Option<f64>, count: i64, distinct: bool, values: std::collections::HashSet<String> },
    Min { min: Option<Value> },
    Max { max: Option<Value> },
}

impl AggregateState {
    pub fn new_count(distinct: bool) -> Self {
        Self::Count {
            count: 0,
            distinct,
            values: std::collections::HashSet::new()
        }
    }

    pub fn new_sum(distinct: bool) -> Self {
        Self::Sum {
            sum: Some(0.0),
            distinct,
            values: std::collections::HashSet::new()
        }
    }

    pub fn new_avg(distinct: bool) -> Self {
        Self::Avg {
            sum: Some(0.0),
            count: 0,
            distinct,
            values: std::collections::HashSet::new()
        }
    }

    pub fn new_min() -> Self {
        Self::Min { min: None }
    }

    pub fn new_max() -> Self {
        Self::Max { max: None }
    }

    /// Update aggregate state with a new value
    pub fn update(&mut self, value: &Value) -> Result<()> {
        match self {
            AggregateState::Count { count, distinct, values } => {
                if *distinct {
                    let value_str = format!("{:?}", value);
                    if values.insert(value_str) {
                        *count += 1;
                    }
                } else {
                    *count += 1;
                }
            }
            AggregateState::Sum { sum, distinct, values } => {
                if *distinct {
                    let value_str = format!("{:?}", value);
                    if values.insert(value_str) {
                        if let Some(num_value) = Self::extract_numeric(value) {
                            *sum = sum.map(|s| s + num_value);
                        }
                    }
                } else {
                    if let Some(num_value) = Self::extract_numeric(value) {
                        *sum = sum.map(|s| s + num_value);
                    }
                }
            }
            AggregateState::Avg { sum, count, distinct, values } => {
                if *distinct {
                    let value_str = format!("{:?}", value);
                    if values.insert(value_str) {
                        if let Some(num_value) = Self::extract_numeric(value) {
                            *sum = sum.map(|s| s + num_value);
                            *count += 1;
                        }
                    }
                } else {
                    if let Some(num_value) = Self::extract_numeric(value) {
                        *sum = sum.map(|s| s + num_value);
                        *count += 1;
                    }
                }
            }
            AggregateState::Min { min } => {
                if !matches!(value.kind, ValueKind::Null(_)) {
                    match min {
                        None => *min = Some(value.clone()),
                        Some(current_min) => {
                            if compare_values(value, current_min) == std::cmp::Ordering::Less {
                                *min = Some(value.clone());
                            }
                        }
                    }
                }
            }
            AggregateState::Max { max } => {
                if !matches!(value.kind, ValueKind::Null(_)) {
                    match max {
                        None => *max = Some(value.clone()),
                        Some(current_max) => {
                            if compare_values(value, current_max) == std::cmp::Ordering::Greater {
                                *max = Some(value.clone());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Get the final result of the aggregate
    pub fn result(&self) -> Result<Value> {
        match self {
            AggregateState::Count { count, .. } => {
                Ok(Value { kind: ValueKind::Integer(*count) })
            }
            AggregateState::Sum { sum, .. } => {
                match sum {
                    Some(s) => {
                        // Return as integer if it's a whole number, otherwise as float
                        if s.fract() == 0.0 && *s <= i64::MAX as f64 && *s >= i64::MIN as f64 {
                            Ok(Value { kind: ValueKind::Integer(*s as i64) })
                        } else {
                            Ok(Value { kind: ValueKind::Float(*s) })
                        }
                    }
                    None => Ok(Value { kind: ValueKind::Null(NullValue) }),
                }
            }
            AggregateState::Avg { sum, count, .. } => {
                if *count == 0 {
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    match sum {
                        Some(s) => {
                            let avg = s / *count as f64;
                            // Return as integer if it's a whole number
                            if avg.fract() == 0.0 && avg <= i64::MAX as f64 && avg >= i64::MIN as f64 {
                                Ok(Value { kind: ValueKind::Integer(avg as i64) })
                            } else {
                                Ok(Value { kind: ValueKind::Float(avg) })
                            }
                        }
                        None => Ok(Value { kind: ValueKind::Null(NullValue) }),
                    }
                }
            }
            AggregateState::Min { min } => {
                match min {
                    Some(m) => Ok(m.clone()),
                    None => Ok(Value { kind: ValueKind::Null(NullValue) }),
                }
            }
            AggregateState::Max { max } => {
                match max {
                    Some(m) => Ok(m.clone()),
                    None => Ok(Value { kind: ValueKind::Null(NullValue) }),
                }
            }
        }
    }

    /// Extract numeric value from a Value for SUM and AVG
    fn extract_numeric(value: &Value) -> Option<f64> {
        match &value.kind {
            ValueKind::Integer(i) => Some(*i as f64),
            ValueKind::Float(f) => Some(*f),
            ValueKind::Null(_) => None,
            _ => None, // Non-numeric types for SUM/AVG return NULL
        }
    }
}

/// Compare two Values for ordering (used by aggregate functions)
fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (&a.kind, &b.kind) {
        (ValueKind::Null(_), _) => std::cmp::Ordering::Less,
        (_, ValueKind::Null(_)) => std::cmp::Ordering::Greater,
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

/// Expression evaluator
pub struct ExpressionEvaluator;

impl ExpressionEvaluator {
    /// Create a new expression evaluator
    pub fn new() -> Self {
        Self
    }

    /// Evaluate an expression in the given context
    pub fn evaluate(&self, expr: &Expression, context: &EvaluationContext) -> Result<Value> {
        match expr {
            Expression::Value(value) => Ok(value.clone()),
            Expression::Literal(value) => Ok(value.clone()),

            Expression::Column { table, name } => {
                if let Some(table_alias) = table {
                    // Qualified column: construct qualified name and look for it
                    let qualified_name = format!("{}.{}", table_alias, name);

                    // First try the columns map with qualified name
                    if let Some(value) = context.get_column_value(&qualified_name) {
                        return Ok(value.clone());
                    }

                    // Fallback: if the qualified name is not found, try to find it using a heuristic
                    // This handles cases where JOIN creates "left.name" but the query uses "e.name"
                    // First, try to find "left.name" or "right.name" as fallbacks
                    let left_fallback = format!("left.{}", name);
                    let right_fallback = format!("right.{}", name);

                    if let Some(value) = context.get_column_value(&left_fallback) {
                        return Ok(value.clone());
                    }

                    if let Some(value) = context.get_column_value(&right_fallback) {
                        return Ok(value.clone());
                    }

                    // General fallback: try to find the column with any table alias prefix
                    // This handles cases like "d.id" where "d" is an arbitrary table alias
                    for (column_name, value) in &context.columns {
                        // Check if this column matches our target column name (ignoring table alias)
                        if let Some(pos) = column_name.find('.') {
                            let column_part = &column_name[pos + 1..];
                            if column_part == name {
                                return Ok(value.clone());
                            }
                        }
                    }

                    // Fallback: search left columns first, then right (for backwards compatibility)
                    if let (Some(left_row), Some(left_columns)) = (&context.left_row, &context.left_columns) {
                        if let Some(idx) = left_columns.iter().position(|c| c == name) {
                            return Ok(left_row[idx].clone());
                        }
                    }
                    if let (Some(right_row), Some(right_columns)) = (&context.right_row, &context.right_columns) {
                        if let Some(idx) = right_columns.iter().position(|c| c == name) {
                            return Ok(right_row[idx].clone());
                        }
                    }

                    // Final fallback: try unqualified name in columns map
                    // This handles cases like SELECT e.name FROM e where table scan produces unqualified column names
                    if let Some(value) = context.get_column_value(name) {
                        return Ok(value.clone());
                    }

                    // Not found
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    // Unqualified column: check columns map first
                    if let Some(value) = context.get_column_value(name) {
                        Ok(value.clone())
                    } else if let Some(row_data) = &context.row_data {
                        // Fallback to row_data if available
                        if let Some(idx) = context.columns.keys().position(|k| k == name) {
                            Ok(row_data[idx].clone())
                        } else {
                            Ok(Value { kind: ValueKind::Null(NullValue) })
                        }
                    } else {
                        Ok(Value { kind: ValueKind::Null(NullValue) })
                    }
                }
            }

            Expression::BinaryOp { left, op, right } => {
                // Special handling for IN operations with subqueries
                if *op == BinaryOperator::In {
                    // Evaluate left side normally
                    let left_val = self.evaluate(left, context)?;

                    // For IN operations, check if right side is a subquery
                    if let Expression::Subquery(_) = right.as_ref() {
                        // Create a new context with IN clause subquery context
                        let mut in_context = context.clone();
                        in_context.set_subquery_context(SubqueryContext::InClause);
                        let right_val = self.evaluate(right, &in_context)?;
                        Self::evaluate_binary_operation(*op, &left_val, &right_val)
                    } else {
                        // Evaluate right side normally for non-subquery IN operations
                        let right_val = self.evaluate(right, context)?;
                        Self::evaluate_binary_operation(*op, &left_val, &right_val)
                    }
                } else {
                    // For all other operations, evaluate normally
                    let left_val = self.evaluate(left, context)?;
                    let right_val = self.evaluate(right, context)?;
                    Self::evaluate_binary_operation(*op, &left_val, &right_val)
                }
            }

            Expression::UnaryOp { op, expr: inner_expr } => {
                let inner_val = self.evaluate(inner_expr, context)?;
                Self::evaluate_unary_operation(*op, &inner_val)
            }

            Expression::Function { name, args, distinct: _ } => {
                self.evaluate_function(name, args, context)
            }

            Expression::List(values) => {
                let mut evaluated_values = Vec::new();
                for value in values {
                    evaluated_values.push(self.evaluate(value, context)?);
                }
                Ok(Value::list(evaluated_values))
            }

            Expression::Star => {
                // STAR should be handled at a higher level
                Err(RustgreSQLError::InvalidOperation("STAR expression not supported in this context".to_string()))
            }

            Expression::Parameter(_) => {
                // Parameters are not yet supported
                Err(RustgreSQLError::InvalidOperation("Parameters not yet supported".to_string()))
            }

            Expression::Subquery(subquery_stmt) => {
                // Execute the subquery and return the result
                self.evaluate_subquery(subquery_stmt.as_ref(), context)
            }

            Expression::WindowFunction(_) => {
                // Window functions should be handled by WindowOperator, not in expression evaluator
                Err(RustgreSQLError::InvalidOperation("Window functions must be evaluated by WindowOperator".to_string()))
            }

            Expression::Exists { subquery, negated } => {
                // Execute the subquery and check if it returns any rows
                self.evaluate_exists_subquery(subquery.as_ref(), *negated, context)
            }
        }
    }

    /// Evaluate a binary operation with proper type checking and NULL handling
    fn evaluate_binary_operation(op: BinaryOperator, left: &Value, right: &Value) -> Result<Value> {
        // Handle NULL values in three-valued logic
        // IS and IS NOT operators handle NULLs specifically, so skip this check for them
        if op != BinaryOperator::Is && op != BinaryOperator::IsNot && 
           (matches!(&left.kind, ValueKind::Null(_)) || matches!(&right.kind, ValueKind::Null(_))) {
            return Self::handle_null_in_binary_operation(op);
        }

        match op {
            // Arithmetic operators
            BinaryOperator::Add => Self::evaluate_addition(left, right),
            BinaryOperator::Subtract => Self::evaluate_subtraction(left, right),
            BinaryOperator::Multiply => Self::evaluate_multiplication(left, right),
            BinaryOperator::Divide => Self::evaluate_division(left, right),

            // Comparison operators
            BinaryOperator::Equals => Self::evaluate_equals(left, right),
            BinaryOperator::NotEquals => Self::evaluate_not_equals(left, right),
            BinaryOperator::LessThan => Self::evaluate_less_than(left, right),
            BinaryOperator::LessThanOrEquals => Self::evaluate_less_than_or_equals(left, right),
            BinaryOperator::GreaterThan => Self::evaluate_greater_than(left, right),
            BinaryOperator::GreaterThanOrEquals => Self::evaluate_greater_than_or_equals(left, right),

            // Logical operators
            BinaryOperator::And => {
                let left_logic = ThreeValuedLogic::from_value(left);
                let right_logic = ThreeValuedLogic::from_value(right);
                Ok(left_logic.and(right_logic).to_value())
            }
            BinaryOperator::Or => {
                let left_logic = ThreeValuedLogic::from_value(left);
                let right_logic = ThreeValuedLogic::from_value(right);
                Ok(left_logic.or(right_logic).to_value())
            }

            // Other operators
            BinaryOperator::Like => Self::evaluate_like(left, right),
            BinaryOperator::ILike => Self::evaluate_ilike(left, right),
            BinaryOperator::In => Self::evaluate_in(left, right),
            BinaryOperator::Is => Self::evaluate_is(left, right),
            BinaryOperator::IsNot => Self::evaluate_is_not(left, right),
        }
    }

    /// Evaluate a unary operation
    fn evaluate_unary_operation(op: UnaryOperator, operand: &Value) -> Result<Value> {
        match op {
            UnaryOperator::Not => {
                let logic = ThreeValuedLogic::from_value(operand);
                Ok(logic.not().to_value())
            }
            UnaryOperator::Minus => {
                match &operand.kind {
                    ValueKind::Integer(i) => Ok(Value { kind: ValueKind::Integer(-i) }),
                    ValueKind::Float(f) => Ok(Value { kind: ValueKind::Float(-f) }),
                    ValueKind::Null(_) => Ok(Value { kind: ValueKind::Null(NullValue) }),
                    _ => Err(RustgreSQLError::InvalidOperation("Unary minus not supported for this type".to_string())),
                }
            }
            UnaryOperator::Plus => {
                // Unary plus is a no-op for numeric types
                match &operand.kind {
                    ValueKind::Integer(_) | ValueKind::Float(_) => Ok(operand.clone()),
                    ValueKind::Null(_) => Ok(Value { kind: ValueKind::Null(NullValue) }),
                    _ => Err(RustgreSQLError::InvalidOperation("Unary plus not supported for this type".to_string())),
                }
            }
            UnaryOperator::Exists => {
                // EXISTS should be handled as part of EXISTS expression evaluation
                Err(RustgreSQLError::InvalidOperation("EXISTS should be handled as EXISTS expression, not as unary operator".to_string()))
            }
            UnaryOperator::NotExists => {
                // NOT EXISTS should be handled as part of EXISTS expression evaluation
                Err(RustgreSQLError::InvalidOperation("NOT EXISTS should be handled as EXISTS expression, not as unary operator".to_string()))
            }
        }
    }

    /// Handle NULL values in binary operations according to SQL three-valued logic
    fn handle_null_in_binary_operation(op: BinaryOperator) -> Result<Value> {
        match op {
            // Logical operators return UNKNOWN if either operand is NULL
            BinaryOperator::And | BinaryOperator::Or => {
                Ok(ThreeValuedLogic::Unknown.to_value())
            }
            // Comparison operators return UNKNOWN if either operand is NULL
            BinaryOperator::Equals | BinaryOperator::NotEquals |
            BinaryOperator::LessThan | BinaryOperator::LessThanOrEquals |
            BinaryOperator::GreaterThan | BinaryOperator::GreaterThanOrEquals => {
                Ok(ThreeValuedLogic::Unknown.to_value())
            }
            // Arithmetic operators return NULL if either operand is NULL
            BinaryOperator::Add | BinaryOperator::Subtract |
            BinaryOperator::Multiply | BinaryOperator::Divide => {
                Ok(Value { kind: ValueKind::Null(NullValue) })
            }
            // Other operators
            _ => Ok(Value { kind: ValueKind::Null(NullValue) }),
        }
    }

    // Arithmetic operations
    fn evaluate_addition(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Integer(l + r) })
            }
            (ValueKind::Float(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(l + r) })
            }
            (ValueKind::Integer(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(*l as f64 + r) })
            }
            (ValueKind::Float(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Float(l + *r as f64) })
            }
            (ValueKind::String(l), ValueKind::String(r)) => {
                Ok(Value { kind: ValueKind::String(format!("{}{}", l, r)) })
            }
            _ => Err(RustgreSQLError::Type("Invalid types for addition".to_string())),
        }
    }

    fn evaluate_subtraction(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Integer(l - r) })
            }
            (ValueKind::Float(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(l - r) })
            }
            (ValueKind::Integer(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(*l as f64 - r) })
            }
            (ValueKind::Float(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Float(l - *r as f64) })
            }
            _ => Err(RustgreSQLError::Type("Invalid types for subtraction".to_string())),
        }
    }

    fn evaluate_multiplication(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Integer(l * r) })
            }
            (ValueKind::Float(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(l * r) })
            }
            (ValueKind::Integer(l), ValueKind::Float(r)) => {
                Ok(Value { kind: ValueKind::Float(*l as f64 * r) })
            }
            (ValueKind::Float(l), ValueKind::Integer(r)) => {
                Ok(Value { kind: ValueKind::Float(l * *r as f64) })
            }
            _ => Err(RustgreSQLError::Type("Invalid types for multiplication".to_string())),
        }
    }

    fn evaluate_division(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => {
                if *r == 0 {
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    Ok(Value { kind: ValueKind::Integer(l / r) })
                }
            }
            (ValueKind::Float(l), ValueKind::Float(r)) => {
                if *r == 0.0 {
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    Ok(Value { kind: ValueKind::Float(l / r) })
                }
            }
            (ValueKind::Integer(l), ValueKind::Float(r)) => {
                if *r == 0.0 {
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    Ok(Value { kind: ValueKind::Float(*l as f64 / r) })
                }
            }
            (ValueKind::Float(l), ValueKind::Integer(r)) => {
                if *r == 0 {
                    Ok(Value { kind: ValueKind::Null(NullValue) })
                } else {
                    Ok(Value { kind: ValueKind::Float(l / *r as f64) })
                }
            }
            _ => Err(RustgreSQLError::Type("Invalid types for division".to_string())),
        }
    }

    // Comparison operations
    fn evaluate_equals(left: &Value, right: &Value) -> Result<Value> {
        let result = match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => l == r,
            (ValueKind::Float(l), ValueKind::Float(r)) => l == r,
            (ValueKind::String(l), ValueKind::String(r)) => l == r,
            (ValueKind::Boolean(l), ValueKind::Boolean(r)) => l == r,
            (ValueKind::Integer(l), ValueKind::Float(r)) => (*l as f64 - r).abs() < f64::EPSILON,
            (ValueKind::Float(l), ValueKind::Integer(r)) => (l - *r as f64).abs() < f64::EPSILON,
            _ => false,
        };
        Ok(Value { kind: ValueKind::Boolean(result) })
    }

    fn evaluate_not_equals(left: &Value, right: &Value) -> Result<Value> {
        let equals_result = Self::evaluate_equals(left, right)?;
        if let ValueKind::Boolean(result) = equals_result.kind {
            Ok(Value { kind: ValueKind::Boolean(!result) })
        } else {
            Err(RustgreSQLError::Execution("NOT EQUALS operation failed".to_string()))
        }
    }

    fn evaluate_less_than(left: &Value, right: &Value) -> Result<Value> {
        let result = match (&left.kind, &right.kind) {
            (ValueKind::Integer(l), ValueKind::Integer(r)) => l < r,
            (ValueKind::Float(l), ValueKind::Float(r)) => l < r,
            (ValueKind::String(l), ValueKind::String(r)) => l < r,
            (ValueKind::Boolean(l), ValueKind::Boolean(r)) => {
                // FALSE < TRUE in SQL boolean ordering
                !l && *r
            }
            (ValueKind::Integer(l), ValueKind::Float(r)) => (*l as f64) < *r,
            (ValueKind::Float(l), ValueKind::Integer(r)) => *l < (*r as f64),
            _ => false,
        };
        Ok(Value { kind: ValueKind::Boolean(result) })
    }

    fn evaluate_less_than_or_equals(left: &Value, right: &Value) -> Result<Value> {
        let less_than = Self::evaluate_less_than(left, right)?;
        let equals = Self::evaluate_equals(left, right)?;

        if let (ValueKind::Boolean(lt), ValueKind::Boolean(eq)) = (less_than.kind, equals.kind) {
            Ok(Value { kind: ValueKind::Boolean(lt || eq) })
        } else {
            Err(RustgreSQLError::Execution("LESS THAN OR EQUALS operation failed".to_string()))
        }
    }

    fn evaluate_greater_than(left: &Value, right: &Value) -> Result<Value> {
        let less_than_or_equals = Self::evaluate_less_than_or_equals(left, right)?;
        if let ValueKind::Boolean(lte) = less_than_or_equals.kind {
            Ok(Value { kind: ValueKind::Boolean(!lte) })
        } else {
            Err(RustgreSQLError::Execution("GREATER THAN operation failed".to_string()))
        }
    }

    fn evaluate_greater_than_or_equals(left: &Value, right: &Value) -> Result<Value> {
        let less_than = Self::evaluate_less_than(left, right)?;
        if let ValueKind::Boolean(lt) = less_than.kind {
            Ok(Value { kind: ValueKind::Boolean(!lt) })
        } else {
            Err(RustgreSQLError::Execution("GREATER THAN OR EQUALS operation failed".to_string()))
        }
    }

    // String operations
    fn evaluate_like(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::String(left_str), ValueKind::String(pattern)) => {
                // Convert SQL LIKE pattern to regex
                let regex_pattern = pattern
                    .replace('%', ".*")
                    .replace('_', ".");

                let regex = regex::Regex::new(&format!("^{}$", regex_pattern))
                    .map_err(|e| RustgreSQLError::InvalidOperation(format!("Invalid LIKE pattern: {}", e)))?;

                let result = regex.is_match(left_str);
                Ok(Value { kind: ValueKind::Boolean(result) })
            }
            _ => Err(RustgreSQLError::Type("LIKE operation requires string operands".to_string())),
        }
    }

    fn evaluate_ilike(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::String(left_str), ValueKind::String(pattern)) => {
                // Convert SQL ILIKE pattern to regex (case insensitive)
                let regex_pattern = pattern
                    .replace('%', ".*")
                    .replace('_', ".");

                let regex = regex::Regex::new(&format!("(?i)^{}$", regex_pattern))
                    .map_err(|e| RustgreSQLError::InvalidOperation(format!("Invalid ILIKE pattern: {}", e)))?;

                let result = regex.is_match(left_str);
                Ok(Value { kind: ValueKind::Boolean(result) })
            }
            _ => Err(RustgreSQLError::Type("ILIKE operation requires string operands".to_string())),
        }
    }

    fn evaluate_in(left: &Value, right: &Value) -> Result<Value> {
        match &right.kind {
            ValueKind::List(values) => {
                for value in values {
                    let equals_result = Self::evaluate_equals(left, value)?;
                    if let ValueKind::Boolean(true) = equals_result.kind {
                        return Ok(Value { kind: ValueKind::Boolean(true) });
                    }
                }
                Ok(Value { kind: ValueKind::Boolean(false) })
            }
            ValueKind::String(right_str) => {
                if let ValueKind::String(left_str) = &left.kind {
                    let result = left_str == right_str;
                    Ok(Value { kind: ValueKind::Boolean(result) })
                } else {
                    Ok(Value { kind: ValueKind::Boolean(false) })
                }
            }
            _ => Err(RustgreSQLError::Type("IN operation requires a list or subquery result".to_string())),
        }
    }

    fn evaluate_is(left: &Value, right: &Value) -> Result<Value> {
        match (&left.kind, &right.kind) {
            (ValueKind::Null(_), ValueKind::Null(_)) => {
                Ok(Value { kind: ValueKind::Boolean(true) })
            }
            (ValueKind::Null(_), _) => {
                Ok(Value { kind: ValueKind::Boolean(false) })
            }
            (_, ValueKind::Null(_)) => {
                Ok(Value { kind: ValueKind::Boolean(false) })
            }
            _ => {
                // For non-NULL values, use equality
                Self::evaluate_equals(left, right)
            }
        }
    }

    fn evaluate_is_not(left: &Value, right: &Value) -> Result<Value> {
        let is_result = Self::evaluate_is(left, right)?;
        if let ValueKind::Boolean(result) = is_result.kind {
            Ok(Value { kind: ValueKind::Boolean(!result) })
        } else {
            Err(RustgreSQLError::Execution("IS operation failed".to_string()))
        }
    }

    // Function evaluation
    fn evaluate_function(&self, name: &str, args: &[Expression], context: &EvaluationContext) -> Result<Value> {
        match name.to_uppercase().as_str() {
            // Aggregate functions - check if they're available in HAVING context
            "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" => {
                // Check if we have aggregate values available for HAVING clause evaluation
                if let Some(ref having_aggregates) = context.having_aggregates {
                    // Create a string representation of the function call to use as key
                    let mut key_parts = vec![name.to_uppercase()];
                    for arg in args {
                        match arg {
                            Expression::Column { name, .. } => key_parts.push(name.to_uppercase()),
                            Expression::Literal(_) => key_parts.push("LITERAL".to_string()),
                            _ => key_parts.push("EXPR".to_string()),
                        }
                    }
                    let key = format!("{}({})", key_parts[0], key_parts[1..].join(","));

                    if let Some(value) = having_aggregates.get(&key) {
                        return Ok(value.clone());
                    }
                }

                Err(RustgreSQLError::InvalidOperation(
                    format!("Aggregate function '{}' cannot be evaluated directly. Use GROUP BY or aggregate context.", name)
                ))
            }

            // Scalar functions
            "ABS" => {
                if args.len() != 1 {
                    return Err(RustgreSQLError::InvalidOperation("ABS function requires exactly 1 argument".to_string()));
                }
                let arg = self.evaluate(&args[0], context)?;
                match arg.kind {
                    ValueKind::Integer(i) => Ok(Value { kind: ValueKind::Integer(i.abs()) }),
                    ValueKind::Float(f) => Ok(Value { kind: ValueKind::Float(f.abs()) }),
                    _ => Err(RustgreSQLError::Type("ABS function requires numeric argument".to_string())),
                }
            }
            "COALESCE" => {
                if args.is_empty() {
                    return Err(RustgreSQLError::InvalidOperation("COALESCE function requires at least 1 argument".to_string()));
                }
                for arg in args {
                    let value = self.evaluate(arg, context)?;
                    if !matches!(value.kind, ValueKind::Null(_)) {
                        return Ok(value);
                    }
                }
                Ok(Value { kind: ValueKind::Null(NullValue) })
            }
            "LENGTH" => {
                if args.len() != 1 {
                    return Err(RustgreSQLError::InvalidOperation("LENGTH function requires exactly 1 argument".to_string()));
                }
                let arg = self.evaluate(&args[0], context)?;
                match arg.kind {
                    ValueKind::String(s) => Ok(Value { kind: ValueKind::Integer(s.len() as i64) }),
                    _ => Err(RustgreSQLError::Type("LENGTH function requires string argument".to_string())),
                }
            }
            _ => Err(RustgreSQLError::InvalidOperation(format!("Unknown function: {}", name))),
        }
    }

    /// Find correlated columns in a subquery by checking which column references
    /// are not available in the subquery's own FROM clause
    fn find_correlated_columns_in_subquery(&self, subquery_stmt: &crate::sql::ast::Statement, _context: &EvaluationContext) -> Result<Vec<String>> {
        let mut correlated_columns = Vec::new();

        if let crate::sql::ast::Statement::Select(select_stmt) = subquery_stmt {
            // Handle both Simple and SetOperation variants
            if let crate::sql::ast::SelectStatement::Simple {
                with_clause: _, from, columns, where_clause, having, ..
            } = select_stmt {
                // Get all tables available in the subquery's FROM clause
                let mut subquery_tables = std::collections::HashSet::new();
                for table_ref in from {
                    match table_ref {
                        crate::sql::ast::TableRef::Table { name, alias } => {
                            // Add the base table name
                            subquery_tables.insert(name.clone());

                            // Add the alias if present
                            if let Some(ref table_alias) = alias {
                                subquery_tables.insert(table_alias.clone());
                            }
                        },
                        crate::sql::ast::TableRef::Subquery { alias, .. } => {
                            // For subqueries, add the alias if present
                            if let Some(ref table_alias) = alias {
                                subquery_tables.insert(table_alias.clone());
                                // Debug log
                                // subquery_context.log(&format!("DEBUG: Added subquery alias '{}' to subquery_tables", table_alias));
                            }
                        }
                    };
                }

                // Debug log
                // subquery_context.log(&format!("DEBUG: Final subquery_tables: {:?}", subquery_tables));

                // Collect all column references in the subquery
                let mut all_columns = Vec::new();

                // Check columns in SELECT list
                for col_spec in columns {
                    self.collect_column_references(&col_spec.expr, &mut all_columns);
                }

                // Check columns in WHERE clause
                if let Some(where_clause) = where_clause {
                    self.collect_column_references(where_clause, &mut all_columns);
                }

                // Check columns in HAVING clause
                if let Some(having_clause) = having {
                    self.collect_column_references(having_clause, &mut all_columns);
                }

                // Check if any column references don't belong to subquery tables
                // For now, we'll use a simplified heuristic: if a column reference has a table qualifier
                // that's not in the subquery's FROM clause, it's correlated
                for column in all_columns {
                    if let crate::sql::ast::Expression::Column { table: Some(table_name), name } = column {
                        if !subquery_tables.contains(&table_name) {
                            correlated_columns.push(format!("{}.{}", table_name, name));
                        }
                    }
                }
            }
            // TODO: Handle SetOperation variant for correlated subqueries
        }

        Ok(correlated_columns)
    }

    /// Collect all column references from an expression recursively
    fn collect_column_references(&self, expr: &crate::sql::ast::Expression, columns: &mut Vec<crate::sql::ast::Expression>) {
        match expr {
            crate::sql::ast::Expression::Column { .. } => {
                columns.push(expr.clone());
            }
            crate::sql::ast::Expression::BinaryOp { left, right, .. } => {
                self.collect_column_references(left, columns);
                self.collect_column_references(right, columns);
            }
            crate::sql::ast::Expression::UnaryOp { expr, .. } => {
                self.collect_column_references(expr, columns);
            }
            crate::sql::ast::Expression::Function { args, .. } => {
                for arg in args {
                    self.collect_column_references(arg, columns);
                }
            }
            crate::sql::ast::Expression::Subquery(_) => {
                // Don't recurse into subqueries for correlation detection
                // They should be handled independently
            }
            crate::sql::ast::Expression::Exists { subquery, .. } => {
                // Don't recurse into EXISTS subqueries for correlation detection
                // They should be handled independently
                std::mem::drop(subquery); // Suppress unused warning
            }
            _ => {}
        }
    }

    /// Evaluate a subquery expression
    fn evaluate_subquery(&self, subquery_stmt: &crate::sql::ast::Statement, context: &EvaluationContext) -> Result<Value> {
        // Detect correlated columns in the subquery
        let correlated_columns = self.find_correlated_columns_in_subquery(subquery_stmt, context)?;

        // Create subquery operator with correlation info
        let subquery_op = crate::executor::operators::SubqueryOperator::new(
            subquery_stmt.clone(),
            correlated_columns.clone(),
        );

        // Create a new execution context for the subquery
        let mut subquery_context = crate::executor::operators::ExecutionContext::new();

        // Pass catalog, buffer_manager, and materialized_ctes from outer context so subquery can access database and CTEs
        if let Some(catalog) = &context.catalog {
            subquery_context.set_catalog(catalog.clone());
        }
        if let Some(buffer_manager) = &context.buffer_manager {
            subquery_context.set_buffer_manager(buffer_manager.clone());
        }
        if let Some(materialized_ctes) = context.get_materialized_ctes() {
            subquery_context.set_materialized_ctes(materialized_ctes.clone());
        }

        // Execute the subquery (correlated or non-correlated)
        let result = if !correlated_columns.is_empty() {
            subquery_op.execute_correlated(context, &mut subquery_context)?
        } else {
            subquery_op.execute(&mut subquery_context)?
        };

        // Handle subquery results based on context
        match context.get_subquery_context() {
            Some(SubqueryContext::InClause) => {
                // For IN clauses, convert all rows to a list of values from the first column
                if result.rows.is_empty() {
                    // Empty subquery result - return empty list for IN clause
                    Ok(Value { kind: ValueKind::List(Vec::new()) })
                } else {
                    // Convert first column of each row to a list
                    let values: Vec<Value> = result.rows.iter()
                        .map(|row| row[0].clone())  // Take first column of each row
                        .collect();
                    Ok(Value { kind: ValueKind::List(values) })
                }
            }
            Some(SubqueryContext::Exists) => {
                // For EXISTS clauses, return boolean based on whether any rows were returned
                Ok(Value { kind: ValueKind::Boolean(!result.rows.is_empty()) })
            }
            Some(SubqueryContext::Scalar) | None => {
                // For scalar subqueries or default context, return the first column of the first row
                if result.rows.is_empty() {
                    // Empty subquery result
                    Ok(Value { kind: ValueKind::Null(crate::types::NullValue) })
                } else if result.rows.len() == 1 && result.column_names.len() == 1 {
                    // Scalar subquery - return the single value
                    Ok(result.rows[0][0].clone())
                } else {
                    // Multiple rows or columns but used in scalar context - return first value
                    // This maintains backwards compatibility for existing code
                    Ok(result.rows[0][0].clone())
                }
            }
        }
    }

    /// Evaluate EXISTS subquery - returns TRUE if subquery returns any rows, FALSE otherwise
    fn evaluate_exists_subquery(&self, subquery_stmt: &crate::sql::ast::Statement, negated: bool, context: &EvaluationContext) -> Result<Value> {
        // Create a temporary context with EXISTS subquery context
        let mut exists_context = context.clone();
        exists_context.set_subquery_context(SubqueryContext::Exists);

        // Use the existing subquery evaluation logic with EXISTS context
        let exists_result = self.evaluate_subquery(subquery_stmt, &exists_context)?;

        // Apply negation if needed
        if negated {
            match &exists_result.kind {
                ValueKind::Boolean(exists) => Ok(Value { kind: ValueKind::Boolean(!exists) }),
                _ => Err(crate::error::RustgreSQLError::InvalidOperation("EXISTS subquery did not return boolean value".to_string())),
            }
        } else {
            Ok(exists_result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ValueKind;

    #[test]
    fn test_three_valued_logic() {
        let true_val = Value { kind: ValueKind::Boolean(true) };
        let false_val = Value { kind: ValueKind::Boolean(false) };
        let null_val = Value { kind: ValueKind::Null(NullValue) };

        assert_eq!(ThreeValuedLogic::from_value(&true_val), ThreeValuedLogic::True);
        assert_eq!(ThreeValuedLogic::from_value(&false_val), ThreeValuedLogic::False);
        assert_eq!(ThreeValuedLogic::from_value(&null_val), ThreeValuedLogic::Unknown);

        // Test AND logic
        assert_eq!(ThreeValuedLogic::True.and(ThreeValuedLogic::True), ThreeValuedLogic::True);
        assert_eq!(ThreeValuedLogic::True.and(ThreeValuedLogic::False), ThreeValuedLogic::False);
        assert_eq!(ThreeValuedLogic::True.and(ThreeValuedLogic::Unknown), ThreeValuedLogic::Unknown);
        assert_eq!(ThreeValuedLogic::Unknown.and(ThreeValuedLogic::True), ThreeValuedLogic::Unknown);

        // Test OR logic
        assert_eq!(ThreeValuedLogic::True.or(ThreeValuedLogic::False), ThreeValuedLogic::True);
        assert_eq!(ThreeValuedLogic::False.or(ThreeValuedLogic::False), ThreeValuedLogic::False);
        assert_eq!(ThreeValuedLogic::False.or(ThreeValuedLogic::Unknown), ThreeValuedLogic::Unknown);

        // Test NOT logic
        assert_eq!(ThreeValuedLogic::True.not(), ThreeValuedLogic::False);
        assert_eq!(ThreeValuedLogic::False.not(), ThreeValuedLogic::True);
        assert_eq!(ThreeValuedLogic::Unknown.not(), ThreeValuedLogic::Unknown);
    }

    #[test]
    fn test_arithmetic_operations() {
        let left = Value { kind: ValueKind::Integer(5) };
        let right = Value { kind: ValueKind::Integer(3) };

        let result = ExpressionEvaluator::evaluate_binary_operation(BinaryOperator::Add, &left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Integer(8));

        let result = ExpressionEvaluator::evaluate_binary_operation(BinaryOperator::Multiply, &left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Integer(15));
    }

    #[test]
    fn test_comparison_operations() {
        let left = Value { kind: ValueKind::Integer(5) };
        let right = Value { kind: ValueKind::Integer(3) };

        let result = ExpressionEvaluator::evaluate_binary_operation(BinaryOperator::GreaterThan, &left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true));

        let result = ExpressionEvaluator::evaluate_binary_operation(BinaryOperator::Equals, &left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false));
    }

    #[test]
    fn test_in_clause_with_literal_list() {
        let left = Value { kind: ValueKind::Integer(2) };
        let right = Value { kind: ValueKind::List(vec![
            Value { kind: ValueKind::Integer(1) },
            Value { kind: ValueKind::Integer(2) },
            Value { kind: ValueKind::Integer(3) },
        ])};

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true));

        // Test with value not in list
        let left = Value { kind: ValueKind::Integer(5) };
        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false));
    }

    #[test]
    fn test_in_clause_with_string_values() {
        let left = Value { kind: ValueKind::String("Engineering".to_string()) };
        let right = Value { kind: ValueKind::List(vec![
            Value { kind: ValueKind::String("Sales".to_string()) },
            Value { kind: ValueKind::String("Engineering".to_string()) },
            Value { kind: ValueKind::String("Marketing".to_string()) },
        ])};

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true));

        // Test with string not in list
        let left = Value { kind: ValueKind::String("Finance".to_string()) };
        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false));
    }

    #[test]
    fn test_in_clause_with_empty_list() {
        let left = Value { kind: ValueKind::Integer(1) };
        let right = Value { kind: ValueKind::List(vec![]) };

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false));
    }

    #[test]
    fn test_in_clause_with_single_string() {
        // This tests the old behavior for single string values
        let left = Value { kind: ValueKind::String("test".to_string()) };
        let right = Value { kind: ValueKind::String("test".to_string()) };

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true));

        // Test with different strings
        let left = Value { kind: ValueKind::String("different".to_string()) };
        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false));
    }

    #[test]
    fn test_subquery_context_creation() {
        let context = EvaluationContext::with_in_subquery_context();
        assert!(matches!(context.get_subquery_context(), Some(SubqueryContext::InClause)));
    }

    #[test]
    fn test_subquery_context_setting() {
        let mut context = EvaluationContext::new();
        assert_eq!(context.get_subquery_context(), None);

        context.set_subquery_context(SubqueryContext::Scalar);
        assert!(matches!(context.get_subquery_context(), Some(SubqueryContext::Scalar)));

        context.set_subquery_context(SubqueryContext::Exists);
        assert!(matches!(context.get_subquery_context(), Some(SubqueryContext::Exists)));
    }

    #[test]
    fn test_in_clause_error_handling() {
        let left = Value { kind: ValueKind::Integer(1) };
        let right = Value { kind: ValueKind::Float(1.0) };

        let result = ExpressionEvaluator::evaluate_in(&left, &right);
        assert!(result.is_err());

        if let Err(RustgreSQLError::Type(msg)) = result {
            assert!(msg.contains("IN operation requires a list or subquery result"));
        } else {
            panic!("Expected Type error for unsupported IN operation");
        }
    }

    #[test]
    fn test_mixed_types_in_in_clause() {
        // Test with integer and float values
        let left = Value { kind: ValueKind::Integer(2) };
        let right = Value { kind: ValueKind::List(vec![
            Value { kind: ValueKind::Integer(1) },
            Value { kind: ValueKind::Float(2.0) },  // Should match due to float comparison
            Value { kind: ValueKind::Integer(3) },
        ])};

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true));
    }

    #[test]
    fn test_null_values_in_in_clause() {
        let left = Value { kind: ValueKind::Integer(1) };
        let right = Value { kind: ValueKind::List(vec![
            Value { kind: ValueKind::Integer(1) },
            Value { kind: ValueKind::Null(NullValue) },
            Value { kind: ValueKind::Integer(2) },
        ])};

        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(true)); // Should find the match before NULL

        // Test when value is NULL
        let left = Value { kind: ValueKind::Null(NullValue) };
        let result = ExpressionEvaluator::evaluate_in(&left, &right).unwrap();
        assert_eq!(result.kind, ValueKind::Boolean(false)); // NULL should not match anything
    }
}