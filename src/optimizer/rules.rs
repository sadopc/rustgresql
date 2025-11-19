//! Query optimization rules
//!
//! Provides various optimization rules for query planning.

use crate::{
    Result,
    sql::ast::{Expression, BinaryOperator, UnaryOperator},
    executor::planner::PlanNode,
    types::{Value, ValueKind},
    optimizer::cost_model::{CostModel, ParallelCostEstimate, CostEstimate},
};

/// Optimization rule trait
pub trait OptimizerRule {
    /// Apply the rule to a plan node
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode>;
    /// Get rule name
    fn name(&self) -> &'static str;
    /// Check if rule should be applied
    fn is_applicable(&self, plan: &PlanNode) -> bool {
        true // Default: always applicable
    }
}

/// Rule engine for applying optimization rules
pub struct RuleEngine {
    rules: Vec<Box<dyn OptimizerRule>>,
    iteration_limit: usize,
    enable_iteration: bool,
}

impl std::fmt::Debug for RuleEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleEngine")
            .field("rules_count", &self.rules.len())
            .field("iteration_limit", &self.iteration_limit)
            .field("enable_iteration", &self.enable_iteration)
            .finish()
    }
}

impl RuleEngine {
    /// Create new rule engine
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            iteration_limit: 10,
            enable_iteration: true,
        }
    }

    /// Create rule engine with custom configuration
    pub fn with_config(iteration_limit: usize, enable_iteration: bool) -> Self {
        Self {
            rules: Vec::new(),
            iteration_limit,
            enable_iteration,
        }
    }

    /// Add rule to engine
    pub fn add_rule(&mut self, rule: Box<dyn OptimizerRule>) {
        self.rules.push(rule);
    }

    /// Apply all rules to plan (with fixpoint iteration)
    pub fn optimize(&self, initial_plan: &PlanNode) -> Result<PlanNode> {
        let mut current_plan = initial_plan.clone();
        let mut iteration_count = 0;

        if self.enable_iteration {
            // Apply rules iteratively until fixpoint
            loop {
                let old_plan = current_plan.clone();
                let mut changed = false;

                for rule in &self.rules {
                    if rule.is_applicable(&current_plan) {
                        let new_plan = rule.apply(&current_plan)?;
                        if !self.plans_equal(&current_plan, &new_plan) {
                            current_plan = new_plan;
                            changed = true;
                        }
                    }
                }

                iteration_count += 1;
                if !changed || iteration_count >= self.iteration_limit {
                    break;
                }
            }
        } else {
            // Single pass application
            for rule in &self.rules {
                if rule.is_applicable(&current_plan) {
                    current_plan = rule.apply(&current_plan)?;
                }
            }
        }

        Ok(current_plan)
    }

    /// Apply single rule
    pub fn apply_rule(&self, plan: &PlanNode, rule_index: usize) -> Result<PlanNode> {
        if rule_index < self.rules.len() {
            self.rules[rule_index].apply(plan)
        } else {
            Ok(plan.clone())
        }
    }

    /// Get all rules
    pub fn rules(&self) -> &[Box<dyn OptimizerRule>] {
        &self.rules
    }

    /// Check if two plans are equal (simplified)
    fn plans_equal(&self, plan1: &PlanNode, plan2: &PlanNode) -> bool {
        // This is a simplified equality check
        // In a real implementation, this would be more sophisticated
        std::mem::discriminant(plan1) == std::mem::discriminant(plan2)
    }
}

/// Predicate pushdown rule
#[derive(Debug)]
pub struct PredicatePushdownRule;

impl OptimizerRule for PredicatePushdownRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                // Try to push filter down to the input plan
                let pushed_down = self.try_pushdown_filter(input.as_ref(), condition.clone())?;
                match pushed_down {
                    Some(new_input) => {
                        // Successfully pushed down, need to adjust the condition
                        self.apply(&new_input)
                    }
                    None => {
                        // Cannot push down, keep original structure
                        let new_input = self.apply(input.as_ref())?;
                        Ok(PlanNode::Filter {
                            input: Box::new(new_input),
                            condition: self.optimize_expression(condition)?.unwrap_or_else(|| condition.clone()),
                        })
                    }
                }
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                // Try to push conditions down to join inputs
                let (left_condition, right_condition, remaining_condition) = self.classify_join_conditions(condition);

                let optimized_left = if let Some(left_cond) = left_condition {
                    let left_with_filter = PlanNode::Filter {
                        input: left.clone(),
                        condition: left_cond,
                    };
                    self.apply(&left_with_filter)?
                } else {
                    self.apply(left.as_ref())?
                };

                let optimized_right = if let Some(right_cond) = right_condition {
                    let right_with_filter = PlanNode::Filter {
                        input: right.clone(),
                        condition: right_cond,
                    };
                    self.apply(&right_with_filter)?
                } else {
                    self.apply(right.as_ref())?
                };

                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: remaining_condition.and_then(|expr| self.optimize_expression(&expr).ok()).flatten(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => self.apply_to_children(plan),
        }
    }

    fn name(&self) -> &'static str {
        "PredicatePushdown"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        matches!(plan, PlanNode::Filter { .. } | PlanNode::Join { .. })
    }
}

