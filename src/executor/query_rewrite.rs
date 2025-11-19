//! Query rewrite system for materialized views
//!
//! This module provides automatic query rewriting to use materialized views
//! when possible, improving query performance by avoiding repeated computation.

use crate::{Result, RustgreSQLError};
use crate::sql::ast::*;
use crate::catalog::view::{ViewDef, RefreshType, TableDependency};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

/// Query rewriter for materialized views
#[derive(Debug)]
pub struct QueryRewriter {
    /// Available materialized views indexed by their query patterns
    materialized_views: HashMap<String, ViewDef>,
    /// Query pattern cache for faster matching
    query_patterns: HashMap<String, QueryPattern>,
}

/// Query pattern for matching against materialized views
#[derive(Debug, Clone)]
struct QueryPattern {
    /// Base tables involved in the query
    base_tables: HashSet<String>,
    /// Join conditions between tables
    join_conditions: Vec<JoinCondition>,
    /// Filter predicates
    filters: Vec<Expression>,
    /// Group by columns
    group_by: Vec<Expression>,
    /// Aggregate functions
    aggregates: Vec<AggregateFunction>,
    /// Projected columns
    projections: Vec<ColumnSpec>,
}

/// Join condition for pattern matching
#[derive(Debug, Clone)]
struct JoinCondition {
    left_table: String,
    left_column: String,
    right_table: String,
    right_column: String,
    join_type: JoinType,
}

/// Aggregate function for pattern matching
#[derive(Debug, Clone)]
struct AggregateFunction {
    function_name: String,
    argument: Expression,
    distinct: bool,
}

/// Rewrite result containing the rewritten query and metadata
#[derive(Debug)]
pub struct RewriteResult {
    /// Rewritten query (if rewrite was possible)
    pub rewritten_query: Option<Statement>,
    /// Name of the materialized view used (if any)
    pub view_name: Option<String>,
    /// Whether the query was rewritten
    pub was_rewritten: bool,
    /// Estimated performance improvement
    pub performance_gain: f64,
    /// View freshness information
    pub view_freshness: Option<ViewFreshness>,
}

/// View freshness information
#[derive(Debug)]
pub struct ViewFreshness {
    /// Last refresh time
    pub last_refreshed: Option<SystemTime>,
    /// Whether the view is considered fresh enough
    pub is_fresh: bool,
    /// staleness in seconds
    pub staleness_seconds: u64,
}

impl QueryRewriter {
    /// Create a new query rewriter
    pub fn new() -> Self {
        Self {
            materialized_views: HashMap::new(),
            query_patterns: HashMap::new(),
        }
    }

    /// Register a materialized view for rewriting
    pub fn register_materialized_view(&mut self, view: ViewDef) -> Result<()> {
        if !view.materialized {
            return Err(RustgreSQLError::Internal(
                "Cannot register non-materialized view for query rewriting".to_string()
            ));
        }

        let pattern = self.extract_query_pattern(&view.query)?;
        self.query_patterns.insert(view.name.clone(), pattern);
        self.materialized_views.insert(view.name.clone(), view);

        Ok(())
    }

    /// Unregister a materialized view
    pub fn unregister_materialized_view(&mut self, view_name: &str) {
        self.materialized_views.remove(view_name);
        self.query_patterns.remove(view_name);
    }

    /// Rewrite a query to use materialized views if possible
    pub fn rewrite_query(&self, query: &Statement) -> Result<RewriteResult> {
        match query {
            Statement::Select(select) => self.rewrite_select_query(select),
            _ => Ok(RewriteResult {
                rewritten_query: None,
                view_name: None,
                was_rewritten: false,
                performance_gain: 0.0,
                view_freshness: None,
            })
        }
    }

    /// Rewrite a SELECT query
    fn rewrite_select_query(&self, select: &SelectStatement) -> Result<RewriteResult> {
        // Extract pattern from the incoming query
        let query_pattern = self.extract_select_pattern(select)?;

        // Find matching materialized views
        let matching_views = self.find_matching_views(&query_pattern)?;

        if matching_views.is_empty() {
            return Ok(RewriteResult {
                rewritten_query: None,
                view_name: None,
                was_rewritten: false,
                performance_gain: 0.0,
                view_freshness: None,
            });
        }

        // Select the best view based on performance and freshness
        let best_view = self.select_best_view(&matching_views, &query_pattern)?;

        // Rewrite the query using the selected view
        let rewritten_query = self.rewrite_using_view(select, &best_view)?;

        Ok(RewriteResult {
            rewritten_query: Some(rewritten_query),
            view_name: Some(best_view.name.clone()),
            was_rewritten: true,
            performance_gain: self.estimate_performance_gain(&query_pattern, &best_view),
            view_freshness: Some(self.check_view_freshness(&best_view)),
        })
    }

