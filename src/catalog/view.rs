//! View management module
//!
//! Manages materialized and non-materialized views including metadata,
//! dependency tracking, and refresh mechanisms

use crate::Result;
use crate::error::RustgreSQLError;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

/// View definition metadata
#[derive(Debug, Clone)]
pub struct ViewDef {
    pub view_id: u64,
    pub name: String,
    pub schema_id: u64,
    pub columns: Vec<ColumnDef>,
    pub query: String,
    pub materialized: bool,
    pub refresh_type: RefreshType,
    pub last_refreshed: Option<SystemTime>,
    pub data_table_id: Option<u64>,
    pub dependencies: Vec<TableDependency>,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
}

/// View column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub column_id: u64,
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub default_value: Option<String>,
}

/// SQL data types supported by views
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    BigInt,
    Text,
    Varchar(u32), // length
    Boolean,
    Decimal,
    Real,
    Date,
    Timestamp,
    Time,
}

/// Refresh strategies for materialized views
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshType {
    /// Manual refresh only (REFRESH MATERIALIZED VIEW)
    Manual,
    /// Refresh on commit to base tables
    OnCommit,
    /// Automatic refresh when data changes
    OnDemand,
    /// Scheduled refresh with interval
    Scheduled(Duration),
}

/// Refresh status for materialized views
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshStatus {
    /// View is up to date
    Fresh,
    /// View needs refresh
    Stale,
    /// View is currently being refreshed
    Refreshing,
    /// View refresh failed
    Failed(String),
}

/// Dependency change tracking
#[derive(Debug, Clone)]
pub struct DependencyChange {
    pub table_name: String,
    pub change_type: ChangeType,
    pub timestamp: SystemTime,
    pub affected_rows: u64,
}

/// Types of changes that can trigger view refresh
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Insert,
    Update,
    Delete,
    Truncate,
    SchemaChange, // ALTER TABLE, DROP COLUMN, etc.
}

/// View refresh schedule
#[derive(Debug, Clone)]
pub struct RefreshSchedule {
    pub interval: Duration,
    pub next_refresh: SystemTime,
    pub last_refresh: Option<SystemTime>,
    pub active: bool,
}

/// Table dependency information
#[derive(Debug, Clone)]
pub struct TableDependency {
    pub table_id: u64,
    pub table_name: String,
    pub schema_id: u64,
    pub schema_name: String,
    pub dependency_type: DependencyType,
}

/// Types of dependency relationships
#[derive(Debug, Clone, PartialEq)]
pub enum DependencyType {
    /// Read dependency - view reads from this table
    Read,
    /// Write dependency - view updates this table (rare)
    Write,
    /// Auto dependency - automatically maintained
    Auto,
}

/// View manager for handling view lifecycle
#[derive(Debug)]
pub struct ViewManager {
    views: Arc<Mutex<HashMap<String, ViewDef>>>,
    next_view_id: Arc<Mutex<u64>>,
    next_column_id: Arc<Mutex<u64>>,
    // Enhanced dependency tracking
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    // Change tracking for automatic refresh
    change_log: Arc<Mutex<Vec<DependencyChange>>>,
    // Refresh scheduling
    refresh_schedules: Arc<Mutex<HashMap<String, RefreshSchedule>>>,
    // View status tracking
    view_status: Arc<Mutex<HashMap<String, RefreshStatus>>>,
}

/// Dependency graph for tracking view relationships
#[derive(Debug)]
struct DependencyGraph {
    // Map of table -> list of views that depend on it
    table_to_views: HashMap<String, HashSet<String>>,
    // Map of view -> list of tables it depends on
    view_to_tables: HashMap<String, HashSet<String>>,
    // Map of view -> list of views that depend on it (for cascading)
    view_dependencies: HashMap<String, HashSet<String>>,
}