impl PredicatePushdownRule {
    fn try_pushdown_filter(&self, input: &PlanNode, condition: Expression) -> Result<Option<PlanNode>> {
        match input {
            PlanNode::Scan { table_name, columns, .. } => {
                // Can always push filter to scan
                Ok(Some(PlanNode::Scan {
                    table_name: table_name.clone(),
                    columns: columns.clone(),
                    alias: None,
                }))
            }
            PlanNode::IndexScan { table_name, index_name, columns, .. } => {
                // Can push filter to index scan if it's indexable
                if self.is_condition_indexable(&condition) {
                    Ok(Some(PlanNode::IndexScan {
                        table_name: table_name.clone(),
                        index_name: index_name.clone(),
                        index_condition: Some(condition),
                        columns: columns.clone(),
                    }))
                } else {
                    // Can't push down, would need residual filter
                    Ok(None)
                }
            }
            PlanNode::IndexOnlyScan { table_name, index_name, columns, .. } => {
                // Can push filter to index-only scan if it's indexable
                if self.is_condition_indexable(&condition) {
                    Ok(Some(PlanNode::IndexOnlyScan {
                        table_name: table_name.clone(),
                        index_name: index_name.clone(),
                        index_condition: Some(condition),
                        columns: columns.clone(),
                    }))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None), // Cannot push down through other plan types
        }
    }

    fn is_condition_indexable(&self, condition: &Expression) -> bool {
        match condition {
            Expression::BinaryOp { op, .. } => {
                matches!(op, BinaryOperator::Equals | BinaryOperator::LessThan |
                           BinaryOperator::LessThanOrEquals | BinaryOperator::GreaterThan |
                           BinaryOperator::GreaterThanOrEquals)
            }
            _ => false,
        }
    }

    fn classify_join_conditions(&self, condition: &Option<Expression>) -> (Option<Expression>, Option<Expression>, Option<Expression>) {
        match condition {
            Some(expr) => {
                // Simplified: if it's an AND condition, try to split it
                if let Expression::BinaryOp { left, op: BinaryOperator::And, right } = expr {
                    // Try to determine which side belongs to which table
                    // This is very simplified - a real implementation would analyze column names
                    if self.contains_only_left_columns(left) {
                        (Some(*left.clone()), None, Some(*right.clone()))
                    } else if self.contains_only_left_columns(right) {
                        (Some(*right.clone()), None, Some(*left.clone()))
                    } else {
                        (None, None, Some(expr.clone()))
                    }
                } else {
                    (None, None, Some(expr.clone()))
                }
            }
            None => (None, None, None),
        }
    }

    fn contains_only_left_columns(&self, _expr: &Expression) -> bool {
        // Simplified - in a real implementation, this would analyze column names
        // For now, assume it's always true for demonstration
        true
    }

    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: self.optimize_expression(condition)?.unwrap_or_else(|| condition.clone()),
                })
            }
            PlanNode::Project { input, columns, .. } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Project {
                    input: Box::new(optimized_input),
                    columns: columns.clone(),
                    table_aliases: std::collections::HashMap::new(),
                    left_columns: None,
                    right_columns: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: match condition {
                Some(expr) => Some(self.optimize_expression(expr)?.unwrap_or_else(|| expr.clone())),
                None => None,
            },
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => Ok(plan.clone()),
        }
    }

    fn optimize_expression(&self, expr: &Expression) -> Result<Option<Expression>> {
        // Apply constant folding to expressions
        let constant_folder = ConstantFoldingRule;
        constant_folder.apply_to_expression(expr)
    }
}

/// Projection pushdown rule
#[derive(Debug)]
pub struct ProjectionPushdownRule;

impl OptimizerRule for ProjectionPushdownRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Project { input, columns, .. } => {
                // Try to push projection down to the input
                let required_columns: Vec<String> = columns.iter()
                    .filter_map(|(name, expr)| {
                        if let Expression::Column { name: col_name, .. } = expr {
                            Some(col_name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                if self.can_pushdown_projection(input.as_ref(), &required_columns) {
                    let pushed_down = self.pushdown_projection(input.as_ref(), &required_columns)?;
                    match pushed_down {
                        Some(new_input) => {
                            // Successfully pushed down
                            self.apply(&new_input)
                        }
                        None => {
                            // Cannot push down further
                            let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Project {
                    input: Box::new(optimized_input),
                    columns: columns.clone(),
                    table_aliases: std::collections::HashMap::new(),
                    left_columns: None,
                    right_columns: None,
                })
                        }
                    }
                } else {
                    // Cannot push down
                    let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Project {
                    input: Box::new(optimized_input),
                    columns: columns.clone(),
                    table_aliases: std::collections::HashMap::new(),
                    left_columns: None,
                    right_columns: None,
                })
                }
            }
            _ => self.apply_to_children(plan),
        }
    }

    fn name(&self) -> &'static str {
        "ProjectionPushdown"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        matches!(plan, PlanNode::Project { .. })
    }
}

impl ProjectionPushdownRule {
    fn can_pushdown_projection(&self, input: &PlanNode, required_columns: &[String]) -> bool {
        match input {
            PlanNode::Scan { .. } => true, // Can always push down to scan
            PlanNode::IndexScan { .. } => true, // Can push down to index scan
            PlanNode::IndexOnlyScan { .. } => true, // Can push down to index-only scan
            PlanNode::Filter { .. } => true, // Can push down past filter
            _ => false, // Cannot push down through other plan types
        }
    }