    /// Extract query pattern from SQL string
    fn extract_query_pattern(&self, sql: &str) -> Result<QueryPattern> {
        // Parse the SQL to extract the pattern
        let statements = crate::sql::parser::parse_sql(sql)?;

        if statements.is_empty() {
            return Err(RustgreSQLError::Internal(
                "Empty SQL statement".to_string()
            ));
        }

        match &statements[0] {
            Statement::Select(select) => self.extract_select_pattern(select),
            _ => Err(RustgreSQLError::Internal(
                "Only SELECT statements can be used for materialized view patterns".to_string()
            ))
        }
    }

    /// Extract pattern from SELECT statement
    fn extract_select_pattern(&self, select: &SelectStatement) -> Result<QueryPattern> {
        match select {
            SelectStatement::Simple {
                from,
                joins,
                where_clause,
                columns,
                group_by,
                ..
            } => {
                // Extract base tables
                let mut base_tables = HashSet::new();
                for table in from {
                    base_tables.insert(table.name.clone());
                }

                // Extract join conditions
                let mut join_conditions = Vec::new();
                for join in joins {
                    if let Some(condition) = &join.condition {
                        self.extract_join_condition(condition, &base_tables, &mut join_conditions)?;
                    }
                    base_tables.insert(join.table.name.clone());
                }

                // Extract filters
                let mut filters = Vec::new();
                if let Some(where_clause) = where_clause {
                    filters.push(where_clause.clone());
                }

                // Extract aggregates
                let mut aggregates = Vec::new();
                for col_spec in columns {
                    self.extract_aggregates(&col_spec.expr, &mut aggregates);
                }

                Ok(QueryPattern {
                    base_tables,
                    join_conditions,
                    filters,
                    group_by: group_by.clone(),
                    aggregates,
                    projections: columns.clone(),
                })
            }
            SelectStatement::SetOperation(_) => {
                Err(RustgreSQLError::Internal(
                    "Set operations are not supported for materialized view patterns".to_string()
                ))
            }
        }
    }