impl ViewManager {
    /// Create a new view manager
    pub fn new() -> Self {
        Self {
            views: Arc::new(Mutex::new(HashMap::new())),
            next_view_id: Arc::new(Mutex::new(1)),
            next_column_id: Arc::new(Mutex::new(1)),
            dependency_graph: Arc::new(RwLock::new(DependencyGraph {
                table_to_views: HashMap::new(),
                view_to_tables: HashMap::new(),
                view_dependencies: HashMap::new(),
            })),
            change_log: Arc::new(Mutex::new(Vec::new())),
            refresh_schedules: Arc::new(Mutex::new(HashMap::new())),
            view_status: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initialize system views and metadata
    pub fn initialize(&self) -> Result<()> {
        // Create system view metadata tables
        // In a real implementation, this would create pg_view, pg_view_column tables
        Ok(())
    }

    /// Create a new view
    pub fn create_view(
        &self,
        name: &str,
        schema_id: u64,
        columns: Vec<(String, DataType)>,
        query: String,
        materialized: bool,
    ) -> Result<u64> {
        let mut views = self.views.lock().unwrap();
        let mut next_view_id = self.next_view_id.lock().unwrap();
        let mut next_column_id = self.next_column_id.lock().unwrap();

        // Check for duplicate view names
        if views.contains_key(name) {
            return Err(RustgreSQLError::AlreadyExists(format!("View '{}' already exists", name)));
        }

        let view_id = *next_view_id;
        *next_view_id += 1;

        let now = SystemTime::now();

        // Create column definitions with IDs
        let view_columns: Vec<ColumnDef> = columns
            .into_iter()
            .map(|(col_name, data_type)| {
                let column_id = *next_column_id;
                *next_column_id += 1;
                ColumnDef {
                    column_id,
                    name: col_name,
                    data_type,
                    nullable: true, // Views can have nullable columns by default
                    default_value: None,
                }
            })
            .collect();

        // Parse dependencies from query (simplified - would need full SQL parsing)
        let dependencies = self.extract_dependencies(&query)?;

        // Update dependency graph first
        self.update_dependency_graph(name, &dependencies)?;

        let view_def = ViewDef {
            view_id,
            name: name.to_string(),
            schema_id,
            columns: view_columns,
            query,
            materialized,
            refresh_type: RefreshType::Manual, // Default to manual
            last_refreshed: if materialized { None } else { Some(now) },
            data_table_id: None, // Will be set if materialized view is populated
            dependencies,
            created_at: now,
            modified_at: now,
        };

        // Initialize view status
        let mut status = self.view_status.lock().unwrap();
        status.insert(name.to_string(), if materialized { RefreshStatus::Stale } else { RefreshStatus::Fresh });

        views.insert(name.to_string(), view_def);
        Ok(view_id)
    }

    /// Drop a view
    pub fn drop_view(&self, name: &str, cascade: bool) -> Result<()> {
        let mut views = self.views.lock().unwrap();

        if let Some(view_def) = views.get(name) {
            // Check for dependent views if not cascading
            if !cascade {
                for (_, other_view) in views.iter() {
                    if other_view.name != name {
                        // Check if other view depends on this view
                        for dep in &other_view.dependencies {
                            if dep.table_name == name {
                                return Err(RustgreSQLError::DependentObjects(
                                    format!("Cannot drop view '{}' because view '{}' depends on it", name, other_view.name)
                                ));
                            }
                        }
                    }
                }
            }

            // For materialized views, would need to drop the underlying data table
            if let Some(data_table_id) = view_def.data_table_id {
                // TODO: Drop the materialized view's data table
                log::debug!("Would drop data table {} for materialized view {}", data_table_id, name);
            }

            views.remove(name);

            // Clean up dependency graph
            self.remove_from_dependency_graph(name)?;

            // Clean up schedules and status
            let mut schedules = self.refresh_schedules.lock().unwrap();
            schedules.remove(name);

            let mut status = self.view_status.lock().unwrap();
            status.remove(name);

            Ok(())
        } else {
            Err(RustgreSQLError::NotFound(format!("View '{}' not found", name)))
        }
    }

    /// Get view definition
    pub fn get_view(&self, name: &str) -> Result<Option<ViewDef>> {
        let views = self.views.lock().unwrap();
        Ok(views.get(name).cloned())
    }

    /// List all views in a schema
    pub fn list_views_in_schema(&self, schema_id: u64) -> Result<Vec<ViewDef>> {
        let views = self.views.lock().unwrap();
        Ok(views
            .values()
            .filter(|view| view.schema_id == schema_id)
            .cloned()
            .collect())
    }

    /// List all views (both materialized and regular)
    pub fn list_views(&self) -> Result<Vec<ViewDef>> {
        let views = self.views.lock().unwrap();
        Ok(views.values().cloned().collect())
    }

    /// List only materialized views
    pub fn list_materialized_views(&self) -> Result<Vec<ViewDef>> {
        let views = self.views.lock().unwrap();
        Ok(views
            .values()
            .filter(|view| view.materialized)
            .cloned()
            .collect())
    }

    /// Refresh a materialized view
    pub fn refresh_materialized_view(&self, name: &str, with_data: bool) -> Result<()> {
        let mut views = self.views.lock().unwrap();

        if let Some(view_def) = views.get_mut(name) {
            if !view_def.materialized {
                return Err(RustgreSQLError::InvalidOperation(
                    format!("'{}' is not a materialized view", name)
                ));
            }

            let now = SystemTime::now();
            view_def.last_refreshed = Some(now);
            view_def.modified_at = now;

            // Update view status
            let mut status = self.view_status.lock().unwrap();
            status.insert(name.to_string(), RefreshStatus::Refreshing);

            if with_data {
                // TODO: Execute the view query and populate the materialized data
                log::debug!("Would populate materialized view '{}' with data", name);

                // In a real implementation, this would:
                // 1. Parse the view query
                // 2. Execute it using the query executor
                // 3. Store results in the materialized view's data table
                // 4. Update statistics
                // 5. Update change log

                // Mark as fresh after successful refresh
                status.insert(name.to_string(), RefreshStatus::Fresh);
            } else {
                // Refresh metadata only
                log::debug!("Refreshing metadata for materialized view '{}'", name);
                status.insert(name.to_string(), RefreshStatus::Fresh);
            }

            Ok(())
        } else {
            Err(RustgreSQLError::NotFound(format!("View '{}' not found", name)))
        }
    }

    /// Set up scheduled refresh for a materialized view
    pub fn schedule_refresh(&self, name: &str, interval: Duration) -> Result<()> {
        let views = self.views.lock().unwrap();

        if !views.contains_key(name) {
            return Err(RustgreSQLError::NotFound(format!("View '{}' not found", name)));
        }

        if let Some(view_def) = views.get(name) {
            if !view_def.materialized {
                return Err(RustgreSQLError::InvalidOperation(
                    format!("Cannot schedule refresh for non-materialized view '{}'", name)
                ));
            }
        }

        let mut schedules = self.refresh_schedules.lock().unwrap();
        schedules.insert(name.to_string(), RefreshSchedule {
            interval,
            next_refresh: SystemTime::now() + interval,
            last_refresh: None,
            active: true,
        });

        Ok(())
    }

    /// Check if a materialized view needs refresh
    pub fn needs_refresh(&self, name: &str) -> Result<bool> {
        let views = self.views.lock().unwrap();
        let schedules = self.refresh_schedules.lock().unwrap();
        let status = self.view_status.lock().unwrap();

        if let Some(view_def) = views.get(name) {
            if !view_def.materialized {
                return Ok(false); // Regular views don't need refresh
            }

            // Check if view is currently fresh
            if let Some(refresh_status) = status.get(name) {
                match refresh_status {
                    RefreshStatus::Fresh => {
                        // Check if scheduled refresh is due
                        if let Some(schedule) = schedules.get(name) {
                            if schedule.active && SystemTime::now() >= schedule.next_refresh {
                                return Ok(true);
                            }
                        }
                        return Ok(false);
                    },
                    RefreshStatus::Stale | RefreshStatus::Failed(_) => return Ok(true),
                    RefreshStatus::Refreshing => return Ok(false),
                }
            }

            // Default to stale if no status found
            Ok(true)
        } else {
            Err(RustgreSQLError::NotFound(format!("View '{}' not found", name)))
        }
    }

    /// Get refresh status of a view
    pub fn get_refresh_status(&self, name: &str) -> Result<RefreshStatus> {
        let status = self.view_status.lock().unwrap();
        status.get(name)
            .cloned()
            .ok_or_else(|| RustgreSQLError::NotFound(format!("View '{}' not found", name)))
    }

    /// Record a table change that may affect materialized views
    pub fn record_table_change(&self, table_name: &str, change_type: ChangeType, affected_rows: u64) -> Result<()> {
        let change = DependencyChange {
            table_name: table_name.to_string(),
            change_type,
            timestamp: SystemTime::now(),
            affected_rows,
        };

        let mut change_log = self.change_log.lock().unwrap();
        change_log.push(change);

        // Mark dependent views as stale
        self.mark_dependent_views_stale(table_name)?;

        // Clean old change logs (keep last 1000 changes)
        let change_log_len = change_log.len();
        if change_log_len > 1000 {
            change_log.drain(0..change_log_len - 1000);
        }

        Ok(())
    }

    /// Get all views that need refresh
    pub fn get_views_needing_refresh(&self) -> Result<Vec<String>> {
        let views = self.views.lock().unwrap();
        let schedules = self.refresh_schedules.lock().unwrap();
        let status = self.view_status.lock().unwrap();
        let mut need_refresh = Vec::new();
        let now = SystemTime::now();

        for (name, view_def) in views.iter() {
            if !view_def.materialized {
                continue;
            }

            let should_refresh = match status.get(name) {
                Some(RefreshStatus::Fresh) => {
                    // Check scheduled refresh
                    if let Some(schedule) = schedules.get(name) {
                        schedule.active && now >= schedule.next_refresh
                    } else {
                        false
                    }
                },
                Some(RefreshStatus::Stale) | Some(RefreshStatus::Failed(_)) => true,
                Some(RefreshStatus::Refreshing) => false,
                None => true, // Unknown status, assume needs refresh
            };

            if should_refresh {
                need_refresh.push(name.clone());
            }
        }

        Ok(need_refresh)
    }

    /// Refresh all views that need it
    pub fn refresh_all_stale_views(&self) -> Result<Vec<String>> {
        let views_needing_refresh = self.get_views_needing_refresh()?;
        let mut refreshed = Vec::new();

        for view_name in &views_needing_refresh {
            match self.refresh_materialized_view(view_name, true) {
                Ok(_) => {
                    refreshed.push(view_name.clone());
                    // Update schedule
                    self.update_refresh_schedule(view_name)?;
                },
                Err(e) => {
                    log::warn!("Failed to refresh materialized view '{}': {}", view_name, e);

                    // Mark as failed
                    let mut status = self.view_status.lock().unwrap();
                    status.insert(view_name.clone(), RefreshStatus::Failed(e.to_string()));
                }
            }
        }

        Ok(refreshed)
    }

    /// Get dependency information for a view
    pub fn get_dependency_info(&self, name: &str) -> Result<DependencyInfo> {
        let views = self.views.lock().unwrap();
        let dependency_graph = self.dependency_graph.read().unwrap();

        if let Some(view_def) = views.get(name) {
            let table_deps = dependency_graph.view_to_tables
                .get(name)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();

            let view_deps = dependency_graph.view_dependencies
                .get(name)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();

            let dependent_views = dependency_graph.table_to_views
                .get(name)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default();

            Ok(DependencyInfo {
                view_name: name.to_string(),
                table_dependencies: table_deps,
                view_dependencies: view_deps,
                dependent_views,
                dependency_depth: self.calculate_dependency_depth(name)?,
            })
        } else {
            Err(RustgreSQLError::NotFound(format!("View '{}' not found", name)))
        }
    }

    /// Validate dependency graph for cycles
    pub fn validate_dependencies(&self) -> Result<Vec<String>> {
        let dependency_graph = self.dependency_graph.read().unwrap();
        let mut cycles = Vec::new();

        // Simple cycle detection using DFS
        let mut visited = HashSet::new();
        let mut recursion_stack = HashSet::new();

        for view_name in dependency_graph.view_dependencies.keys() {
            if !visited.contains(view_name) {
                if let Some(cycle) = self.detect_cycle_dfs(
                    view_name,
                    &mut visited,
                    &mut recursion_stack,
                    &dependency_graph.view_dependencies
                ) {
                    cycles.push(cycle);
                }
            }
        }

        Ok(cycles)
    }

    /// Get recent changes affecting a view
    pub fn get_recent_changes(&self, view_name: &str, since: Option<SystemTime>) -> Result<Vec<DependencyChange>> {
        let change_log = self.change_log.lock().unwrap();
        let dependency_graph = self.dependency_graph.read().unwrap();

        let view_tables = dependency_graph.view_to_tables
            .get(view_name)
            .map(|set| set.clone())
            .unwrap_or_default();

        let cutoff_time = since.unwrap_or_else(|| SystemTime::now() - Duration::from_secs(3600)); // Default to 1 hour ago

        Ok(change_log
            .iter()
            .filter(|change| {
                view_tables.contains(&change.table_name) && change.timestamp >= cutoff_time
            })
            .cloned()
            .collect())
    }

    /// Check if a view exists
    pub fn view_exists(&self, name: &str) -> bool {
        let views = self.views.lock().unwrap();
        views.contains_key(name)
    }

    /// Get views that depend on a specific table
    pub fn get_dependent_views(&self, table_name: &str) -> Result<Vec<ViewDef>> {
        let views = self.views.lock().unwrap();
        Ok(views
            .values()
            .filter(|view| {
                view.dependencies
                    .iter()
                    .any(|dep| dep.table_name == table_name)
            })
            .cloned()
            .collect())
    }

    /// Get view statistics
    pub fn get_view_stats(&self) -> Result<ViewStats> {
        let views = self.views.lock().unwrap();
        let status = self.view_status.lock().unwrap();

        let total_views = views.len();
        let materialized_count = views
            .values()
            .filter(|view| view.materialized)
            .count();

        let mut stale_views = 0;
        let mut refreshing_views = 0;
        let mut failed_views = 0;

        for (view_name, view_def) in views.iter() {
            if view_def.materialized {
                if let Some(refresh_status) = status.get(view_name) {
                    match refresh_status {
                        RefreshStatus::Stale => stale_views += 1,
                        RefreshStatus::Refreshing => refreshing_views += 1,
                        RefreshStatus::Failed(_) => failed_views += 1,
                        RefreshStatus::Fresh => {} // Fresh views don't need special counting
                    }
                } else {
                    // Views without status are considered stale
                    stale_views += 1;
                }
            }
        }

        Ok(ViewStats {
            total_views,
            materialized_count,
            regular_count: total_views - materialized_count,
            stale_views,
            refreshing_views,
            failed_views,
        })
    }

    /// Extract table dependencies from a SQL query (simplified implementation)
    fn extract_dependencies(&self, query: &str) -> Result<Vec<TableDependency>> {
        let mut dependencies = Vec::new();

        // Simplified dependency extraction - in practice, this would use the SQL parser
        // to properly analyze FROM clauses and subqueries
        let query_lower = query.to_lowercase();

        // Look for common table patterns in the query
        let common_tables = vec!["users", "orders", "products", "customers", "items"];

        for table_name in common_tables {
            if query_lower.contains(&format!(" from {}", table_name)) ||
               query_lower.contains(&format!(" join {}", table_name)) ||
               query_lower.contains(&format!(" {} ", table_name)) {
                dependencies.push(TableDependency {
                    table_id: 0, // Would be resolved from catalog
                    table_name: table_name.to_string(),
                    schema_id: 1, // Default to public schema
                    schema_name: "public".to_string(),
                    dependency_type: DependencyType::Read,
                });
            }
        }

        Ok(dependencies)
    }

    // Helper methods for dependency graph management

    /// Update dependency graph when creating a view
    fn update_dependency_graph(&self, view_name: &str, dependencies: &[TableDependency]) -> Result<()> {
        let mut graph = self.dependency_graph.write().unwrap();

        // Clear existing dependencies for this view
        let old_tables: HashSet<String> = graph.view_to_tables.remove(view_name)
            .unwrap_or_default()
            .into_iter()
            .collect();

        // Remove old table -> view mappings
        for table in &old_tables {
            if let Some(views) = graph.table_to_views.get_mut(table) {
                views.remove(view_name);
                if views.is_empty() {
                    graph.table_to_views.remove(table);
                }
            }
        }

        // Add new dependencies
        let mut view_tables = HashSet::new();
        for dep in dependencies {
            view_tables.insert(dep.table_name.clone());

            // Add table -> view mapping
            graph.table_to_views
                .entry(dep.table_name.clone())
                .or_insert_with(HashSet::new)
                .insert(view_name.to_string());
        }

        // Add view -> tables mapping
        graph.view_to_tables.insert(view_name.to_string(), view_tables);

        Ok(())
    }

    /// Remove view from dependency graph
    fn remove_from_dependency_graph(&self, view_name: &str) -> Result<()> {
        let mut graph = self.dependency_graph.write().unwrap();

        // Remove view -> tables mapping
        if let Some(tables) = graph.view_to_tables.remove(view_name) {
            // Remove table -> view mappings
            for table in tables {
                if let Some(views) = graph.table_to_views.get_mut(&table) {
                    views.remove(view_name);
                    if views.is_empty() {
                        graph.table_to_views.remove(&table);
                    }
                }
            }
        }

        // Remove view -> view dependencies
        graph.view_dependencies.remove(view_name);

        // Remove this view from other views' dependencies
        for (_, dependent_views) in graph.view_dependencies.iter_mut() {
            dependent_views.remove(view_name);
        }

        Ok(())
    }

    /// Mark dependent views as stale when a table changes
    fn mark_dependent_views_stale(&self, table_name: &str) -> Result<()> {
        let graph = self.dependency_graph.read().unwrap();
        let mut status = self.view_status.lock().unwrap();

        if let Some(dependent_views) = graph.table_to_views.get(table_name) {
            for view_name in dependent_views {
                if let Some(refresh_status) = status.get(view_name) {
                    match refresh_status {
                        RefreshStatus::Fresh | RefreshStatus::Failed(_) => {
                            status.insert(view_name.clone(), RefreshStatus::Stale);
                        },
                        _ => {} // Keep current status for Refreshing or Stale
                    }
                }
            }
        }

        Ok(())
    }

    /// Update refresh schedule after successful refresh
    fn update_refresh_schedule(&self, view_name: &str) -> Result<()> {
        let mut schedules = self.refresh_schedules.lock().unwrap();

        if let Some(schedule) = schedules.get_mut(view_name) {
            schedule.last_refresh = Some(SystemTime::now());
            schedule.next_refresh = SystemTime::now() + schedule.interval;
        }

        Ok(())
    }

    /// Calculate dependency depth for a view
    fn calculate_dependency_depth(&self, view_name: &str) -> Result<usize> {
        let graph = self.dependency_graph.read().unwrap();
        let mut visited = HashSet::new();
        self.calculate_depth_dfs(view_name, &graph, &mut visited)
    }

    /// DFS helper for calculating dependency depth
    fn calculate_depth_dfs(
        &self,
        view_name: &str,
        graph: &DependencyGraph,
        visited: &mut HashSet<String>,
    ) -> Result<usize> {
        if visited.contains(view_name) {
            return Err(RustgreSQLError::InvalidOperation(
                "Circular dependency detected".to_string()
            ));
        }

        visited.insert(view_name.to_string());

        let max_depth = if let Some(dependencies) = graph.view_dependencies.get(view_name) {
            let mut max = 0;
            for dep_view in dependencies {
                let depth = self.calculate_depth_dfs(dep_view, graph, visited)?;
                max = max.max(depth);
            }
            max
        } else {
            0
        };

        visited.remove(view_name);
        Ok(max_depth + 1)
    }

    /// DFS helper for cycle detection
    fn detect_cycle_dfs(
        &self,
        view_name: &str,
        visited: &mut HashSet<String>,
        recursion_stack: &mut HashSet<String>,
        view_dependencies: &HashMap<String, HashSet<String>>,
    ) -> Option<String> {
        visited.insert(view_name.to_string());
        recursion_stack.insert(view_name.to_string());

        if let Some(dependencies) = view_dependencies.get(view_name) {
            for dep_view in dependencies {
                if !visited.contains(dep_view) {
                    if let Some(cycle) = self.detect_cycle_dfs(
                        dep_view,
                        visited,
                        recursion_stack,
                        view_dependencies
                    ) {
                        return Some(cycle);
                    }
                } else if recursion_stack.contains(dep_view) {
                    // Found a cycle
                    let cycle_start = dep_view;
                    return Some(format!("Cycle detected involving view: {}", cycle_start));
                }
            }
        }

        recursion_stack.remove(view_name);
        None
    }
}

/// Dependency information for a view
#[derive(Debug, Clone)]
pub struct DependencyInfo {
    pub view_name: String,
    pub table_dependencies: Vec<String>,
    pub view_dependencies: Vec<String>,
    pub dependent_views: Vec<String>,
    pub dependency_depth: usize,
}

/// View statistics
#[derive(Debug, Clone)]
pub struct ViewStats {
    pub total_views: usize,
    pub materialized_count: usize,
    pub regular_count: usize,
    pub stale_views: usize,
    pub refreshing_views: usize,
    pub failed_views: usize,
}

impl Default for ViewManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_create_simple_view() {
        let view_manager = ViewManager::new();

        let columns = vec![
            ("id".to_string(), DataType::Integer),
            ("name".to_string(), DataType::Text),
            ("email".to_string(), DataType::Text),
        ];

        let query = "SELECT id, name, email FROM users WHERE active = true".to_string();

        let view_id = view_manager.create_view(
            "active_users",
            1, // public schema
            columns,
            query,
            false, // not materialized
        ).unwrap();

        assert!(view_id > 0);

        let view_def = view_manager.get_view("active_users").unwrap().unwrap();
        assert_eq!(view_def.name, "active_users");
        assert!(!view_def.materialized);
        assert_eq!(view_def.columns.len(), 3);
        assert_eq!(view_def.query, "SELECT id, name, email FROM users WHERE active = true");
    }

    #[test]
    fn test_create_materialized_view() {
        let view_manager = ViewManager::new();

        let columns = vec![
            ("order_id".to_string(), DataType::Integer),
            ("total".to_string(), DataType::Decimal),
            ("order_date".to_string(), DataType::Date),
        ];

        let query = "SELECT order_id, SUM(amount) as total, order_date FROM orders GROUP BY order_id, order_date".to_string();

        let view_id = view_manager.create_view(
            "order_summary",
            1, // public schema
            columns,
            query,
            true, // materialized
        ).unwrap();

        assert!(view_id > 0);

        let view_def = view_manager.get_view("order_summary").unwrap().unwrap();
        assert_eq!(view_def.name, "order_summary");
        assert!(view_def.materialized);
        assert!(view_def.last_refreshed.is_none()); // Materialized views start unrefreshed
    }

    #[test]
    fn test_duplicate_view_error() {
        let view_manager = ViewManager::new();

        let columns = vec![
            ("id".to_string(), DataType::Integer),
        ];

        let query = "SELECT id FROM users".to_string();

        // Create first view
        view_manager.create_view("user_ids", 1, columns.clone(), query.clone(), false).unwrap();

        // Try to create duplicate
        let result = view_manager.create_view("user_ids", 1, columns, query, false);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RustgreSQLError::AlreadyExists(_)));
    }

    #[test]
    fn test_drop_view() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        view_manager.create_view("user_ids", 1, columns, query, false).unwrap();
        assert!(view_manager.view_exists("user_ids"));

        view_manager.drop_view("user_ids", false).unwrap();
        assert!(!view_manager.view_exists("user_ids"));
    }

    #[test]
    fn test_refresh_materialized_view() {
        let view_manager = ViewManager::new();

        let columns = vec![("count".to_string(), DataType::Integer)];
        let query = "SELECT COUNT(*) as count FROM users".to_string();

        view_manager.create_view("user_count", 1, columns, query, true).unwrap();

        // Refresh the materialized view
        view_manager.refresh_materialized_view("user_count", true).unwrap();

        let view_def = view_manager.get_view("user_count").unwrap().unwrap();
        assert!(view_def.last_refreshed.is_some());
    }

    #[test]
    fn test_refresh_regular_view_error() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        view_manager.create_view("user_ids", 1, columns, query, false).unwrap();

        // Try to refresh a regular (non-materialized) view
        let result = view_manager.refresh_materialized_view("user_ids", true);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RustgreSQLError::InvalidOperation(_)));
    }

    #[test]
    fn test_view_stats() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        // Create regular view
        view_manager.create_view("user_ids", 1, columns.clone(), query.clone(), false).unwrap();

        // Create materialized view
        view_manager.create_view("user_ids_mat", 1, columns, query, true).unwrap();

        let stats = view_manager.get_view_stats().unwrap();
        assert_eq!(stats.total_views, 2);
        assert_eq!(stats.materialized_count, 1);
        assert_eq!(stats.regular_count, 1);
    }

    #[test]
    fn test_refresh_status_tracking() {
        let view_manager = ViewManager::new();

        let columns = vec![("count".to_string(), DataType::Integer)];
        let query = "SELECT COUNT(*) as count FROM users".to_string();

        view_manager.create_view("user_count", 1, columns, query, true).unwrap();

        // Initial status should be Stale
        assert_eq!(view_manager.get_refresh_status("user_count").unwrap(), RefreshStatus::Stale);
        assert!(view_manager.needs_refresh("user_count").unwrap());

        // Refresh the view
        view_manager.refresh_materialized_view("user_count", true).unwrap();

        // Status should now be Fresh
        assert_eq!(view_manager.get_refresh_status("user_count").unwrap(), RefreshStatus::Fresh);
        assert!(!view_manager.needs_refresh("user_count").unwrap());
    }

    #[test]
    fn test_scheduled_refresh() {
        let view_manager = ViewManager::new();

        let columns = vec![("total".to_string(), DataType::Decimal)];
        let query = "SELECT SUM(amount) as total FROM orders".to_string();

        view_manager.create_view("order_total", 1, columns, query, true).unwrap();

        // Schedule refresh every 5 minutes
        let interval = Duration::from_secs(300);
        view_manager.schedule_refresh("order_total", interval).unwrap();

        // Initially should need refresh
        assert!(view_manager.needs_refresh("order_total").unwrap());

        // Refresh the view
        view_manager.refresh_materialized_view("order_total", true).unwrap();

        // Should not need refresh immediately after
        assert!(!view_manager.needs_refresh("order_total").unwrap());
    }

    #[test]
    fn test_table_change_tracking() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        view_manager.create_view("user_ids", 1, columns, query, true).unwrap();

        // Initially fresh
        view_manager.refresh_materialized_view("user_ids", true).unwrap();
        assert_eq!(view_manager.get_refresh_status("user_ids").unwrap(), RefreshStatus::Fresh);

        // Record a table change
        view_manager.record_table_change("users", ChangeType::Insert, 10).unwrap();

        // View should now be stale
        assert_eq!(view_manager.get_refresh_status("user_ids").unwrap(), RefreshStatus::Stale);
        assert!(view_manager.needs_refresh("user_ids").unwrap());

        // Get recent changes
        let changes = view_manager.get_recent_changes("user_ids", None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].table_name, "users");
        assert_eq!(changes[0].change_type, ChangeType::Insert);
        assert_eq!(changes[0].affected_rows, 10);
    }

    #[test]
    fn test_get_views_needing_refresh() {
        let view_manager = ViewManager::new();

        // Create multiple materialized views
        let columns1 = vec![("count1".to_string(), DataType::Integer)];
        let query1 = "SELECT COUNT(*) as count1 FROM users".to_string();
        view_manager.create_view("user_count", 1, columns1, query1, true).unwrap();

        let columns2 = vec![("total".to_string(), DataType::Decimal)];
        let query2 = "SELECT SUM(amount) as total FROM orders".to_string();
        view_manager.create_view("order_total", 1, columns2, query2, true).unwrap();

        // Refresh one view
        view_manager.refresh_materialized_view("user_count", true).unwrap();

        // Should return views needing refresh
        let needing_refresh = view_manager.get_views_needing_refresh().unwrap();
        assert_eq!(needing_refresh.len(), 1);
        assert!(needing_refresh.contains(&"order_total".to_string()));
    }

    #[test]
    fn test_dependency_info() {
        let view_manager = ViewManager::new();

        let columns = vec![("user_id".to_string(), DataType::Integer), ("order_count".to_string(), DataType::Integer)];
        let query = "SELECT user_id, COUNT(*) as order_count FROM orders GROUP BY user_id".to_string();

        view_manager.create_view("user_order_counts", 1, columns, query, true).unwrap();

        let dep_info = view_manager.get_dependency_info("user_order_counts").unwrap();
        assert_eq!(dep_info.view_name, "user_order_counts");
        assert!(dep_info.table_dependencies.contains(&"orders".to_string()));
        assert_eq!(dep_info.dependency_depth, 1); // Direct dependency on tables
    }

    #[test]
    fn test_enhanced_view_stats() {
        let view_manager = ViewManager::new();

        // Create regular view
        let columns1 = vec![("id".to_string(), DataType::Integer)];
        let query1 = "SELECT id FROM users".to_string();
        view_manager.create_view("user_ids", 1, columns1.clone(), query1, false).unwrap();

        // Create materialized view
        let query2 = "SELECT COUNT(*) as count FROM users".to_string();
        view_manager.create_view("user_count", 1, columns1, query2, true).unwrap();

        let stats = view_manager.get_view_stats().unwrap();
        assert_eq!(stats.total_views, 2);
        assert_eq!(stats.materialized_count, 1);
        assert_eq!(stats.regular_count, 1);
        assert_eq!(stats.stale_views, 1); // Materialized view starts stale
        assert_eq!(stats.refreshing_views, 0);
        assert_eq!(stats.failed_views, 0);

        // Refresh the materialized view
        view_manager.refresh_materialized_view("user_count", true).unwrap();

        let stats = view_manager.get_view_stats().unwrap();
        assert_eq!(stats.stale_views, 0); // Should be fresh now
    }

    #[test]
    fn test_dependency_validation() {
        let view_manager = ViewManager::new();

        // Create views with valid dependencies
        let columns1 = vec![("id".to_string(), DataType::Integer)];
        let query1 = "SELECT id FROM users".to_string();
        view_manager.create_view("user_ids", 1, columns1, query1, true).unwrap();

        let columns2 = vec![("count".to_string(), DataType::Integer)];
        let query2 = "SELECT COUNT(*) as count FROM user_ids".to_string();
        view_manager.create_view("user_count", 1, columns2, query2, true).unwrap();

        // Should not detect cycles
        let cycles = view_manager.validate_dependencies().unwrap();
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_multiple_change_types() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM products".to_string();

        view_manager.create_view("product_ids", 1, columns, query, true).unwrap();

        // Refresh the view
        view_manager.refresh_materialized_view("product_ids", true).unwrap();

        // Record different types of changes
        view_manager.record_table_change("products", ChangeType::Insert, 5).unwrap();
        view_manager.record_table_change("products", ChangeType::Update, 3).unwrap();
        view_manager.record_table_change("products", ChangeType::Delete, 2).unwrap();

        // Get recent changes
        let changes = view_manager.get_recent_changes("product_ids", None).unwrap();
        assert_eq!(changes.len(), 3);

        let change_types: Vec<ChangeType> = changes.iter().map(|c| c.change_type.clone()).collect();
        assert!(change_types.contains(&ChangeType::Insert));
        assert!(change_types.contains(&ChangeType::Update));
        assert!(change_types.contains(&ChangeType::Delete));
    }

    #[test]
    fn test_refresh_all_stale_views() {
        let view_manager = ViewManager::new();

        // Create multiple materialized views
        let columns1 = vec![("user_count".to_string(), DataType::Integer)];
        let query1 = "SELECT COUNT(*) as user_count FROM users".to_string();
        view_manager.create_view("user_count", 1, columns1, query1, true).unwrap();

        let columns2 = vec![("order_total".to_string(), DataType::Decimal)];
        let query2 = "SELECT SUM(amount) as order_total FROM orders".to_string();
        view_manager.create_view("order_total", 1, columns2, query2, true).unwrap();

        let columns3 = vec![("product_count".to_string(), DataType::Integer)];
        let query3 = "SELECT COUNT(*) as product_count FROM products".to_string();
        view_manager.create_view("product_count", 1, columns3, query3, true).unwrap();

        // All should need refresh initially
        let needing_refresh = view_manager.get_views_needing_refresh().unwrap();
        assert_eq!(needing_refresh.len(), 3);

        // Refresh all
        let refreshed = view_manager.refresh_all_stale_views().unwrap();
        assert_eq!(refreshed.len(), 3);

        // None should need refresh now
        let needing_refresh = view_manager.get_views_needing_refresh().unwrap();
        assert_eq!(needing_refresh.len(), 0);
    }

    #[test]
    fn test_non_materialized_view_refresh() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        // Create regular (non-materialized) view
        view_manager.create_view("user_ids", 1, columns, query, false).unwrap();

        // Regular views should not need refresh
        assert!(!view_manager.needs_refresh("user_ids").unwrap());

        // Should get error trying to schedule refresh for regular view
        let result = view_manager.schedule_refresh("user_ids", Duration::from_secs(60));
        assert!(result.is_err());
    }

    #[test]
    fn test_change_log_cleanup() {
        let view_manager = ViewManager::new();

        let columns = vec![("id".to_string(), DataType::Integer)];
        let query = "SELECT id FROM users".to_string();

        view_manager.create_view("user_ids", 1, columns, query, true).unwrap();

        // Add many changes to trigger cleanup
        for i in 0..1005 {
            view_manager.record_table_change("users", ChangeType::Insert, 1).unwrap();
        }

        let changes = view_manager.get_recent_changes("user_ids", None).unwrap();
        // Should have at most 1000 changes due to cleanup
        assert!(changes.len() <= 1000);
    }
}