    fn pushdown_projection(&self, input: &PlanNode, required_columns: &[String]) -> Result<Option<PlanNode>> {
        match input {
            PlanNode::Scan { table_name, .. } => {
                Ok(Some(PlanNode::Scan {
                    table_name: table_name.clone(),
                    columns: required_columns.to_vec(),
                    alias: None,
                }))
            }
            PlanNode::IndexScan { table_name, index_name, index_condition, .. } => {
                Ok(Some(PlanNode::IndexScan {
                    table_name: table_name.clone(),
                    index_name: index_name.clone(),
                    index_condition: index_condition.clone(),
                    columns: required_columns.to_vec(),
                }))
            }
            PlanNode::IndexOnlyScan { table_name, index_name, index_condition, .. } => {
                Ok(Some(PlanNode::IndexOnlyScan {
                    table_name: table_name.clone(),
                    index_name: index_name.clone(),
                    index_condition: index_condition.clone(),
                    columns: required_columns.to_vec(),
                }))
            }
            PlanNode::Filter { input, condition } => {
                if let Some(new_input) = self.pushdown_projection(input.as_ref(), required_columns)? {
                    Ok(Some(PlanNode::Filter {
                        input: Box::new(new_input),
                        condition: condition.clone(),
                    }))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: condition.clone(),
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => Ok(plan.clone()),
        }
    }
}

/// Constant folding rule
#[derive(Debug)]
pub struct ConstantFoldingRule;

impl OptimizerRule for ConstantFoldingRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        self.apply_to_children(plan)
    }

    fn name(&self) -> &'static str {
        "ConstantFolding"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        // Constant folding is always applicable to optimize expressions
        true
    }
}