    /// Extract join conditions from an expression
    fn extract_join_condition(&self, expr: &Expression, base_tables: &HashSet<String>, join_conditions: &mut Vec<JoinCondition>) -> Result<()> {
        match expr {
            Expression::BinaryOp { left, op: BinaryOperator::Equals, right } => {
                if let (Expression::Column { name: left_col, table: left_table },
                     Expression::Column { name: right_col, table: right_table }) = (&**left, &**right) {

                    let left_table_name = left_table.as_ref().unwrap_or(&"".to_string()).clone();
                    let right_table_name = right_table.as_ref().unwrap_or(&"".to_string()).clone();

                    // Check if both tables are in our base tables
                    if base_tables.contains(&left_table_name) && base_tables.contains(&right_table_name) {
                        join_conditions.push(JoinCondition {
                            left_table: left_table_name,
                            left_column: left_col.clone(),
                            right_table: right_table_name,
                            right_column: right_col.clone(),
                            join_type: JoinType::Inner, // Default to inner join
                        });
                    }
                }
            }
            Expression::BinaryOp { left, op: _, right } => {
                self.extract_join_condition(left, base_tables, join_conditions)?;
                self.extract_join_condition(right, base_tables, join_conditions)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Extract aggregate functions from an expression
    fn extract_aggregates(&self, expr: &Expression, aggregates: &mut Vec<AggregateFunction>) {
        match expr {
            Expression::Function { name, args } => {
                let upper_name = name.to_uppercase();
                match upper_name.as_str() {
                    "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" => {
                        if let Some(arg) = args.first() {
                            aggregates.push(AggregateFunction {
                                function_name: upper_name,
                                argument: arg.clone(),
                                distinct: false, // TODO: Detect DISTINCT
                            });
                        }
                    }
                    _ => {
                        for arg in args {
                            self.extract_aggregates(arg, aggregates);
                        }
                    }
                }
            }
            Expression::BinaryOp { left, right, .. } => {
                self.extract_aggregates(left, aggregates);
                self.extract_aggregates(right, aggregates);
            }
            _ => {}
        }
    }

    /// Find materialized views that match the query pattern
    fn find_matching_views(&self, query_pattern: &QueryPattern) -> Result<Vec<&ViewDef>> {
        let mut matching_views = Vec::new();

        for (view_name, view_pattern) in &self.query_patterns {
            if self.pattern_matches(query_pattern, view_pattern) {
                if let Some(view_def) = self.materialized_views.get(view_name) {
                    matching_views.push(view_def);
                }
            }
        }

        Ok(matching_views)
    }

    /// Check if a query pattern matches a view pattern
    fn pattern_matches(&self, query: &QueryPattern, view: &QueryPattern) -> bool {
        // Check base tables
        if query.base_tables != view.base_tables {
            return false;
        }

        // Check join conditions
        if query.join_conditions.len() != view.join_conditions.len() {
            return false;
        }

        for query_join in &query.join_conditions {
            if !view.join_conditions.iter().any(|view_join| {
                query_join.left_table == view_join.left_table &&
                query_join.left_column == view_join.left_column &&
                query_join.right_table == view_join.right_table &&
                query_join.right_column == view_join.right_column
            }) {
                return false;
            }
        }

        // Check aggregates
        if query.aggregates.len() != view.aggregates.len() {
            return false;
        }

        for query_agg in &query.aggregates {
            if !view.aggregates.iter().any(|view_agg| {
                query_agg.function_name == view_agg.function_name &&
                self.expressions_equivalent(&query_agg.argument, &view_agg.argument)
            }) {
                return false;
            }
        }

        true
    }

    /// Check if two expressions are equivalent for matching purposes
    fn expressions_equivalent(&self, a: &Expression, b: &Expression) -> bool {
        match (a, b) {
            (Expression::Column { name: name_a, .. }, Expression::Column { name: name_b, .. }) => {
                name_a == name_b
            }
            (Expression::Value(val_a), Expression::Value(val_b)) => {
                format!("{:?}", val_a) == format!("{:?}", val_b)
            }
            (Expression::Function { name: name_a, args: args_a }, Expression::Function { name: name_b, args: args_b }) => {
                name_a.to_lowercase() == name_b.to_lowercase() &&
                args_a.len() == args_b.len() &&
                args_a.iter().zip(args_b.iter()).all(|(a_arg, b_arg)| self.expressions_equivalent(a_arg, b_arg))
            }
            (Expression::Star, Expression::Star) => true,
            _ => false,
        }
    }

    /// Select the best view from matching candidates
    fn select_best_view(&self, matching_views: &[&ViewDef], query_pattern: &QueryPattern) -> Result<ViewDef> {
        // For now, select the view with the best performance characteristics
        // In a more sophisticated implementation, this would consider:
        // - View freshness
        // - View size
        // - Query complexity reduction
        // - Statistics

        if matching_views.is_empty() {
            return Err(RustgreSQLError::Internal(
                "No matching views found".to_string()
            ));
        }

        let mut best_view = matching_views[0];
        let mut best_score = self.calculate_view_score(best_view, query_pattern);

        for view in &matching_views[1..] {
            let score = self.calculate_view_score(view, query_pattern);
            if score > best_score {
                best_score = score;
                best_view = view;
            }
        }

        Ok((*best_view).clone())
    }

    /// Calculate a score for a view to determine suitability
    fn calculate_view_score(&self, view: &ViewDef, _query_pattern: &QueryPattern) -> f64 {
        let mut score = 0.0;

        // Prefer views with more recent refreshes
        if let Some(last_refreshed) = view.last_refreshed {
            if let Ok(elapsed) = last_refreshed.elapsed() {
                score += (1.0 / (elapsed.as_secs() as f64 + 1.0)) * 100.0;
            }
        }

        // Prefer views that cover more complex queries
        score += view.dependencies.len() as f64 * 10.0;

        // Prefer materialized views over regular views
        if view.materialized {
            score += 50.0;
        }

        score
    }

    /// Rewrite a query using a specific materialized view
    fn rewrite_using_view(&self, select: &SelectStatement, view: &ViewDef) -> Result<Statement> {
        match select {
            SelectStatement::Simple { columns, where_clause, group_by, having, limit, offset, .. } => {
                // Create a new SELECT that uses the materialized view as the base table
                let rewritten_select = SelectStatement::Simple {
                    with_clause: None,
                    distinct: false,
                    from: vec![TableRef {
                        name: view.name.clone(),
                        alias: None,
                    }],
                    joins: vec![],
                    where_clause: where_clause.clone(), // Keep additional filters
                    columns: columns.clone(), // Keep same projections
                    group_by: group_by.clone(), // Keep grouping
                    having: having.clone(),
                    order_by: vec![],
                    limit: *limit,
                    offset: *offset,
                    named_windows: vec![],
                };

                Ok(Statement::Select(rewritten_select))
            }
            SelectStatement::SetOperation(_) => {
                Err(RustgreSQLError::Internal(
                    "Cannot rewrite set operations using materialized views".to_string()
                ))
            }
        }
    }

    /// Estimate performance gain from using a materialized view
    fn estimate_performance_gain(&self, _query_pattern: &QueryPattern, view: &ViewDef) -> f64 {
        // Simplified performance estimation
        let mut gain = 1.0;

        // Base gain from materialization
        if view.materialized {
            gain *= 5.0; // Materialized views are typically 5x faster
        }

        // Additional gain from pre-computed joins
        if view.dependencies.len() > 1 {
            gain *= 2.0;
        }

        // Additional gain from pre-computed aggregates
        if view.query.to_uppercase().contains("GROUP BY") {
            gain *= 3.0;
        }

        gain
    }

    /// Check view freshness
    fn check_view_freshness(&self, view: &ViewDef) -> ViewFreshness {
        let now = SystemTime::now();
        let last_refreshed = view.last_refreshed;

        let (is_fresh, staleness_seconds) = match last_refreshed {
            Some(refreshed) => {
                if let Ok(elapsed) = refreshed.elapsed() {
                    match view.refresh_type {
                        RefreshType::Manual => (elapsed.as_secs() < 3600, elapsed.as_secs()), // 1 hour for manual refresh
                        RefreshType::OnCommit => (elapsed.as_secs() < 300, elapsed.as_secs()), // 5 minutes
                        RefreshType::OnDemand => (elapsed.as_secs() < 1800, elapsed.as_secs()), // 30 minutes
                        RefreshType::Scheduled(interval) => (elapsed.as_secs() < interval.as_secs(), elapsed.as_secs()),
                    }
                } else {
                    (false, u64::MAX)
                }
            }
            None => (false, u64::MAX),
        };

        ViewFreshness {
            last_refreshed,
            is_fresh,
            staleness_seconds,
        }
    }

    /// Get statistics about the query rewriter
    pub fn get_stats(&self) -> RewriterStats {
        RewriterStats {
            registered_views: self.materialized_views.len(),
            cached_patterns: self.query_patterns.len(),
        }
    }
}

/// Query rewriter statistics
#[derive(Debug)]
pub struct RewriterStats {
    /// Number of registered materialized views
    pub registered_views: usize,
    /// Number of cached query patterns
    pub cached_patterns: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_rewriter_creation() {
        let rewriter = QueryRewriter::new();
        assert_eq!(rewriter.get_stats().registered_views, 0);
    }

    #[test]
    fn test_pattern_matching() {
        let rewriter = QueryRewriter::new();

        let expr1 = Expression::Column {
            name: "id".to_string(),
            table: Some("users".to_string())
        };

        let expr2 = Expression::Column {
            name: "id".to_string(),
            table: Some("users".to_string())
        };

        assert!(rewriter.expressions_equivalent(&expr1, &expr2));
    }

    #[test]
    fn test_aggregate_extraction() {
        let rewriter = QueryRewriter::new();
        let mut aggregates = Vec::new();

        let count_expr = Expression::Function {
            name: "COUNT".to_string(),
            args: vec![Expression::Star],
        };

        rewriter.extract_aggregates(&count_expr, &mut aggregates);
        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0].function_name, "COUNT");
    }

    #[test]
    fn test_view_freshness() {
        let rewriter = QueryRewriter::new();

        let view = ViewDef {
            view_id: 1,
            name: "test_view".to_string(),
            schema_id: 1,
            columns: vec![],
            query: "SELECT COUNT(*) FROM users".to_string(),
            materialized: true,
            refresh_type: RefreshType::Manual,
            last_refreshed: Some(SystemTime::now()),
            data_table_id: None,
            dependencies: vec![],
            created_at: SystemTime::now(),
            modified_at: SystemTime::now(),
        };

        let freshness = rewriter.check_view_freshness(&view);
        assert!(freshness.is_fresh);
        assert!(freshness.last_refreshed.is_some());
    }
}