impl ConstantFoldingRule {
    pub fn apply_to_expression(&self, expr: &Expression) -> Result<Option<Expression>> {
        match expr {
            Expression::BinaryOp { left, op, right } => {
                let optimized_left = self.apply_to_expression(left.as_ref())?;
                let optimized_right = self.apply_to_expression(right.as_ref())?;

                let folded = if let (Some(Expression::Value(left_val)), Some(Expression::Value(right_val))) =
                    (optimized_left.as_ref(), optimized_right.as_ref())
                {
                    self.fold_binary_operation(*op, left_val, right_val)
                } else {
                    None
                };

                if let Some(folded_expr) = folded {
                    Ok(Some(folded_expr))
                } else {
                    Ok(Some(Expression::BinaryOp {
                        left: optimized_left.map(Box::new).unwrap_or_else(|| left.clone()),
                        op: *op,
                        right: optimized_right.map(Box::new).unwrap_or_else(|| right.clone()),
                    }))
                }
            }
            Expression::UnaryOp { op, expr } => {
                let optimized_expr = self.apply_to_expression(expr.as_ref())?;

                if let Some(Expression::Value(val)) = optimized_expr.as_ref() {
                    let folded = self.fold_unary_operation(*op, val);
                    if let Some(folded_expr) = folded {
                        return Ok(Some(folded_expr));
                    }
                }

                Ok(Some(Expression::UnaryOp {
                    op: *op,
                    expr: optimized_expr.map(Box::new).unwrap_or_else(|| expr.clone()),
                }))
            }
            Expression::Function { args, .. } => {
                let mut optimized_args = Vec::new();
                let mut changed = false;

                for arg in args {
                    if let Some(optimized_arg) = self.apply_to_expression(arg)? {
                        optimized_args.push(optimized_arg);
                        changed = true;
                    } else {
                        optimized_args.push(arg.clone());
                    }
                }

                if changed {
                    Ok(Some(Expression::Function {
                        name: "temp".to_string(), // Placeholder
                        args: optimized_args,
                    }))
                } else {
                    Ok(None)
                }
            }
            Expression::List(expressions) => {
                let mut optimized_expressions = Vec::new();
                let mut changed = false;

                for expr in expressions {
                    if let Some(optimized_expr) = self.apply_to_expression(expr)? {
                        optimized_expressions.push(optimized_expr);
                        changed = true;
                    } else {
                        optimized_expressions.push(expr.clone());
                    }
                }

                if changed {
                    Ok(Some(Expression::List(optimized_expressions)))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None), // Cannot fold other expressions
        }
    }

    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                let optimized_condition = self.apply_to_expression(condition)?
                    .unwrap_or_else(|| condition.clone());
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: optimized_condition,
                })
            }
            PlanNode::Project { input, columns, .. } => {
                let optimized_input = self.apply(input.as_ref())?;
                let mut optimized_columns = Vec::new();
                let mut changed = false;

                for (name, expr) in columns {
                    if let Some(optimized_expr) = self.apply_to_expression(expr)? {
                        optimized_columns.push((name.clone(), optimized_expr));
                        changed = true;
                    } else {
                        optimized_columns.push((name.clone(), expr.clone()));
                    }
                }

                    Ok(PlanNode::Project {
                        input: Box::new(optimized_input),
                        columns: columns.clone(),
                        table_aliases: std::collections::HashMap::new(),
                        left_columns: None,
                        right_columns: None,
                    })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                let optimized_condition = condition.as_ref()
                    .and_then(|expr| self.apply_to_expression(expr).ok().flatten());

                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: optimized_condition,
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => Ok(plan.clone()),
        }
    }

    fn fold_binary_operation(&self, op: BinaryOperator, left: &crate::types::Value, right: &crate::types::Value) -> Option<Expression> {

        match (&left.kind, &right.kind, op) {
            // Integer arithmetic
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::Add) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Integer(a + b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::Subtract) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Integer(a - b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::Multiply) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Integer(a * b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::Divide) => {
                if *b != 0 { Some(Expression::Value(crate::types::Value { kind: ValueKind::Integer(a / b) })) } else { None }
            },

            // Float operations
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::Add) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Float(a + b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::Subtract) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Float(a - b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::Multiply) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Float(a * b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::Divide) => {
                if *b != 0.0 { Some(Expression::Value(crate::types::Value { kind: ValueKind::Float(a / b) })) } else { None }
            },

            // Mixed type operations (promote to higher precision)
            (ValueKind::Integer(a), ValueKind::Float(b), _) => {
                let real_left = crate::types::Value { kind: ValueKind::Float(*a as f64) };
                self.fold_binary_operation(op, &real_left, right)
            },
            (ValueKind::Float(a), ValueKind::Integer(b), _) => {
                let real_right = crate::types::Value { kind: ValueKind::Float(*b as f64) };
                self.fold_binary_operation(op, left, &real_right)
            },

            // Boolean operations
            (ValueKind::Boolean(a), ValueKind::Boolean(b), BinaryOperator::And) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(*a && *b) })),
            (ValueKind::Boolean(a), ValueKind::Boolean(b), BinaryOperator::Or) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(*a || *b) })),

            // Comparison operations
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::Equals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a == b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::NotEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a != b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::LessThan) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a < b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::LessThanOrEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a <= b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::GreaterThan) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a > b) })),
            (ValueKind::Integer(a), ValueKind::Integer(b), BinaryOperator::GreaterThanOrEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a >= b) })),

            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::Equals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean((a - b).abs() < f64::EPSILON) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::NotEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean((a - b).abs() >= f64::EPSILON) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::LessThan) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a < b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::LessThanOrEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a <= b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::GreaterThan) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a > b) })),
            (ValueKind::Float(a), ValueKind::Float(b), BinaryOperator::GreaterThanOrEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a >= b) })),

            (ValueKind::String(a), ValueKind::String(b), BinaryOperator::Equals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a == b) })),
            (ValueKind::String(a), ValueKind::String(b), BinaryOperator::NotEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a != b) })),

            (ValueKind::Boolean(a), ValueKind::Boolean(b), BinaryOperator::Equals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a == b) })),
            (ValueKind::Boolean(a), ValueKind::Boolean(b), BinaryOperator::NotEquals) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(a != b) })),

            _ => None,
        }
    }

    fn fold_unary_operation(&self, op: UnaryOperator, operand: &crate::types::Value) -> Option<Expression> {
        match (op, &operand.kind) {
            (UnaryOperator::Not, ValueKind::Boolean(value)) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Boolean(!value) })),
            (UnaryOperator::Minus, ValueKind::Integer(value)) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Integer(-value) })),
            (UnaryOperator::Minus, ValueKind::Float(value)) => Some(Expression::Value(crate::types::Value { kind: ValueKind::Float(-value) })),
            (UnaryOperator::Plus, _) => Some(Expression::Value(operand.clone())),
            _ => None,
        }
    }
}

/// Aggregation pushdown optimization rule
///
/// This rule optimizes queries by:
/// 1. Pushing aggregation operations earlier in the execution pipeline
/// 2. Optimizing join-aggregation patterns
/// 3. Reducing intermediate result sizes through early aggregation
#[derive(Debug)]
pub struct AggregationPushdownRule;

impl OptimizerRule for AggregationPushdownRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            // Case 1: Join followed by Aggregation - try to push aggregation down
            PlanNode::Join { left, right, condition, join_type, .. } => {
                if self.can_pushdown_aggregation_through_join(left.as_ref(), right.as_ref()) {
                    // Transform: Join -> Aggregate into Aggregate -> Join where possible
                    if let Some(optimized_plan) = self.optimize_join_aggregation(left.as_ref(), right.as_ref(), condition, join_type.clone())? {
                        return Ok(optimized_plan);
                    }
                }

                // Apply rule to children if no pushdown possible
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }

            // Case 2: Project followed by Aggregation - eliminate unnecessary projections
            PlanNode::Project { input, columns, .. } => {
                if let PlanNode::Aggregate { .. } = input.as_ref() {
                    // Remove redundant projection before aggregation
                    self.apply(input.as_ref())
                 } else {
                    // Apply to input and keep projection
                    let optimized_input = self.apply(input.as_ref())?;
                    Ok(PlanNode::Project {
                        input: Box::new(optimized_input),
                        columns: columns.clone(),
                        table_aliases: std::collections::HashMap::new(),
                        left_columns: None,
                        right_columns: None,
                    })
                }
            }

            // Case 3: Filter followed by Aggregation - try to push filter below aggregation
            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                if let PlanNode::Filter { input: filter_input, condition } = input.as_ref() {
                    // Check if filter can be pushed below aggregation
                    if self.can_pushdown_filter_below_aggregation(condition, group_by_columns) {
                        let optimized_filter_input = self.apply(filter_input.as_ref())?;
                        let pushed_down_filter = PlanNode::Filter {
                            input: Box::new(optimized_filter_input),
                            condition: condition.clone(),
                        };
                        return Ok(PlanNode::Aggregate {
                            input: Box::new(pushed_down_filter),
                            group_by_columns: group_by_columns.clone(),
                            aggregate_functions: aggregate_functions.clone(),
                            having_clause: having_clause.clone(),
                        });
                    }
                }

                // Apply to input
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Aggregate {
                    input: Box::new(optimized_input),
                    group_by_columns: group_by_columns.clone(),
                    aggregate_functions: aggregate_functions.clone(),
                    having_clause: having_clause.clone(),
                })
            }

            // Apply to children for other plan nodes
            _ => self.apply_to_children(plan),
        }
    }

    fn name(&self) -> &'static str {
        "AggregationPushdown"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        match plan {
            PlanNode::Join { .. } | PlanNode::Aggregate { .. } | PlanNode::Project { .. } => true,
            _ => false,
        }
    }
}

impl AggregationPushdownRule {
    /// Check if aggregation can be pushed down through a join
    fn can_pushdown_aggregation_through_join(&self, left: &PlanNode, right: &PlanNode) -> bool {
        // Can push down if one side of the join is already aggregated
        // and the join condition doesn't interfere with the aggregation
        matches!(left, PlanNode::Aggregate { .. }) || matches!(right, PlanNode::Aggregate { .. })
    }

    /// Optimize join-aggregation patterns
    fn optimize_join_aggregation(
        &self,
        left: &PlanNode,
        right: &PlanNode,
        condition: &Option<Expression>,
        join_type: crate::sql::ast::JoinType,
    ) -> Result<Option<PlanNode>> {
        // Pattern 1: (Table A) JOIN (Table B) GROUP BY A.id, B.id
        // If possible, transform to: (Table A GROUP BY A.id) JOIN (Table B GROUP BY B.id)

        // This is a simplified implementation - a full implementation would need to:
        // 1. Analyze the aggregation expressions to see if they only reference one table
        // 2. Check if the join condition references only the grouping columns
        // 3. Ensure the transformation is semantically equivalent

        // For now, return None to indicate no transformation applied
        // A production implementation would analyze the plan structure in detail
        Ok(None)
    }

    /// Check if filter can be pushed below aggregation
    fn can_pushdown_filter_below_aggregation(&self, condition: &Expression, group_by_columns: &[Expression]) -> bool {
        // Filter can be pushed below if:
        // 1. It only references GROUP BY columns, or
        // 2. It can be evaluated before aggregation without changing semantics

        match condition {
            Expression::BinaryOp { left, right, .. } => {
                self.is_group_by_expression(left, group_by_columns) &&
                self.is_group_by_expression(right, group_by_columns)
            }
            Expression::Column { .. } => {
                self.is_group_by_expression(condition, group_by_columns)
            }
            _ => false,
        }
    }

    /// Check if expression is a GROUP BY expression
    fn is_group_by_expression(&self, expr: &Expression, group_by_columns: &[Expression]) -> bool {
        group_by_columns.iter().any(|group_expr| {
            self.expressions_equal(expr, group_expr)
        })
    }

    /// Check if two expressions are equal (simplified)
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

    /// Apply rule to children nodes
    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: condition.clone(),
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Project { input, columns, .. } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Project {
                    input: Box::new(optimized_input),
                    columns: columns.clone(),
                    table_aliases: std::collections::HashMap::new(),
                    left_columns: None,
                    right_columns: None,
                })
            }
            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Aggregate {
                    input: Box::new(optimized_input),
                    group_by_columns: group_by_columns.clone(),
                    aggregate_functions: aggregate_functions.clone(),
                    having_clause: having_clause.clone(),
                })
            }
            _ => Ok(plan.clone()),
        }
    }
}

/// Parallel plan selection rule
///
/// This rule decides whether to use parallel execution for various plan nodes
/// based on the extended cost model with parallel support.
#[derive(Debug)]
pub struct ParallelPlanSelectionRule {
    cost_model: CostModel,
}

impl ParallelPlanSelectionRule {
    /// Create new parallel plan selection rule
    pub fn new(cost_model: CostModel) -> Self {
        Self { cost_model }
    }
}

impl OptimizerRule for ParallelPlanSelectionRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        // First apply to children
        let optimized_plan = self.apply_to_children(plan)?;

        // Then consider parallel execution for this node
        match &optimized_plan {
            PlanNode::Scan { table_name, columns: _, .. } => {
                // Get table statistics (simplified - in real implementation would query catalog)
                let table_stats = self.get_table_statistics(table_name)?;

                if table_stats.row_count > 10000 {
                    // Consider parallel scan for large tables
                    let parallel_cost = self.cost_model.estimate_parallel_seq_scan(
                        table_stats.page_count,
                        table_stats.row_count
                    );

                    if self.cost_model.should_use_parallel(&parallel_cost) {
                        // Create parallel scan node - would need to extend PlanNode enum
                        // For now, return the original plan
                        Ok(optimized_plan)
                    } else {
                        Ok(optimized_plan)
                    }
                } else {
                    Ok(optimized_plan)
                }
            }

            PlanNode::Join { left, right, condition: _, join_type: _, .. } => {
                // Consider parallel hash join
                if self.can_use_parallel_hash_join(&**left, &**right) {
                    let left_stats = self.estimate_plan_statistics(&**left)?;
                    let right_stats = self.estimate_plan_statistics(&**right)?;

                    // Create cost estimates for left and right sides
                    let left_cost = CostEstimate::new(
                        left_stats.estimated_rows as f64 * 0.1, // IO cost
                        left_stats.estimated_rows as f64 * 0.01, // CPU cost
                        left_stats.estimated_rows as f64 * 0.001, // Memory cost
                    );
                    let right_cost = CostEstimate::new(
                        right_stats.estimated_rows as f64 * 0.1,
                        right_stats.estimated_rows as f64 * 0.01,
                        right_stats.estimated_rows as f64 * 0.001,
                    );

                    let parallel_cost = self.cost_model.estimate_parallel_hash_join(
                        left_stats.estimated_rows,
                        right_stats.estimated_rows,
                        left_cost,
                        right_cost,
                        0.1, // join selectivity
                    );

                    if self.cost_model.should_use_parallel(&parallel_cost) {
                        // Create parallel hash join node - would need to extend PlanNode enum
                        // For now, return the original plan
                        Ok(optimized_plan)
                    } else {
                        Ok(optimized_plan)
                    }
                } else {
                    Ok(optimized_plan)
                }
            }

            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause: _ } => {
                // Consider parallel aggregation
                if group_by_columns.len() > 0 || !aggregate_functions.is_empty() {
                    let input_stats = self.estimate_plan_statistics(&**input)?;
                    let group_count_estimate = (input_stats.estimated_rows as f64 * 0.1) as usize;

                    // Create input cost estimate
                    let input_cost = CostEstimate::new(
                        input_stats.estimated_rows as f64 * 0.1, // IO cost
                        input_stats.estimated_rows as f64 * 0.01, // CPU cost
                        input_stats.estimated_rows as f64 * 0.001, // Memory cost
                    );

                    let parallel_cost = self.cost_model.estimate_parallel_aggregation(
                        input_cost,
                        input_stats.estimated_rows,
                        group_count_estimate,
                        aggregate_functions.len(),
                    );

                    if self.cost_model.should_use_parallel(&parallel_cost) {
                        // Create parallel aggregate node - would need to extend PlanNode enum
                        // For now, return the original plan
                        Ok(optimized_plan)
                    } else {
                        Ok(optimized_plan)
                    }
                } else {
                    Ok(optimized_plan)
                }
            }

            _ => Ok(optimized_plan),
        }
    }

    fn name(&self) -> &'static str {
        "ParallelPlanSelection"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        matches!(plan, PlanNode::Scan { .. } | PlanNode::Join { .. } | PlanNode::Aggregate { .. })
    }
}

impl ParallelPlanSelectionRule {
    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: condition.clone(),
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Aggregate {
                    input: Box::new(optimized_input),
                    group_by_columns: group_by_columns.clone(),
                    aggregate_functions: aggregate_functions.clone(),
                    having_clause: having_clause.clone(),
                })
            }
            _ => Ok(plan.clone()),
        }
    }

    fn get_table_statistics(&self, table_name: &str) -> Result<TableStatistics> {
        // Simplified implementation - in real system would query catalog
        match table_name {
            "users" => Ok(TableStatistics {
                name: table_name.to_string(),
                row_count: 50000,
                page_count: 1000,
                avg_row_size: 200,
            }),
            "orders" => Ok(TableStatistics {
                name: table_name.to_string(),
                row_count: 250000,
                page_count: 5000,
                avg_row_size: 300,
            }),
            "products" => Ok(TableStatistics {
                name: table_name.to_string(),
                row_count: 10000,
                page_count: 200,
                avg_row_size: 150,
            }),
            _ => Ok(TableStatistics {
                name: table_name.to_string(),
                row_count: 1000,
                page_count: 20,
                avg_row_size: 100,
            }),
        }
    }

    fn can_use_parallel_hash_join(&self, left: &PlanNode, right: &PlanNode) -> bool {
        // Simplified heuristic: use parallel hash join for large result sets
        let left_stats = self.estimate_plan_statistics(left).unwrap_or_default();
        let right_stats = self.estimate_plan_statistics(right).unwrap_or_default();

        left_stats.estimated_rows > 5000 && right_stats.estimated_rows > 5000
    }

    fn estimate_plan_statistics(&self, plan: &PlanNode) -> Result<PlanStatistics> {
        match plan {
            PlanNode::Scan { table_name, .. } => {
                let table_stats = self.get_table_statistics(table_name)?;
                Ok(PlanStatistics {
                    estimated_rows: table_stats.row_count,
                    estimated_width: table_stats.avg_row_size,
                })
            }
            PlanNode::Filter { input, condition } => {
                let input_stats = self.estimate_plan_statistics(input)?;
                // Assume 50% selectivity for filters (simplified)
                Ok(PlanStatistics {
                    estimated_rows: input_stats.estimated_rows / 2,
                    estimated_width: input_stats.estimated_width,
                })
            }
            PlanNode::Join { left, right, .. } => {
                let left_stats = self.estimate_plan_statistics(left)?;
                let right_stats = self.estimate_plan_statistics(right)?;
                // Simplified join cardinality estimation
                Ok(PlanStatistics {
                    estimated_rows: (left_stats.estimated_rows * right_stats.estimated_rows) / 100,
                    estimated_width: left_stats.estimated_width + right_stats.estimated_width,
                })
            }
            _ => Ok(PlanStatistics {
                estimated_rows: 1000,
                estimated_width: 100,
            }),
        }
    }
}

/// Parallel join ordering rule
///
/// This rule optimizes join order considering parallel execution costs.
#[derive(Debug)]
pub struct ParallelJoinOrderingRule {
    cost_model: CostModel,
}

impl ParallelJoinOrderingRule {
    /// Create new parallel join ordering rule
    pub fn new(cost_model: CostModel) -> Self {
        Self { cost_model }
    }
}

impl OptimizerRule for ParallelJoinOrderingRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Join { left, right, condition, join_type, .. } => {
                // Try to optimize join order based on parallel execution costs
                if let Some(optimized_join) = self.optimize_join_order(left.as_ref(), right.as_ref(), condition, *join_type)? {
                    Ok(optimized_join)
                } else {
                    // Apply to children and keep current order
                    let optimized_left = self.apply(left.as_ref())?;
                    let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
                }
            }
            _ => self.apply_to_children(plan),
        }
    }

    fn name(&self) -> &'static str {
        "ParallelJoinOrdering"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        matches!(plan, PlanNode::Join { .. })
    }
}

impl ParallelJoinOrderingRule {
    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: condition.clone(),
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => Ok(plan.clone()),
        }
    }

    fn optimize_join_order(
        &self,
        left: &PlanNode,
        right: &PlanNode,
        condition: &Option<Expression>,
        join_type: crate::sql::ast::JoinType,
    ) -> Result<Option<PlanNode>> {
        // Simplified implementation: compare costs of current vs swapped order
        let left_stats = self.estimate_plan_statistics(left);
        let right_stats = self.estimate_plan_statistics(right);

        // Create cost estimates for both orders
        let left_cost = CostEstimate::new(
            left_stats.estimated_rows as f64 * 0.1, // IO cost
            left_stats.estimated_rows as f64 * 0.01, // CPU cost
            left_stats.estimated_rows as f64 * 0.001, // Memory cost
        );
        let right_cost = CostEstimate::new(
            right_stats.estimated_rows as f64 * 0.1,
            right_stats.estimated_rows as f64 * 0.01,
            right_stats.estimated_rows as f64 * 0.001,
        );

        // Calculate parallel join costs for both orders
        let cost_current = self.cost_model.estimate_parallel_hash_join(
            left_stats.estimated_rows,
            right_stats.estimated_rows,
            left_cost,
            right_cost,
            0.1, // join selectivity
        );

        let cost_swapped = self.cost_model.estimate_parallel_hash_join(
            right_stats.estimated_rows,
            left_stats.estimated_rows,
            right_cost,
            left_cost,
            0.1,
        );

        // If swapped order is significantly cheaper, swap the join
        if cost_swapped.parallel_cost.total_cost < cost_current.parallel_cost.total_cost * 0.9 {
            return Ok(Some(PlanNode::Join {
                left: Box::new(right.clone()),
                right: Box::new(left.clone()),
                condition: condition.clone(),
                join_type,
                left_alias: None,
                right_alias: None,
            }));
        }

        Ok(None)
    }

    fn estimate_plan_statistics(&self, plan: &PlanNode) -> PlanStatistics {
        // Simplified implementation
        match plan {
            PlanNode::Scan { table_name, .. } => {
                match table_name.as_str() {
                    "users" => PlanStatistics { estimated_rows: 50000, estimated_width: 200 },
                    "orders" => PlanStatistics { estimated_rows: 250000, estimated_width: 300 },
                    "products" => PlanStatistics { estimated_rows: 10000, estimated_width: 150 },
                    _ => PlanStatistics { estimated_rows: 1000, estimated_width: 100 },
                }
            }
            _ => PlanStatistics { estimated_rows: 1000, estimated_width: 100 },
        }
    }
}

/// Parallel aggregation optimization rule
///
/// This rule optimizes aggregation operations for parallel execution.
#[derive(Debug)]
pub struct ParallelAggregationOptimizationRule {
    cost_model: CostModel,
}

impl ParallelAggregationOptimizationRule {
    /// Create new parallel aggregation optimization rule
    pub fn new(cost_model: CostModel) -> Self {
        Self { cost_model }
    }
}

impl OptimizerRule for ParallelAggregationOptimizationRule {
    fn apply(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Aggregate { input, group_by_columns, aggregate_functions, having_clause } => {
                // First apply to input and keep regular aggregation
                let optimized_input = self.apply(input.as_ref())?;
                let optimized_plan = PlanNode::Aggregate {
                    input: Box::new(optimized_input),
                    group_by_columns: group_by_columns.clone(),
                    aggregate_functions: aggregate_functions.clone(),
                    having_clause: having_clause.clone(),
                };

                // Check if this aggregation can benefit from parallel execution
                if self.can_benefit_from_parallel_aggregation(&*input, &group_by_columns, &aggregate_functions) {
                    let input_stats = self.estimate_input_statistics(&*input)?;
                    let group_count = self.estimate_group_count(&input_stats, &group_by_columns);

                    // Create input cost estimate
                    let input_cost = CostEstimate::new(
                        input_stats.estimated_rows as f64 * 0.1, // IO cost
                        input_stats.estimated_rows as f64 * 0.01, // CPU cost
                        input_stats.estimated_rows as f64 * 0.001, // Memory cost
                    );

                    let parallel_cost = self.cost_model.estimate_parallel_aggregation(
                        input_cost,
                        input_stats.estimated_rows,
                        group_count,
                        aggregate_functions.len(),
                    );

                    if self.cost_model.should_use_parallel(&parallel_cost) {
                        // Create parallel aggregate node - would need to extend PlanNode enum
                        // For now, return the original plan since we can't create parallel nodes yet
                        Ok(optimized_plan)
                    } else {
                        Ok(optimized_plan)
                    }
                } else {
                    Ok(optimized_plan)
                }
            }
            _ => self.apply_to_children(plan),
        }
    }

    fn name(&self) -> &'static str {
        "ParallelAggregationOptimization"
    }

    fn is_applicable(&self, plan: &PlanNode) -> bool {
        matches!(plan, PlanNode::Aggregate { .. })
    }
}

impl ParallelAggregationOptimizationRule {
    fn apply_to_children(&self, plan: &PlanNode) -> Result<PlanNode> {
        match plan {
            PlanNode::Filter { input, condition } => {
                let optimized_input = self.apply(input.as_ref())?;
                Ok(PlanNode::Filter {
                    input: Box::new(optimized_input),
                    condition: condition.clone(),
                })
            }
            PlanNode::Join { left, right, condition, join_type, .. } => {
                let optimized_left = self.apply(left.as_ref())?;
                let optimized_right = self.apply(right.as_ref())?;
                Ok(PlanNode::Join {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition: condition.clone(),
                    join_type: join_type.clone(),
                    left_alias: None,
                    right_alias: None,
                })
            }
            _ => Ok(plan.clone()),
        }
    }

    fn can_benefit_from_parallel_aggregation(
        &self,
        input: &PlanNode,
        group_by_columns: &[Expression],
        aggregate_functions: &[(String, Expression)],
    ) -> bool {
        // Parallel aggregation is beneficial when:
        // 1. Input has many rows
        // 2. There are aggregation functions
        // 3. Either many groups or few expensive aggregations

        let input_stats = self.estimate_input_statistics(input).unwrap_or_default();
        let has_many_rows = input_stats.estimated_rows > 10000;
        let has_aggregations = !aggregate_functions.is_empty();
        let has_many_groups = group_by_columns.len() > 2 || self.estimate_group_count(&input_stats, group_by_columns) > 1000;
        let has_expensive_aggregations = aggregate_functions.iter().any(|(name, _expr)| {
            matches!(name.to_lowercase().as_str(), "avg" | "sum" | "stddev" | "variance" | "count")
        });

        has_many_rows && has_aggregations && (has_many_groups || has_expensive_aggregations)
    }

    fn estimate_input_statistics(&self, input: &PlanNode) -> Result<PlanStatistics> {
        // Simplified implementation
        match input {
            PlanNode::Scan { table_name, .. } => {
                match table_name.as_str() {
                    "users" => Ok(PlanStatistics { estimated_rows: 50000, estimated_width: 200 }),
                    "orders" => Ok(PlanStatistics { estimated_rows: 250000, estimated_width: 300 }),
                    "products" => Ok(PlanStatistics { estimated_rows: 10000, estimated_width: 150 }),
                    _ => Ok(PlanStatistics { estimated_rows: 1000, estimated_width: 100 }),
                }
            }
            PlanNode::Filter { input, .. } => {
                let input_stats = self.estimate_input_statistics(input.as_ref())?;
                Ok(PlanStatistics {
                    estimated_rows: input_stats.estimated_rows / 2,
                    estimated_width: input_stats.estimated_width,
                })
            }
            _ => Ok(PlanStatistics { estimated_rows: 1000, estimated_width: 100 }),
        }
    }

    fn estimate_group_count(&self, input_stats: &PlanStatistics, group_by_columns: &[Expression]) -> usize {
        // Simplified heuristic: estimate 10% of rows as unique groups
        // In real implementation would use statistics on columns
        (input_stats.estimated_rows as f64 * 0.1) as usize
    }
}

/// Supporting types for parallel optimization rules

#[derive(Debug, Clone, Default)]
struct TableStatistics {
    name: String,
    row_count: usize,
    page_count: usize,
    avg_row_size: usize,
}

#[derive(Debug, Clone, Default)]
struct PlanStatistics {
    estimated_rows: usize,
    estimated_width: usize,
}