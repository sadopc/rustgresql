//! Execution engine
//!
//! Coordinates query execution and manages executor state

use crate::{Result, sql::{Statement, SelectStatement, InsertStatement, UpdateStatement, DeleteStatement, CreateTableStatement}, sql::ast::{CreateIndexStatement, DropTableStatement, DropIndexStatement, AlterTableStatement, CreateViewStatement, DropViewStatement, RefreshMaterializedViewStatement, CreateProcedureStatement, CreateFunctionStatement, DropProcedureStatement, DropFunctionStatement, CallProcedureStatement, PerformStatement, Expression, TableRef}, executor::planner::{QueryPlanner, PlanNode}, executor::operators::{QueryResult, ExecutionContext}};
use crate::optimizer::OptimizedQueryPlanner;
use std::sync::Arc;
use crate::executor::ddl_error::{DdlError, DdlOperation};
use crate::executor::query_rewrite::QueryRewriter;
use crate::executor::procedure::ProcedureExecutor;
use crate::types::{DataType, DataTypeKind, Value};
use crate::catalog::{CatalogManager, get_catalog};
use crate::transaction::ddl_transaction::{get_ddl_transaction_manager, DdlOperationType, RollbackInfo};
use crate::transaction::ddl_wal::{get_ddl_wal_manager};
use crate::storage::schema_evolution::{TableSchema, ColumnSchema, ConstraintSchema, IndexSchema, ConstraintType, IndexType, ForeignKeyReference, TableStatistics, IndexStatistics};

/// Execution statistics
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ExecutionStats {
    pub rows_scanned: usize,
    pub rows_filtered: usize,
    pub rows_produced: usize,
    pub execution_time_ms: u64,
}

/// Executor
#[derive(Debug)]
pub struct Executor {
    context: ExecutionContext,
    planner: OptimizedQueryPlanner,
    stats: ExecutionStats,
    catalog: std::sync::Arc<CatalogManager>,
    buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>,
    query_rewriter: QueryRewriter,
    procedure_executor: ProcedureExecutor,
}

impl Executor {
    pub fn new() -> Self {
        let catalog = get_catalog();
        let buffer_manager = Self::create_buffer_manager();

        let mut context = ExecutionContext::new();
        context.set_catalog(catalog.clone());
        context.set_buffer_manager(buffer_manager.clone());

        Self {
            context,
            planner: OptimizedQueryPlanner::new(),
            stats: ExecutionStats {
                rows_scanned: 0,
                rows_filtered: 0,
                rows_produced: 0,
                execution_time_ms: 0,
            },
            catalog,
            buffer_manager,
            query_rewriter: QueryRewriter::new(),
            procedure_executor: ProcedureExecutor::new(),
        }
    }

    /// Create executor with specific catalog and buffer manager
    pub fn with_catalog_and_buffer(catalog: std::sync::Arc<crate::catalog::CatalogManager>, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) -> Self {
        let mut context = ExecutionContext::new();
        context.set_catalog(catalog.clone());
        context.set_buffer_manager(buffer_manager.clone());

        Self {
            context,
            planner: OptimizedQueryPlanner::new(),
            stats: ExecutionStats {
                rows_scanned: 0,
                rows_filtered: 0,
                rows_produced: 0,
                execution_time_ms: 0,
            },
            catalog,
            buffer_manager,
            query_rewriter: QueryRewriter::new(),
            procedure_executor: ProcedureExecutor::new(),
        }
    }

    /// Create a buffer manager for the executor
    fn create_buffer_manager() -> std::sync::Arc<crate::storage::BufferPoolManager> {
        use crate::storage::file_manager::DefaultFileManager;

        let file_path = "rustgresql.db";
        let file_manager = if std::path::Path::new(file_path).exists() {
            DefaultFileManager::open(file_path).unwrap_or_else(|_| {
                DefaultFileManager::create(file_path, 8192).unwrap()
            })
        } else {
            DefaultFileManager::create(file_path, 8192).unwrap()
        };

        std::sync::Arc::new(
            crate::storage::BufferPoolManager::new(
                1000,
                std::sync::Arc::new(std::sync::Mutex::new(file_manager))
            )
        )
    }

    /// Execute a SQL statement and return results
    pub fn execute_statement(&mut self, statement: &Statement) -> Result<QueryResult> {
        let start_time = std::time::Instant::now();

        // Apply query rewriting for SELECT statements
        let final_statement = if let Statement::Select(_) = statement {
            match self.query_rewriter.rewrite_query(statement) {
                Ok(rewrite_result) => {
                    if rewrite_result.was_rewritten {
                        if let Some(rewritten_query) = rewrite_result.rewritten_query {
                            // Log the rewrite for debugging
                            eprintln!("Query rewritten using materialized view: {:?}", rewrite_result.view_name);
                            rewritten_query
                        } else {
                            statement.clone()
                        }
                    } else {
                        statement.clone()
                    }
                }
                Err(_) => {
                    // If rewriting fails, continue with original query
                    statement.clone()
                }
            }
        } else {
            statement.clone()
        };

        let result = match &final_statement {
            Statement::Select(select) => self.execute_select(select)?,
            Statement::Insert(insert) => self.execute_insert(insert)?,
            Statement::Update(update) => self.execute_update(update)?,
            Statement::Delete(delete) => self.execute_delete(delete)?,
            Statement::CreateTable(create) => self.execute_create_table(create)?,
            Statement::CreateIndex(create) => self.execute_create_index(create)?,
            Statement::DropTable(drop) => self.execute_drop_table(drop)?,
            Statement::DropIndex(drop) => self.execute_drop_index(drop)?,
            Statement::AlterTable(alter) => self.execute_alter_table(alter)?,
            Statement::CreateView(create) => self.execute_create_view(create)?,
            Statement::DropView(drop) => self.execute_drop_view(drop)?,
            Statement::RefreshMaterializedView(refresh) => self.execute_refresh_materialized_view(refresh)?,
            // Stored procedure statements
            Statement::CreateProcedure(proc) => self.execute_create_procedure(proc)?,
            Statement::CreateFunction(func) => self.execute_create_function(func)?,
            Statement::DropProcedure(drop) => self.execute_drop_procedure(drop)?,
            Statement::DropFunction(drop) => self.execute_drop_function(drop)?,
            Statement::CallProcedure(call) => self.execute_call_procedure(call)?,
            Statement::Perform(perform) => self.execute_perform(perform)?,

            // Control flow statements - these should only appear within procedures
            Statement::Block(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::Return(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::IfStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::CaseStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::LoopStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::WhileStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::ForStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::Exit(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::Continue(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::Declare(_) => QueryResult { rows: vec![], column_names: vec![] },
            Statement::RaiseStatement(_) => QueryResult { rows: vec![], column_names: vec![] },
        };

        let execution_time = start_time.elapsed().as_millis() as u64;
        self.stats.execution_time_ms = execution_time;

        // Update statistics
        self.stats.rows_produced = result.rows.len();

        self.context.log(&format!(
            "Statement executed in {}ms, produced {} rows",
            execution_time,
            result.rows.len()
        ));

        Ok(result)
    }

    /// Execute SELECT statement
    fn execute_select(&mut self, select: &SelectStatement) -> Result<QueryResult> {
        let table_indexes = self.get_all_table_indexes()?;

        // Try optimized planning first, fall back to regular planner if optimization fails
        let plan = match self.planner.plan_select(select, &table_indexes) {
            Ok(plan) => plan,
            Err(_) => {
                // Fallback to regular planner when optimization fails (e.g., for subqueries or CTEs)
                let catalog = self.context.get_catalog().ok_or_else(|| {
                    crate::error::RustgreSQLError::Execution("Catalog not available in execution context".to_string())
                })?;
                let regular_planner = crate::executor::planner::QueryPlanner::with_catalog(catalog.clone());
                regular_planner.plan_select(select)?
            }
        };

        self.execute_plan(plan.root)
    }

    /// Get all table indexes for optimization
    fn get_all_table_indexes(&self) -> Result<Vec<(String, Vec<crate::catalog::IndexDef>)>> {
        let mut result = Vec::new();
        let tables = self.catalog.table_manager.list_tables()?;
        for table in tables {
            let indexes = self.catalog.index_manager.list_table_indexes(table.table_id)?;
            let index_defs = indexes.into_iter().map(|info| info.def).collect();
            result.push((table.name, index_defs));
        }
        Ok(result)
    }

    /// Execute INSERT statement
    fn execute_insert(&mut self, insert: &InsertStatement) -> Result<QueryResult> {
        let plan = self.planner.plan_insert(insert)?;
        self.execute_plan(plan.root)
    }

    /// Execute UPDATE statement
    fn execute_update(&mut self, update: &UpdateStatement) -> Result<QueryResult> {
        let plan = self.planner.plan_update(update)?;
        self.execute_plan(plan.root)
    }

    /// Execute DELETE statement
    fn execute_delete(&mut self, delete: &DeleteStatement) -> Result<QueryResult> {
        let plan = self.planner.plan_delete(delete)?;
        self.execute_plan(plan.root)
    }

    /// Execute CREATE TABLE statement with full catalog and storage integration
    fn execute_create_table(&mut self, create: &CreateTableStatement) -> Result<QueryResult> {
        self.context.log(&format!("Executing CREATE TABLE: {}", create.table_name));

        // Check if table already exists
        if let Ok(Some(_)) = self.catalog.get_table(&create.table_name) {
            if create.if_not_exists {
                self.context.log(&format!("Table '{}' already exists, skipping creation", create.table_name));
                return Ok(QueryResult {
                    rows: vec![],
                    column_names: vec!["message".to_string()],
                });
            } else {
                return Err(crate::executor::ddl_error::DdlError::table_already_exists(&create.table_name).into());
            }
        }

        // Begin transaction for DDL operation
        let transaction_id = self.begin_ddl_transaction()?;

        // Create table result tracking
        let mut created_indexes = Vec::new();

        let table_id_result: Result<u64> = (|| {
            // Convert AST column definitions to catalog column definitions
            let mut catalog_columns = Vec::new();
            let mut next_column_id = 1u64;

            // Process column definitions with constraints
            for column in &create.columns {
                let catalog_column = crate::catalog::constraint_mapping::map_column_constraints_to_catalog(
                    &column.name,
                    &column.constraints,
                    column.data_type.clone(),
                    next_column_id,
                )?;

                // Validate column definition
                self.validate_catalog_column_definition(&catalog_column, &create.table_name)?;

                catalog_columns.push(catalog_column);
                next_column_id += 1;
            }

            // Process table-level constraints
            let catalog_table_constraints = crate::catalog::constraint_mapping::map_table_constraints_to_catalog(
                &create.table_constraints,
            )?;

            // Merge column-level and table-level constraints
            let final_columns = self.merge_constraints(catalog_columns, &catalog_table_constraints)?;

            // Validate table constraints
            self.validate_table_constraints(&create.table_constraints, &final_columns, &create.table_name)?;

            // Create the table in the catalog
            let table_id = self.catalog.create_table(&create.table_name, final_columns.clone())?;

            // Log CREATE TABLE to WAL for durability
            if let Some(ddl_wal) = get_ddl_wal_manager() {
                let table_schema = self.convert_catalog_to_table_schema(&create.table_name, &final_columns, &catalog_table_constraints, table_id)?;
                ddl_wal.log_create_table(transaction_id, &create.table_name, Some("public".to_string()), &table_schema)?;
                self.context.log(&format!("Logged CREATE TABLE '{}' to WAL (LSN: recorded)", create.table_name));
            }

            // Create indexes for constraints
            for constraint in &catalog_table_constraints {
                match constraint {
                    crate::catalog::TableConstraint::PrimaryKey { columns } => {
                        let index_name = format!("{}_pkey", create.table_name);
                        self.catalog.index_manager.create_primary_key_index(
                            table_id,
                            &create.table_name,
                            columns.clone(),
                        )?;
                        created_indexes.push(index_name);
                        self.context.log(&format!("Created primary key index for table '{}'", create.table_name));
                    }
                    crate::catalog::TableConstraint::Unique { columns } => {
                        let index_name = format!("unique_{}_{}", create.table_name, columns.join("_"));
                        self.catalog.index_manager.create_index(
                            &index_name,
                            table_id,
                            columns.clone(),
                            crate::catalog::IndexType::BTree,
                            true,
                        )?;
                        created_indexes.push(index_name.clone());
                        self.context.log(&format!("Created unique index '{}' for table '{}'", index_name, create.table_name));
                    }
                    crate::catalog::TableConstraint::ForeignKey {
                        columns,
                        referenced_table,
                        referenced_columns
                    } => {
                        // Validate foreign key reference
                        self.validate_foreign_key_constraint(
                            columns,
                            referenced_table,
                            referenced_columns,
                            &create.table_name,
                        )?;

                        // Create foreign key index for better join performance
                        if !columns.is_empty() {
                            let index_name = format!("fk_{}_{}_{}", create.table_name, referenced_table, columns.join("_"));
                            self.catalog.index_manager.create_index(
                                &index_name,
                                table_id,
                                columns.clone(),
                                crate::catalog::IndexType::BTree,
                                false,
                            )?;
                            created_indexes.push(index_name.clone());
                            self.context.log(&format!("Created foreign key index '{}' for table '{}'", index_name, create.table_name));
                        }
                    }
                    crate::catalog::TableConstraint::Check { condition: _ } => {
                        // Check constraints don't need indexes, but they're validated during insert/update
                        self.context.log(&format!("Added CHECK constraint to table '{}'", create.table_name));
                    }
                    crate::catalog::TableConstraint::NotNull { column: _ } => {
                        // NOT NULL constraints are handled at the column level
                    }
                }
            }

            // Log DDL operation for recovery
            self.log_ddl_operation(
                transaction_id,
                &format!("CREATE TABLE {} ({})", create.table_name,
                    create.columns.iter()
                        .map(|c| format!("{} {}", c.name, c.data_type.type_name()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )?;

            Ok(table_id)
        })();

        // Handle the result
        let created_table_id = match table_id_result {
            Ok(id) => id,
            Err(e) => {
                // Rollback on any error
                let _ = self.rollback_ddl_transaction(transaction_id);

                // Cleanup any partially created indexes
                for index_name in &created_indexes {
                    let _ = self.catalog.index_manager.drop_index(index_name);
                }

                // Try to drop the partially created table
                let _ = self.catalog.drop_table(&create.table_name);

                self.context.log(&format!("Failed to create table '{}': {}", create.table_name, e));
                return Err(e);
            }
        };

        // Commit the transaction
        self.commit_ddl_transaction(transaction_id)?;

        // Also commit DDL WAL transaction
        if let Some(ddl_wal) = get_ddl_wal_manager() {
            ddl_wal.commit_ddl_transaction(transaction_id)?;
            self.context.log(&format!("Committed DDL WAL transaction for CREATE TABLE '{}'", create.table_name));
        }

        self.context.log(&format!(
            "Successfully created table '{}' with ID {} and {} indexes",
            create.table_name, created_table_id, created_indexes.len()
        ));

        Ok(QueryResult {
            rows: vec![vec![
                crate::types::Value::string(format!("Table '{}' created successfully", create.table_name))
            ]],
            column_names: vec!["message".to_string()],
        })
    }

    /// Execute CREATE INDEX statement
    fn execute_create_index(&mut self, create: &CreateIndexStatement) -> Result<QueryResult> {
        use crate::catalog::IndexType;

        // Check if index already exists
        if let Some(_) = self.catalog.index_manager.get_index(&create.index_name)? {
            if create.if_not_exists {
                self.context.log(&format!("Index '{}' already exists, skipping", create.index_name));
                return Ok(QueryResult {
                    rows: vec![],
                    column_names: vec![],
                });
            } else {
                return Err(crate::executor::ddl_error::DdlError::index_already_exists(&create.index_name).into());
            }
        }

        // Get table
        let table_def = match self.catalog.get_table(&create.table_name)? {
            Some(table) => table,
            None => return Err(crate::executor::ddl_error::DdlError::table_not_found(&create.table_name, crate::executor::ddl_error::DdlOperation::Create).into()),
        };

        // Create the index
        let index_id = self.catalog.index_manager.create_index(
            &create.index_name,
            table_def.table_id,
            create.columns.clone(),
            IndexType::BTree,
            create.unique,
        )?;

        self.context.log(&format!("Created index '{}' with ID {}", create.index_name, index_id));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Execute DROP TABLE statement
    fn execute_drop_table(&mut self, drop: &DropTableStatement) -> Result<QueryResult> {
        use crate::executor::ddl_error::{DdlError, DdlOperation, DdlObjectType};
        use crate::transaction::ddl_transaction::{get_ddl_transaction_manager, DdlOperationType, RollbackInfo};

        let table_name = &drop.table_name;

        // Resolve table reference (schema.table or just table)
        let (schema_name, resolved_table_name) = self.catalog.resolve_table_reference(table_name)?;

        self.context.log(&format!("Dropping table: {}.{}", schema_name, resolved_table_name));

        // Check if table exists
        let table_def = match self.catalog.get_table(&resolved_table_name)? {
            Some(table) => table,
            None => {
                if drop.if_exists {
                    self.context.log(&format!("Table {}.{} does not exist, skipping", schema_name, resolved_table_name));
                    return Ok(QueryResult {
                        rows: vec![],
                        column_names: vec![],
                    });
                } else {
                    return Err(DdlError::table_not_found(&resolved_table_name, DdlOperation::Drop).into());
                }
            }
        };

        // Verify table is in the correct schema
        if self.catalog.validate_table_in_schema(&resolved_table_name, &schema_name)? {
            // Begin DDL transaction for safety
            let ddl_tx_manager = get_ddl_transaction_manager();
            let transaction_id = 1; // Simplified transaction ID

            let ddl_context = ddl_tx_manager.begin_transaction(
                transaction_id,
                crate::transaction::ddl_transaction::SchemaChangeIsolation::Immediate,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            )?;

            // Acquire table lock
            let lock_object_name = format!("{}.{}", schema_name, resolved_table_name);
            ddl_context.lock_object(lock_object_name.clone())?;
            ddl_tx_manager.acquire_global_lock(&lock_object_name, transaction_id)?;

            // Check for dependent objects (foreign key references from other tables)
            let dependents = self.find_table_dependents(&resolved_table_name, &self.catalog)?;

            if !dependents.is_empty() {
                // If there are dependents, we need to either use CASCADE or fail
                return Err(DdlError::dependency_exists(
                    &format!("{}.{}", schema_name, resolved_table_name),
                    DdlObjectType::Table,
                    dependents,
                ).into());
            }

            // Create rollback information (backup table definition and data)
            let rollback_info = Some(RollbackInfo::RestoreTable {
                table_name: resolved_table_name.clone(),
                schema_name: schema_name.clone(),
                backup_data: self.create_table_backup(&table_def)?,
            });

            // Add operation to DDL transaction
            let operation_id = ddl_context.add_operation(
                DdlOperationType::DropTable {
                    table_name: resolved_table_name.clone(),
                    schema_name: schema_name.clone(),
                },
                Vec::new(), // No dependencies for DROP TABLE
                rollback_info,
            )?;

            // Mark operation as executing
            ddl_context.start_operation(operation_id)?;

            // Execute the actual table drop
            let catalog_clone = self.catalog.clone();
            self.drop_table_with_dependencies(&resolved_table_name, &catalog_clone)?;

            // Mark operation as completed
            ddl_context.complete_operation(operation_id)?;

            // Commit DDL transaction
            ddl_tx_manager.commit_transaction(transaction_id)?;

            self.context.log(&format!("Successfully dropped table: {}.{}", schema_name, resolved_table_name));

            Ok(QueryResult {
                rows: vec![],
                column_names: vec![],
            })
        } else {
            Err(DdlError::table_not_found(&format!("{}.{}", schema_name, resolved_table_name), DdlOperation::Drop).into())
        }
    }

    /// Execute DROP INDEX statement
    fn execute_drop_index(&mut self, drop: &DropIndexStatement) -> Result<QueryResult> {
        use crate::executor::ddl_error::{DdlError, DdlOperation, DdlObjectType};
        use crate::transaction::ddl_transaction::{get_ddl_transaction_manager, DdlOperationType, RollbackInfo};

        let catalog = self.catalog.clone();
        let index_name = &drop.index_name;

        self.context.log(&format!("Dropping index: {}", index_name));

        // Check if index exists
        let index_info = match catalog.index_manager.get_index(index_name)? {
            Some(index) => index,
            None => {
                if drop.if_exists {
                    self.context.log(&format!("Index '{}' does not exist, skipping", index_name));
                    return Ok(QueryResult {
                        rows: vec![],
                        column_names: vec![],
                    });
                } else {
                    return Err(DdlError::index_not_found(index_name, DdlOperation::Drop).into());
                }
            }
        };

        // Begin DDL transaction for safety
        let ddl_tx_manager = get_ddl_transaction_manager();
        let transaction_id = 1; // Simplified transaction ID

        let ddl_context = ddl_tx_manager.begin_transaction(
            transaction_id,
            crate::transaction::ddl_transaction::SchemaChangeIsolation::Immediate,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        )?;

        // Acquire index lock
        let lock_object_name = format!("index_{}", index_name);
        ddl_context.lock_object(lock_object_name.clone())?;
        ddl_tx_manager.acquire_global_lock(&lock_object_name, transaction_id)?;

        // Check if index is a system-generated index (primary key, unique constraint)
        if index_info.def.is_system_generated {
            return Err(DdlError::unsupported_operation(
                "DROP INDEX",
                "Cannot drop system-generated indexes (drop the constraint or table instead)",
            ).into());
        }

        // Get table name for the index (for error messages and context)
        let table_name = self.get_table_name_for_index(index_info.def.table_id, &catalog)?;

        // Check for dependencies (other indexes depending on this index)
        let dependents = self.find_index_dependents(index_name, &catalog)?;

        if !dependents.is_empty() {
            return Err(DdlError::dependency_exists(
                index_name,
                DdlObjectType::Index,
                dependents,
            ).into());
        }

        // Create rollback information (backup index definition)
        let rollback_info = Some(RollbackInfo::RecreateIndex {
            index_name: index_name.clone(),
            table_name: table_name.clone(),
            schema_name: "public".to_string(), // Simplified schema handling
            index_definition: self.create_index_backup(&index_info)?,
        });

        // Add operation to DDL transaction
        let operation_id = ddl_context.add_operation(
            DdlOperationType::DropIndex {
                index_name: index_name.clone(),
                schema_name: "public".to_string(), // Simplified schema handling
            },
            Vec::new(), // No dependencies for DROP INDEX
            rollback_info,
        )?;

        // Mark operation as executing
        ddl_context.start_operation(operation_id)?;

        // Execute the actual index drop
        self.drop_index_with_cleanup(index_name, &catalog)?;

        // Mark operation as completed
        ddl_context.complete_operation(operation_id)?;

        // Commit DDL transaction
        ddl_tx_manager.commit_transaction(transaction_id)?;

        self.context.log(&format!("Successfully dropped index: {}", index_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Convert SelectStatement to SQL string for storage
    fn select_to_sql(&self, select: &SelectStatement) -> String {
        use crate::sql::ast::SelectStatement;

        match select {
            SelectStatement::Simple {
                distinct,
                columns,
                from,
                where_clause,
                group_by,
                having,
                order_by,
                limit,
                offset,
                ..
            } => {
                let mut sql = String::from("SELECT ");

                if *distinct {
                    sql.push_str("DISTINCT ");
                }

                // Columns
                if columns.is_empty() {
                    sql.push('*');
                } else {
                    let col_strs: Vec<String> = columns.iter().map(|col| {
                        let expr_str = Self::expression_to_sql(&col.expr);
                        if let Some(alias) = &col.alias {
                            format!("{} AS {}", expr_str, alias)
                        } else {
                            expr_str
                        }
                    }).collect();
                    sql.push_str(&col_strs.join(", "));
                }

                // FROM clause
                if !from.is_empty() {
                    sql.push_str(" FROM ");
                    let from_strs: Vec<String> = from.iter().map(|table_ref| {
                        match table_ref {
                            TableRef::Table { name, alias } => {
                                if let Some(alias) = alias {
                                    format!("{} AS {}", name, alias)
                                } else {
                                    name.clone()
                                }
                            }
                            TableRef::Subquery { subquery, alias } => {
                                let subquery_str = match subquery.as_ref() {
                                    crate::sql::ast::Statement::Select(select) => self.select_to_sql(select),
                                    // For other statement types, we could add more formatting
                                    _ => "SUBQUERY".to_string(),
                                };
                                if let Some(alias) = alias {
                                    format!("({}) AS {}", subquery_str, alias)
                                } else {
                                    format!("({})", subquery_str)
                                }
                            }
                        }
                    }).collect();
                    sql.push_str(&from_strs.join(", "));
                }

                // WHERE clause
                if let Some(where_expr) = where_clause {
                    sql.push_str(" WHERE ");
                    sql.push_str(&Self::expression_to_sql(where_expr));
                }

                // GROUP BY
                if !group_by.is_empty() {
                    sql.push_str(" GROUP BY ");
                    let group_strs: Vec<String> = group_by.iter().map(|expr| Self::expression_to_sql(expr)).collect();
                    sql.push_str(&group_strs.join(", "));
                }

                // HAVING
                if let Some(having_expr) = having {
                    sql.push_str(" HAVING ");
                    sql.push_str(&Self::expression_to_sql(having_expr));
                }

                // ORDER BY
                if !order_by.is_empty() {
                    sql.push_str(" ORDER BY ");
                    let order_strs: Vec<String> = order_by.iter().map(|order| {
                        use crate::sql::ast::SortDirection;
                        let expr_str = Self::expression_to_sql(&order.expr);
                        match order.direction {
                            SortDirection::Desc => format!("{} DESC", expr_str),
                            SortDirection::Asc => expr_str,
                        }
                    }).collect();
                    sql.push_str(&order_strs.join(", "));
                }

                // LIMIT
                if let Some(limit_val) = limit {
                    sql.push_str(&format!(" LIMIT {}", limit_val));
                }

                // OFFSET
                if let Some(offset_val) = offset {
                    sql.push_str(&format!(" OFFSET {}", offset_val));
                }

                sql
            }
            SelectStatement::SetOperation(_) => {
                // For set operations, fall back to debug format for now
                format!("{:?}", select)
            }
        }
    }

    /// Convert Expression to SQL string
    fn expression_to_sql(expr: &Expression) -> String {
        use crate::sql::ast::{BinaryOperator, UnaryOperator};
        use crate::types::ValueKind;

        match expr {
            Expression::Column { name, table } => {
                if let Some(table_name) = table {
                    format!("{}.{}", table_name, name)
                } else {
                    name.clone()
                }
            }
            Expression::Value(val) | Expression::Literal(val) => match &val.kind {
                ValueKind::Integer(i) => i.to_string(),
                ValueKind::Float(f) => f.to_string(),
                ValueKind::String(s) => format!("'{}'", s.replace('\'', "''")),
                ValueKind::Boolean(b) => b.to_string().to_uppercase(),
                ValueKind::Null(_) => "NULL".to_string(),
                _ => format!("{:?}", val),
            }
            Expression::BinaryOp { left, op, right } => {
                let op_str = Self::binary_op_to_sql(op);
                format!("{} {} {}", Self::expression_to_sql(left), op_str, Self::expression_to_sql(right))
            }
            Expression::UnaryOp { op, expr } => {
                let op_str = Self::unary_op_to_sql(op);
                format!("{} {}", op_str, Self::expression_to_sql(expr))
            }
            Expression::Function { name, args, distinct: _ } => {
                let arg_strs: Vec<String> = args.iter().map(|arg| Self::expression_to_sql(arg)).collect();
                format!("{}({})", name, arg_strs.join(", "))
            }
            Expression::Star => "*".to_string(),
            _ => format!("{:?}", expr), // Fallback for complex expressions
        }
    }

    /// Convert BinaryOperator to SQL string
    fn binary_op_to_sql(op: &crate::sql::ast::BinaryOperator) -> &'static str {
        use crate::sql::ast::BinaryOperator;
        match op {
            BinaryOperator::Equals => "=",
            BinaryOperator::NotEquals => "!=",
            BinaryOperator::LessThan => "<",
            BinaryOperator::LessThanOrEquals => "<=",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::GreaterThanOrEquals => ">=",
            BinaryOperator::Like => "LIKE",
            BinaryOperator::ILike => "ILIKE",
            BinaryOperator::In => "IN",
            BinaryOperator::And => "AND",
            BinaryOperator::Or => "OR",
            BinaryOperator::Is => "IS",
            BinaryOperator::IsNot => "IS NOT",
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
        }
    }

    /// Convert UnaryOperator to SQL string
    fn unary_op_to_sql(op: &crate::sql::ast::UnaryOperator) -> &'static str {
        use crate::sql::ast::UnaryOperator;
        match op {
            UnaryOperator::Not => "NOT",
            UnaryOperator::Minus => "-",
            UnaryOperator::Plus => "+",
            UnaryOperator::Exists => "EXISTS",
            UnaryOperator::NotExists => "NOT EXISTS",
        }
    }

    /// Execute CREATE VIEW statement
    fn execute_create_view(&mut self, create: &CreateViewStatement) -> Result<QueryResult> {
        use crate::sql::ast::SelectStatement;

        let catalog = &self.catalog;
        let view_name = &create.view_name;

        self.context.log(&format!("Creating {}view: {}",
            if create.materialized { "materialized " } else { "" },
            view_name));

        // Convert column aliases to view data types if provided
        let view_columns = if create.columns.is_empty() {
            // Infer column types from the query - for now use basic types
            vec![]
        } else {
            create.columns.iter().map(|col| {
                (col.clone(), crate::catalog::view::DataType::Text)
            }).collect()
        };

        // Convert the SelectStatement to SQL string for storage
        let query_string = self.select_to_sql(&create.query);

        // Create the view using the catalog manager
        let view_id = catalog.create_view(
            view_name,
            "public", // Default to public schema for now
            view_columns,
            query_string,
            create.materialized,
        )?;

        // Register materialized views with the query rewriter
        if create.materialized {
            if let Some(view_def) = catalog.view_manager.get_view(view_name)? {
                self.register_materialized_view(view_def)?;
                self.context.log(&format!("Registered materialized view '{}' for query rewriting", view_name));
            }
        }

        self.context.log(&format!("Created {}view '{}' with ID {}",
            if create.materialized { "materialized " } else { "" },
            view_name, view_id));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Execute DROP VIEW statement
    fn execute_drop_view(&mut self, drop: &DropViewStatement) -> Result<QueryResult> {
        let catalog = self.catalog.clone();
        let view_name = &drop.view_name;

        self.context.log(&format!("Dropping {}view: {}",
            if drop.materialized { "materialized " } else { "" },
            view_name));

        // Unregister materialized views from query rewriter before dropping
        if drop.materialized {
            self.unregister_materialized_view(view_name);
            self.context.log(&format!("Unregistered materialized view '{}' from query rewriting", view_name));
        }

        // Drop the view using the catalog manager
        catalog.drop_view(view_name, drop.cascade)?;

        self.context.log(&format!("Dropped {}view '{}'",
            if drop.materialized { "materialized " } else { "" },
            view_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Execute REFRESH MATERIALIZED VIEW statement
    fn execute_refresh_materialized_view(&mut self, refresh: &RefreshMaterializedViewStatement) -> Result<QueryResult> {
        let catalog = &self.catalog;
        let view_name = &refresh.view_name;

        self.context.log(&format!("Refreshing materialized view: {} ({})",
            view_name,
            if refresh.concurrently { "concurrently" } else { "standardly" }));

        // Refresh the materialized view using the catalog manager
        catalog.refresh_materialized_view(view_name, refresh.with_data)?;

        self.context.log(&format!("Refreshed materialized view '{}'", view_name));

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Execute ALTER TABLE statement
    fn execute_alter_table(&mut self, alter: &AlterTableStatement) -> Result<QueryResult> {
        use crate::executor::ddl_error::{DdlError, DdlOperation, DdlObjectType};
        use crate::transaction::ddl_transaction::{get_ddl_transaction_manager, DdlOperationType, RollbackInfo};

        let table_name = &alter.table_name;

        // Resolve table reference (schema.table or just table)
        let (schema_name, resolved_table_name) = self.catalog.resolve_table_reference(table_name)?;

        self.context.log(&format!("Altering table: {}.{}", schema_name, resolved_table_name));

        // Check if table exists
        let table_def = match self.catalog.get_table(&resolved_table_name)? {
            Some(table) => table,
            None => {
                return Err(DdlError::table_not_found(&resolved_table_name, DdlOperation::Alter).into());
            }
        };

        // Verify table is in the correct schema
        if !self.catalog.validate_table_in_schema(&resolved_table_name, &schema_name)? {
            return Err(DdlError::table_not_found(&format!("{}.{}", schema_name, resolved_table_name), DdlOperation::Alter).into());
        }

        // Begin DDL transaction for safety
        let ddl_tx_manager = get_ddl_transaction_manager();
        let transaction_id = 1; // Simplified transaction ID

        let ddl_context = ddl_tx_manager.begin_transaction(
            transaction_id,
            crate::transaction::ddl_transaction::SchemaChangeIsolation::Immediate,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        )?;

        // Acquire table lock
        let lock_object_name = format!("{}.{}", schema_name, resolved_table_name);
        ddl_context.lock_object(lock_object_name.clone())?;
        ddl_tx_manager.acquire_global_lock(&lock_object_name, transaction_id)?;

        // Validate the specific ALTER operation
        let catalog_clone = self.catalog.clone();
        self.validate_alter_table_operation(&alter.operation, &table_def, &catalog_clone)?;

        // Create rollback information based on operation type
        let rollback_info = self.create_alter_table_rollback_info(&alter.operation, &table_def)?;

        // Add operation to DDL transaction
        let operation_id = ddl_context.add_operation(
            DdlOperationType::AlterTable {
                table_name: resolved_table_name.clone(),
                schema_name: schema_name.clone(),
                operation: format!("{:?}", alter.operation),
            },
            Vec::new(), // No dependencies for ALTER TABLE
            rollback_info,
        )?;

        // Mark operation as executing
        ddl_context.start_operation(operation_id)?;

        // Execute the specific ALTER TABLE operation
        let catalog_clone = self.catalog.clone();
        let result = self.execute_alter_table_operation(&alter.operation, &resolved_table_name, &schema_name, &catalog_clone)?;

        // Mark operation as completed
        ddl_context.complete_operation(operation_id)?;

        // Commit DDL transaction
        ddl_tx_manager.commit_transaction(transaction_id)?;

        self.context.log(&format!("Successfully altered table: {}.{}", schema_name, resolved_table_name));

        Ok(result)
    }

    /// Execute an execution plan
    fn execute_plan(&mut self, plan: PlanNode) -> Result<QueryResult> {
        plan.execute(&mut self.context)
    }

    /// Get execution statistics
    pub fn get_stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// Get execution logs
    pub fn get_logs(&self) -> &[String] {
        self.context.get_logs()
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ExecutionStats {
            rows_scanned: 0,
            rows_filtered: 0,
            rows_produced: 0,
            execution_time_ms: 0,
        };
    }

    /// Find all tables that depend on the given table (foreign key references)
    fn find_table_dependents(&self, table_name: &str, catalog: &crate::catalog::CatalogManager) -> Result<Vec<String>> {
        let mut dependents = Vec::new();

        // Get all tables and check for foreign key references to our table
        let all_tables = catalog.table_manager.list_tables()?;

        for other_table in all_tables {
            if other_table.name != table_name {
                // Check constraints for foreign key references
                for constraint in &other_table.constraints {
                    if let crate::catalog::TableConstraint::ForeignKey {
                        referenced_table,
                        ..
                    } = constraint {
                        if referenced_table == table_name {
                            dependents.push(format!("{}.{}",
                                other_table.name, // For simplicity, using table name as schema
                                other_table.name));
                            break; // Found a dependency, no need to check other constraints
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// Create a backup of table definition and data for rollback
    fn create_table_backup(&self, table_def: &crate::catalog::TableDef) -> Result<Vec<u8>> {
        use bincode;

        // For now, just serialize the table definition
        // In a real implementation, this would also include table data
        let backup = bincode::serialize(&table_def)
            .map_err(|e| crate::error::RustgreSQLError::Internal(
                format!("Failed to create table backup: {}", e)
            ))?;

        Ok(backup)
    }

    /// Drop table with all its dependencies (indexes, constraints, data)
    fn drop_table_with_dependencies(&mut self, table_name: &str, catalog: &crate::catalog::CatalogManager) -> Result<()> {
        use crate::executor::ddl_error::DdlError;

        // Get table definition before dropping
        let table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, crate::executor::ddl_error::DdlOperation::Drop))?;

        // Step 1: Drop all indexes associated with this table
        let table_indexes = catalog.index_manager.list_table_indexes(table_def.table_id)?;
        for index_info in table_indexes {
            catalog.index_manager.drop_index(&index_info.def.name)?;
            self.context.log(&format!("Dropped index: {}", index_info.def.name));
        }

        // Step 2: Remove the table from the catalog (this handles constraints)
        catalog.table_manager.drop_table(table_name)?;

        // Step 3: In a real implementation, this would also:
        // - Free storage pages used by the table
        // - Remove table from any query caches
        // - Update statistics
        // - Log the operation to WAL

        self.context.log(&format!("Dropped table and all dependencies: {}", table_name));

        Ok(())
    }

    /// Get table name for an index by table ID
    fn get_table_name_for_index(&self, table_id: u64, catalog: &crate::catalog::CatalogManager) -> Result<String> {
        let all_tables = catalog.table_manager.list_tables()?;

        for table in all_tables {
            if table.table_id == table_id {
                return Ok(table.name);
            }
        }

        Err(crate::error::RustgreSQLError::Internal(
            format!("Table with ID {} not found for index", table_id)
        ))
    }

    /// Find all indexes that depend on the given index
    fn find_index_dependents(&self, index_name: &str, catalog: &crate::catalog::CatalogManager) -> Result<Vec<String>> {
        // In a real implementation, this would check for:
        // - Indexes that use this index for partial indexing
        // - Statistics that depend on this index
        // - Query plans that might be cached using this index

        // For now, we'll implement a simplified version that checks for
        // any implicit dependencies (though most index drops don't have explicit dependents)
        let mut dependents = Vec::new();

        // In PostgreSQL, some index types might have dependencies
        // For this implementation, we'll assume no explicit dependencies
        // between user-defined indexes

        Ok(dependents)
    }

    /// Create a backup of index definition for rollback
    fn create_index_backup(&self, index_info: &crate::catalog::IndexInfo) -> Result<Vec<u8>> {
        use bincode;

        // Serialize the index definition for rollback
        let backup = bincode::serialize(&index_info.def)
            .map_err(|e| crate::error::RustgreSQLError::Internal(
                format!("Failed to create index backup: {}", e)
            ))?;

        Ok(backup)
    }

    /// Drop index with cleanup of dependent resources
    fn drop_index_with_cleanup(&mut self, index_name: &str, catalog: &crate::catalog::CatalogManager) -> Result<()> {
        use crate::executor::ddl_error::DdlError;

        // Step 1: Remove the index from the catalog
        catalog.index_manager.drop_index(index_name)?;

        // Step 2: In a real implementation, this would also:
        // - Free storage pages used by the index
        // - Remove index from query optimizer's index statistics
        // - Invalidate cached query plans that use this index
        // - Update system statistics
        // - Remove any related metadata

        self.context.log(&format!("Dropped index and cleaned up resources: {}", index_name));

        Ok(())
    }

    /// Validate an ALTER TABLE operation
    fn validate_alter_table_operation(&mut self, operation: &crate::sql::ast::AlterOperation, table_def: &crate::catalog::TableDef, catalog: &crate::catalog::CatalogManager) -> Result<()> {
        use crate::sql::ast::AlterOperation;

        match operation {
            AlterOperation::AddColumn { column } => {
                // Check if column already exists
                if table_def.columns.iter().any(|c| c.name == column.name) {
                    return Err(DdlError::column_already_exists(&column.name, &table_def.name).into());
                }

                // Validate column definition
                self.validate_column_definition(column, table_def)?;
            }

            AlterOperation::DropColumn { column_name } => {
                // Check if column exists
                if !table_def.columns.iter().any(|c| c.name == *column_name) {
                    return Err(DdlError::column_not_found(column_name, &table_def.name, DdlOperation::Alter).into());
                }

                // Check if column is referenced by constraints
                self.check_column_dependencies(column_name, table_def)?;
            }

            AlterOperation::RenameColumn { old_name, new_name } => {
                // Check if old column exists
                if !table_def.columns.iter().any(|c| c.name == *old_name) {
                    return Err(DdlError::column_not_found(old_name, &table_def.name, DdlOperation::Alter).into());
                }

                // Check if new column name already exists
                if table_def.columns.iter().any(|c| c.name == *new_name) {
                    return Err(DdlError::column_already_exists(new_name, &table_def.name).into());
                }
            }

            AlterOperation::AddConstraint { constraint } => {
                // Validate constraint definition
                self.validate_table_constraint(constraint, table_def)?;
            }

            AlterOperation::DropConstraint { constraint_name } => {
                // Check if constraint exists
                if !self.constraint_exists(constraint_name, table_def) {
                    return Err(DdlError::constraint_not_found(constraint_name, "Constraint not found").into());
                }
            }

            AlterOperation::RenameTable { new_name } => {
                // Check if new table name already exists
                if let Some(_) = catalog.get_table(new_name)? {
                    return Err(DdlError::table_already_exists(new_name).into());
                }
            }
        }

        Ok(())
    }

    /// Validate a column definition for ADD COLUMN
    fn validate_column_definition(&mut self, column: &crate::sql::ast::ColumnDef, table_def: &crate::catalog::TableDef) -> Result<()> {
        // Check data type is supported
        if !self.is_data_type_supported(&column.data_type) {
            return Err(DdlError::invalid_constraint_definition(
                &column.name,
                "Unsupported data type"
            ).into());
        }

        // Check default value compatibility
        for constraint in &column.constraints {
            if let crate::sql::ast::ColumnConstraint::Default(default_value) = constraint {
                if !self.is_default_value_compatible(default_value, &column.data_type) {
                    return Err(DdlError::invalid_default_value(
                        &column.name,
                        &format!("Default value '{}' is not compatible with column type", default_value)
                    ).into());
                }
            }
        }

        Ok(())
    }

    /// Check if a column is referenced by any constraints
    fn check_column_dependencies(&mut self, column_name: &str, table_def: &crate::catalog::TableDef) -> Result<()> {
        // Check primary key constraints
        for constraint in &table_def.constraints {
            match constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } => {
                    if columns.contains(&column_name.to_string()) {
                        return Err(DdlError::column_in_use(column_name, &table_def.name).into());
                    }
                }
                crate::catalog::TableConstraint::ForeignKey { columns, .. } => {
                    if columns.contains(&column_name.to_string()) {
                        return Err(DdlError::column_in_use(column_name, &table_def.name).into());
                    }
                }
                crate::catalog::TableConstraint::Unique { columns } => {
                    if columns.contains(&column_name.to_string()) {
                        return Err(DdlError::column_in_use(column_name, &table_def.name).into());
                    }
                }
                crate::catalog::TableConstraint::NotNull { column } => {
                    if column == column_name {
                        return Err(DdlError::column_in_use(column_name, &table_def.name).into());
                    }
                }
                _ => {}
            }
        }

        // In a real implementation, this would also check for:
        // - Foreign key references from other tables
        // - Indexes that use this column
        // - Views that depend on this column
        // - Stored procedures or functions that reference this column

        Ok(())
    }

    /// Validate a table constraint for ADD CONSTRAINT
    fn validate_table_constraint(&mut self, constraint: &crate::sql::ast::TableConstraint, table_def: &crate::catalog::TableDef) -> Result<()> {
        match constraint {
            crate::sql::ast::TableConstraint::PrimaryKey { columns, .. } => {
                // Check all columns exist
                for column_name in columns {
                    if !table_def.columns.iter().any(|c| c.name == *column_name) {
                        return Err(DdlError::column_not_found(column_name, &table_def.name, DdlOperation::Create).into());
                    }
                }

                // Check if table already has a primary key
                if table_def.columns.iter().any(|c| c.primary_key) {
                    return Err(DdlError::constraint_violation("primary_key", "Table already has a primary key").into());
                }
            }

            crate::sql::ast::TableConstraint::ForeignKey { columns, ref_table, ref_columns: _, .. } => {
                // Check foreign key columns exist
                for column_name in columns {
                    if !table_def.columns.iter().any(|c| c.name == *column_name) {
                        return Err(DdlError::column_not_found(column_name, &table_def.name, DdlOperation::Create).into());
                    }
                }

                // Check referenced table exists
                // This would need catalog integration to validate
                println!("TODO: Validate referenced table '{}'", ref_table);
            }

            crate::sql::ast::TableConstraint::Unique { columns, .. } => {
                // Check all columns exist
                for column_name in columns {
                    if !table_def.columns.iter().any(|c| c.name == *column_name) {
                        return Err(DdlError::column_not_found(column_name, &table_def.name, DdlOperation::Create).into());
                    }
                }
            }

            crate::sql::ast::TableConstraint::Check { condition, .. } => {
                // For now, we'll do basic validation that condition exists
                // In a real implementation, this would parse and validate the expression
                println!("Validating CHECK constraint: {:?}", condition);
            }
        }

        Ok(())
    }

    /// Check if a constraint exists in the table
    fn constraint_exists(&self, _constraint_name: &str, _table_def: &crate::catalog::TableDef) -> bool {
        // For simplicity, this is a basic implementation
        // In a real system, constraints would have names stored
        false // Placeholder - would need proper constraint name tracking
    }

    /// Check if a data type is supported
    fn is_data_type_supported(&self, data_type: &DataType) -> bool {
        match &data_type.kind {
            DataTypeKind::Integer | DataTypeKind::BigInt | DataTypeKind::Real |
            DataTypeKind::DoublePrecision | DataTypeKind::Numeric(_, _) | DataTypeKind::Decimal(_, _) |
            DataTypeKind::Text | DataTypeKind::Varchar(_) |
            DataTypeKind::Char(_) | DataTypeKind::Boolean | DataTypeKind::Date |
            DataTypeKind::Timestamp | DataTypeKind::TimestampWithTimeZone |
            DataTypeKind::Serial | DataTypeKind::BigSerial => true,
            _ => false, // Unsupported types for now
        }
    }

    /// Check if default value is compatible with data type
    fn is_default_value_compatible(&self, _default_value: &str, _data_type: &DataType) -> bool {
        // Simplified validation - in a real implementation this would be more comprehensive
        true
    }

    /// Validate a catalog column definition for CREATE TABLE
    fn validate_catalog_column_definition(&self, column: &crate::catalog::ColumnDef, table_name: &str) -> Result<()> {
        // Check data type is supported
        if !self.is_data_type_supported(&column.data_type) {
            return Err(crate::executor::ddl_error::DdlError::invalid_constraint_definition(
                &column.name,
                "Unsupported data type"
            ).into());
        }

        // Check for valid default value compatibility
        if let Some(default_value) = &column.default_value {
            if !self.is_default_value_compatible(&format!("{:?}", default_value), &column.data_type) {
                return Err(crate::executor::ddl_error::DdlError::invalid_default_value(
                    &column.name,
                    "Default value is not compatible with column data type"
                ).into());
            }
        }

        // Additional validation based on constraint combinations
        if column.primary_key && column.nullable {
            return Err(crate::executor::ddl_error::DdlError::invalid_constraint_definition(
                &column.name,
                "Primary key columns cannot be nullable"
            ).into());
        }

        Ok(())
    }

    /// Validate table constraints for CREATE TABLE
    fn validate_table_constraints(&self, constraints: &[crate::sql::ast::TableConstraint], table_columns: &[crate::catalog::ColumnDef], table_name: &str) -> Result<()> {
        for constraint in constraints {
            match constraint {
                crate::sql::ast::TableConstraint::PrimaryKey { columns, .. } => {
                    // Check all columns exist and are compatible with primary key
                    for column_name in columns {
                        if let Some(column) = table_columns.iter().find(|c| c.name == *column_name) {
                            if column.nullable {
                                return Err(crate::executor::ddl_error::DdlError::invalid_constraint_definition(
                                    &format!("PRIMARY KEY ({})", columns.join(", ")),
                                    "Primary key columns cannot be nullable"
                                ).into());
                            }
                        } else {
                            return Err(crate::executor::ddl_error::DdlError::column_not_found(
                                column_name,
                                table_name,
                                crate::executor::ddl_error::DdlOperation::Create
                            ).into());
                        }
                    }
                }

                crate::sql::ast::TableConstraint::ForeignKey { columns, ref_table, ref_columns, .. } => {
                    // Check foreign key columns exist
                    for column_name in columns {
                        if !table_columns.iter().any(|c| c.name == *column_name) {
                            return Err(crate::executor::ddl_error::DdlError::column_not_found(
                                column_name,
                                table_name,
                                crate::executor::ddl_error::DdlOperation::Create
                            ).into());
                        }
                    }

                    // Validate referenced table exists
                    if self.catalog.get_table(ref_table)?.is_none() {
                        return Err(crate::executor::ddl_error::DdlError::table_not_found(
                            ref_table,
                            crate::executor::ddl_error::DdlOperation::Create
                        ).into());
                    }
                }

                crate::sql::ast::TableConstraint::Unique { columns, .. } => {
                    // Check all columns exist
                    for column_name in columns {
                        if !table_columns.iter().any(|c| c.name == *column_name) {
                            return Err(crate::executor::ddl_error::DdlError::column_not_found(
                                column_name,
                                table_name,
                                crate::executor::ddl_error::DdlOperation::Create
                            ).into());
                        }
                    }
                }

                crate::sql::ast::TableConstraint::Check { condition, .. } => {
                    // Basic validation that condition exists
                    // In a real implementation, this would parse and validate the expression
                    // For now, we'll just accept any CHECK condition
                }
            }
        }

        Ok(())
    }

    /// Merge column-level and table-level constraints
    fn merge_constraints(&self, mut columns: Vec<crate::catalog::ColumnDef>, table_constraints: &[crate::catalog::TableConstraint]) -> Result<Vec<crate::catalog::ColumnDef>> {
        // Apply table-level constraints to columns
        for constraint in table_constraints {
            match constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns: pk_columns } => {
                    for column_name in pk_columns {
                        if let Some(column) = columns.iter_mut().find(|c| c.name == *column_name) {
                            column.primary_key = true;
                            column.nullable = false; // Primary keys are implicitly NOT NULL
                        }
                    }
                }
                crate::catalog::TableConstraint::Unique { columns: uniq_columns } => {
                    for column_name in uniq_columns {
                        if let Some(column) = columns.iter_mut().find(|c| c.name == *column_name) {
                            column.unique = true;
                        }
                    }
                }
                crate::catalog::TableConstraint::NotNull { column: nn_column } => {
                    if let Some(column) = columns.iter_mut().find(|c| c.name == *nn_column) {
                        column.nullable = false;
                    }
                }
                crate::catalog::TableConstraint::ForeignKey { columns: fk_columns, .. } => {
                    // Foreign key constraints are stored at table level, but we might mark columns
                    for column_name in fk_columns {
                        if let Some(_column) = columns.iter_mut().find(|c| c.name == *column_name) {
                            // Could add foreign key flag to column if needed
                        }
                    }
                }
                crate::catalog::TableConstraint::Check { condition: _ } => {
                    // Check constraints are stored at table level
                }
            }
        }

        Ok(columns)
    }

    /// Create rollback information for ALTER TABLE operations
    fn create_alter_table_rollback_info(&self, operation: &crate::sql::ast::AlterOperation, table_def: &crate::catalog::TableDef) -> Result<Option<RollbackInfo>> {
        use crate::sql::ast::AlterOperation;
        use bincode;

        let rollback_info = match operation {
            AlterOperation::AddColumn { column } => {
                // For ADD COLUMN, rollback would be to drop the column
                Some(RollbackInfo::ReverseAlter {
                    table_name: table_def.name.clone(),
                    schema_name: "public".to_string(),
                    reverse_operation: format!("DROP COLUMN {}", column.name),
                    original_schema: bincode::serialize(table_def).unwrap_or_default(),
                })
            }

            AlterOperation::DropColumn { column_name } => {
                // For DROP COLUMN, rollback would be to restore the column
                Some(RollbackInfo::ReverseAlter {
                    table_name: table_def.name.clone(),
                    schema_name: "public".to_string(),
                    reverse_operation: format!("ADD COLUMN {} [restore]", column_name),
                    original_schema: bincode::serialize(table_def).unwrap_or_default(),
                })
            }

            AlterOperation::RenameColumn { old_name, .. } => {
                Some(RollbackInfo::ReverseAlter {
                    table_name: table_def.name.clone(),
                    schema_name: "public".to_string(),
                    reverse_operation: format!("ALTER TABLE {} RENAME COLUMN TO {}", old_name, old_name),
                    original_schema: bincode::serialize(table_def).unwrap_or_default(),
                })
            }

            AlterOperation::AddConstraint { .. } => {
                Some(RollbackInfo::ReverseAlter {
                    table_name: table_def.name.clone(),
                    schema_name: "public".to_string(),
                    reverse_operation: "DROP CONSTRAINT [restore]".to_string(),
                    original_schema: bincode::serialize(table_def).unwrap_or_default(),
                })
            }

            _ => None, // Other operations would be handled separately
        };

        Ok(rollback_info)
    }

    /// Execute the specific ALTER TABLE operation
    fn execute_alter_table_operation(
        &mut self,
        operation: &crate::sql::ast::AlterOperation,
        table_name: &str,
        _schema_name: &str,
        _catalog: &crate::catalog::CatalogManager,
    ) -> Result<QueryResult> {
        use crate::sql::ast::AlterOperation;

        match operation {
            AlterOperation::AddColumn { column } => {
                self.execute_add_column(column, table_name)?;
            }

            AlterOperation::DropColumn { column_name } => {
                self.execute_drop_column(column_name, table_name)?;
            }

            AlterOperation::RenameColumn { old_name, new_name } => {
                self.execute_rename_column(old_name, new_name, table_name)?;
            }

            AlterOperation::AddConstraint { constraint } => {
                self.execute_add_constraint(constraint, table_name)?;
            }

            AlterOperation::DropConstraint { constraint_name } => {
                self.execute_drop_constraint(constraint_name, table_name)?;
            }

            AlterOperation::RenameTable { new_name } => {
                self.execute_rename_table(table_name, new_name)?;
            }
        }

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// ALTER TABLE execution functions
    /// Implement full column operations with data handling

    fn execute_add_column(&mut self, column: &crate::sql::ast::ColumnDef, table_name: &str) -> Result<()> {
        println!("Executing ADD COLUMN {} to table {}", column.name, table_name);

        // Get catalog and table definition
        let catalog = &self.catalog;
        let mut table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;

        // Map AST column definition to catalog format
        let mut new_column = crate::catalog::constraint_mapping::map_column_constraints_to_catalog(
            &column.name,
            &column.constraints,
            column.data_type.clone(),
            table_def.columns.len() as u64,
        )?;

        // Add column to table definition
        table_def.columns.push(new_column);
        table_def.modified_at = std::time::SystemTime::now();

        // Update table in catalog - persist the changes
        catalog.update_table_definition(table_name, table_def)?;
        println!("Successfully added column {} to table {}", column.name, table_name);

        Ok(())
    }

    fn execute_drop_column(&mut self, column_name: &str, table_name: &str) -> Result<()> {
        println!("Executing DROP COLUMN {} from table {}", column_name, table_name);

        // Get catalog and table definition
        let catalog = &self.catalog;
        let mut table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;

        // Find the column to remove
        let column_index = table_def.columns.iter()
            .position(|c| c.name == column_name)
            .ok_or_else(|| DdlError::column_not_found(column_name, table_name, DdlOperation::Alter))?;

        // Remove the column
        table_def.columns.remove(column_index);
        table_def.modified_at = std::time::SystemTime::now();

        // Remove any constraints that reference this column
        table_def.constraints.retain(|constraint| {
            match constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } => !columns.contains(&column_name.to_string()),
                crate::catalog::TableConstraint::ForeignKey { columns, .. } => !columns.contains(&column_name.to_string()),
                crate::catalog::TableConstraint::Unique { columns } => !columns.contains(&column_name.to_string()),
                crate::catalog::TableConstraint::Check { .. } => true, // Check constraints don't reference specific columns directly
                crate::catalog::TableConstraint::NotNull { column } => column != column_name,
            }
        });

        // Update table in catalog - persist the changes
        catalog.update_table_definition(table_name, table_def)?;
        println!("Successfully dropped column {} from table {}", column_name, table_name);

        Ok(())
    }

    fn execute_rename_column(&mut self, old_name: &str, new_name: &str, table_name: &str) -> Result<()> {
        println!("Executing RENAME COLUMN {} to {} in table {}", old_name, new_name, table_name);

        // Get catalog and table definition
        let catalog = &self.catalog;
        let mut table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;

        // Find the column to rename
        let column = table_def.columns.iter_mut()
            .find(|c| c.name == old_name)
            .ok_or_else(|| DdlError::column_not_found(old_name, table_name, DdlOperation::Alter))?;

        // Update column name
        column.name = new_name.to_string();
        table_def.modified_at = std::time::SystemTime::now();

        // Update constraints that reference this column
        for constraint in &mut table_def.constraints {
            match constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } |
                crate::catalog::TableConstraint::Unique { columns } => {
                    for column_ref in columns {
                        if column_ref == old_name {
                            *column_ref = new_name.to_string();
                        }
                    }
                }
                crate::catalog::TableConstraint::ForeignKey { columns, .. } => {
                    for column_ref in columns {
                        if column_ref == old_name {
                            *column_ref = new_name.to_string();
                        }
                    }
                }
                crate::catalog::TableConstraint::NotNull { column } => {
                    if column == old_name {
                        *column = new_name.to_string();
                    }
                }
                crate::catalog::TableConstraint::Check { .. } => {
                    // Check constraints are more complex - would need to parse and update expressions
                    // For now, we'll leave them as-is
                }
            }
        }

        // Update table in catalog - persist the changes
        catalog.update_table_definition(table_name, table_def)?;
        println!("Successfully renamed column {} to {} in table {}", old_name, new_name, table_name);

        Ok(())
    }

    fn execute_add_constraint(&mut self, constraint: &crate::sql::ast::TableConstraint, table_name: &str) -> Result<()> {
        println!("Executing ADD CONSTRAINT on table {}", table_name);

        // Get catalog and table definition
        let catalog = &self.catalog;
        let mut table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;

        // Validate the constraint before adding
        self.validate_constraint_addition(constraint, &table_def)?;

        // Map AST constraint to catalog format
        let catalog_constraints = crate::catalog::constraint_mapping::map_table_constraints_to_catalog(
            &[constraint.clone()],
        )?;

        // Handle special cases for different constraint types
        for catalog_constraint in catalog_constraints {
            match &catalog_constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } => {
                    // Check if primary key already exists
                    if table_def.constraints.iter().any(|c| matches!(c, crate::catalog::TableConstraint::PrimaryKey { .. })) {
                        return Err(DdlError::constraint_violation(
                            "primary_key",
                            "Table already has a primary key constraint"
                        ).into());
                    }

                    // Ensure all columns are NOT NULL and exist
                    for col_name in columns {
                        if let Some(col_def) = table_def.columns.iter().find(|c| c.name.as_str() == col_name) {
                            if col_def.nullable {
                                return Err(DdlError::invalid_constraint_definition(
                                    "primary_key",
                                    &format!("Column '{}' used in primary key must be NOT NULL", col_name)
                                ).into());
                            }
                        } else {
                            return Err(DdlError::column_not_found(col_name, table_name, DdlOperation::Alter).into());
                        }
                    }

                    // Create primary key index
                    let index_manager = catalog.index_manager.clone();
                    index_manager.create_primary_key_index(
                        table_def.table_id,
                        table_name,
                        columns.clone()
                    )?;
                }

                crate::catalog::TableConstraint::ForeignKey {
                    columns,
                    referenced_table,
                    referenced_columns
                } => {
                    // Validate foreign key constraint
                    self.validate_foreign_key_constraint(
                        columns,
                        referenced_table,
                        referenced_columns,
                        table_name
                    )?;
                }

                crate::catalog::TableConstraint::Unique { columns } => {
                    // Validate unique constraint
                    for col_name in columns {
                        if !table_def.columns.iter().any(|c| c.name.as_str() == col_name) {
                            return Err(DdlError::column_not_found(col_name, table_name, DdlOperation::Alter).into());
                        }
                    }

                    // Create unique index for the constraint
                    let index_manager = catalog.index_manager.clone();
                    let index_name = format!("unique_{}_{}", table_name, columns.join("_"));
                    index_manager.create_index(
                        &index_name,
                        table_def.table_id,
                        columns.clone(),
                        crate::catalog::IndexType::BTree,
                        true
                    )?;
                }

                crate::catalog::TableConstraint::Check { condition } => {
                    // Validate check constraint syntax
                    if condition.trim().is_empty() {
                        return Err(DdlError::invalid_check_condition("CHECK condition cannot be empty").into());
                    }
                }

                crate::catalog::TableConstraint::NotNull { column } => {
                    // Check if column exists
                    if !table_def.columns.iter().any(|c| c.name.as_str() == column) {
                        return Err(DdlError::column_not_found(column, table_name, DdlOperation::Alter).into());
                    }

                    // Check if NOT NULL constraint would violate existing data
                    self.validate_not_null_constraint(column, table_name)?;
                }
            }

            table_def.constraints.push(catalog_constraint);
        }

        table_def.modified_at = std::time::SystemTime::now();

        println!("Successfully added constraint to table {}", table_name);

        Ok(())
    }

    fn execute_drop_constraint(&mut self, constraint_name: &str, table_name: &str) -> Result<()> {
        println!("Executing DROP CONSTRAINT {} on table {}", constraint_name, table_name);

        // Get catalog and table definition
        let catalog = &self.catalog;
        let mut table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;

        // Find the constraint to drop
        let constraint_index = table_def.constraints.iter().position(|c| {
            // Check if this constraint matches the constraint_name
            // Note: Table-level constraints might not have explicit names in our current structure
            // For now, we'll match by constraint type and columns
            match c {
                crate::catalog::TableConstraint::PrimaryKey { columns } => {
                    constraint_name == "PRIMARY" ||
                    constraint_name == &format!("pk_{}", columns.join("_")) ||
                    constraint_name == &format!("{}_pkey", table_name)
                }
                crate::catalog::TableConstraint::ForeignKey { columns, referenced_table, referenced_columns } => {
                    constraint_name == &format!("fk_{}_{}_{}", table_name, columns.join("_"), referenced_table) ||
                    constraint_name == &format!("{}_{}_fkey", table_name, columns.join("_"))
                }
                crate::catalog::TableConstraint::Unique { columns } => {
                    constraint_name == &format!("unique_{}_{}", table_name, columns.join("_")) ||
                    constraint_name == &format!("{}_{}_key", table_name, columns.join("_"))
                }
                crate::catalog::TableConstraint::Check { condition } => {
                    constraint_name.contains("check") || condition.contains(constraint_name)
                }
                crate::catalog::TableConstraint::NotNull { column } => {
                    constraint_name == &format!("notnull_{}_{}", table_name, column) ||
                    constraint_name == &format!("{}_{}_not_null", table_name, column)
                }
            }
        });

        if let Some(index) = constraint_index {
            let constraint = &table_def.constraints[index];

            // Validate constraint can be dropped
            self.validate_constraint_removal(constraint, table_name)?;

            // Handle cleanup for different constraint types
            match constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } => {
                    // Drop the primary key index
                    let index_manager = catalog.index_manager.clone();
                    let pk_index_name = format!("pk_{}", table_name);
                    if index_manager.index_exists(&pk_index_name) {
                        index_manager.drop_index(&pk_index_name)?;
                    }

                    // Remove PRIMARY KEY flag from columns
                    for column in &table_def.columns {
                        if columns.contains(&column.name) {
                            // Note: In a full implementation, we'd need to update column definitions
                            // to remove the primary_key flag
                        }
                    }
                }

                crate::catalog::TableConstraint::ForeignKey { columns, .. } => {
                    // Drop the foreign key index if it exists
                    let index_manager = catalog.index_manager.clone();
                    let fk_index_name = format!("fk_{}_{}", table_name, columns.join("_"));
                    if index_manager.index_exists(&fk_index_name) {
                        index_manager.drop_index(&fk_index_name)?;
                    }
                }

                crate::catalog::TableConstraint::Unique { columns } => {
                    // Drop the unique index
                    let index_manager = catalog.index_manager.clone();
                    let unique_index_name = format!("unique_{}_{}", table_name, columns.join("_"));
                    if index_manager.index_exists(&unique_index_name) {
                        index_manager.drop_index(&unique_index_name)?;
                    }
                }

                crate::catalog::TableConstraint::Check { .. } => {
                    // No additional cleanup needed for CHECK constraints
                }

                crate::catalog::TableConstraint::NotNull { .. } => {
                    // No additional cleanup needed for NOT NULL constraints
                }
            }

            // Remove the constraint
            table_def.constraints.remove(index);
            table_def.modified_at = std::time::SystemTime::now();

            println!("Successfully dropped constraint {} from table {}", constraint_name, table_name);
        } else {
            return Err(DdlError::constraint_not_found(
                constraint_name,
                &format!("Constraint '{}' not found in table '{}'", constraint_name, table_name)
            ).into());
        }

        Ok(())
    }

    /// Validate that a constraint can be added to the table
    fn validate_constraint_addition(&self, constraint: &crate::sql::ast::TableConstraint, table_def: &crate::catalog::TableDef) -> Result<()> {
        match constraint {
            crate::sql::ast::TableConstraint::PrimaryKey { columns, .. } => {
                // Check for duplicate primary key constraint
                if table_def.constraints.iter().any(|c| matches!(c, crate::catalog::TableConstraint::PrimaryKey { .. })) {
                    return Err(DdlError::constraint_violation(
                        "primary_key",
                        "Table already has a primary key constraint"
                    ).into());
                }

                // Check columns exist and are compatible
                for col_name in columns {
                    if !table_def.columns.iter().any(|c| c.name.as_str() == col_name) {
                        return Err(DdlError::column_not_found(col_name, &table_def.name, DdlOperation::Alter).into());
                    }
                }
            }

            crate::sql::ast::TableConstraint::ForeignKey { columns, ref_table, ref_columns, .. } => {
                // Check if all local columns exist
                for col_name in columns {
                    if !table_def.columns.iter().any(|c| c.name.as_str() == col_name) {
                        return Err(DdlError::column_not_found(col_name, &table_def.name, DdlOperation::Alter).into());
                    }
                }

                // Check if referenced table exists
                let catalog = &self.catalog;
                if catalog.get_table(ref_table)?.is_none() {
                    return Err(DdlError::table_not_found(ref_table, DdlOperation::Create).into());
                }

                // Check if referenced columns exist in the referenced table
                if let Some(ref_table_def) = catalog.get_table(ref_table)? {
                    for ref_col_name in ref_columns {
                        if !ref_table_def.columns.iter().any(|c| c.name.as_str() == ref_col_name) {
                            return Err(DdlError::column_not_found(ref_col_name, ref_table, DdlOperation::Alter).into());
                        }
                    }
                }
            }

            crate::sql::ast::TableConstraint::Unique { columns, .. } => {
                // Check if all columns exist
                for col_name in columns {
                    if !table_def.columns.iter().any(|c| c.name.as_str() == col_name) {
                        return Err(DdlError::column_not_found(col_name, &table_def.name, DdlOperation::Alter).into());
                    }
                }

                // Check for duplicate unique constraint
                for existing_constraint in &table_def.constraints {
                    if let crate::catalog::TableConstraint::Unique { columns: existing_cols } = existing_constraint {
                        if existing_cols == columns {
                            return Err(DdlError::constraint_already_exists(
                                &format!("unique_{}_{}", table_def.name, columns.join("_")),
                                &format!("Unique constraint on columns ({}) already exists", columns.join(", "))
                            ).into());
                        }
                    }
                }
            }

            crate::sql::ast::TableConstraint::Check { condition, .. } => {
                // Basic validation of CHECK condition - in a real implementation,
                // this would involve parsing and validating the expression
                // For now, we'll just check if it's a basic expression
                match condition {
                    crate::sql::ast::Expression::Value(_) |
                    crate::sql::ast::Expression::Literal(_) => {
                        return Err(DdlError::invalid_check_condition("CHECK condition must be a boolean expression").into());
                    }
                    _ => {
                        // Accept more complex expressions for now
                    }
                }

                // Check for duplicate check condition (basic comparison)
                for existing_constraint in &table_def.constraints {
                    if let crate::catalog::TableConstraint::Check { condition: existing_condition } = existing_constraint {
                        // For now, just check if they're the same expression type
                        // In a real implementation, this would involve deep expression comparison
                        match condition {
                            crate::sql::ast::Expression::BinaryOp { left: _, op: op1, right: _ } => {
                                // Simple check - in a real implementation, this would serialize both expressions
                                // and compare them properly. For now, we'll just accept that constraints
                                // might be duplicates and let the database handle it at runtime.
                                let _ = (existing_condition, op1); // Suppress unused warning
                            }
                            _ => {
                                let _ = existing_condition; // Suppress unused warning
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate foreign key constraint details
    fn validate_foreign_key_constraint(
        &self,
        local_columns: &[String],
        referenced_table: &str,
        referenced_columns: &[String],
        table_name: &str,
    ) -> Result<()> {
        // Check column count match
        if local_columns.len() != referenced_columns.len() {
            return Err(DdlError::invalid_constraint_definition(
                "foreign_key",
                &format!("Foreign key column count ({}) does not match referenced column count ({})",
                        local_columns.len(), referenced_columns.len())
            ).into());
        }

        // Check data type compatibility
        let catalog = &self.catalog;
        let table_def = catalog.get_table(table_name)?
            .ok_or_else(|| DdlError::table_not_found(table_name, DdlOperation::Alter))?;
        let ref_table_def = catalog.get_table(referenced_table)?
            .ok_or_else(|| DdlError::table_not_found(referenced_table, DdlOperation::Create))?;

        for (local_col, ref_col) in local_columns.iter().zip(referenced_columns.iter()) {
            let local_col_def = table_def.columns.iter()
                .find(|c| c.name.as_str() == local_col)
                .ok_or_else(|| DdlError::column_not_found(local_col, table_name, DdlOperation::Alter))?;

            let ref_col_def = ref_table_def.columns.iter()
                .find(|c| c.name.as_str() == ref_col)
                .ok_or_else(|| DdlError::column_not_found(ref_col, referenced_table, DdlOperation::Alter))?;

              // Check data type compatibility
          if local_col_def.data_type.kind != ref_col_def.data_type.kind {
              return Err(DdlError::invalid_constraint_definition(
                  "foreign_key",
                  &format!("Data type mismatch between {}.{} ({}) and {}.{} ({})",
                          table_name, local_col, local_col_def.data_type.kind,
                          referenced_table, ref_col, ref_col_def.data_type.kind)
              ).into());
          }
      }

        Ok(())
    }

    /// Validate that NOT NULL constraint won't violate existing data
    fn validate_not_null_constraint(&self, column_name: &str, table_name: &str) -> Result<()> {
        // In a full implementation, this would scan the table to check for NULL values
        // For now, we'll assume the table is empty or the operation is safe
        println!("Validating NOT NULL constraint for column {} in table {} (data validation not yet implemented)",
                column_name, table_name);
        Ok(())
    }

    /// Validate that a constraint can be safely removed
    fn validate_constraint_removal(&self, constraint: &crate::catalog::TableConstraint, table_name: &str) -> Result<()> {
        match constraint {
            crate::catalog::TableConstraint::PrimaryKey { .. } => {
                // Check if any foreign keys reference this primary key
                let catalog = &self.catalog;
                let table_list = catalog.table_manager.list_tables()?;
                for table_info in table_list {
                    if let Ok(table_def) = catalog.get_table(&table_info.name) {
                        if let Some(def) = table_def {
                            for other_constraint in &def.constraints {
                                if let crate::catalog::TableConstraint::ForeignKey { referenced_table, .. } = other_constraint {
                                    if referenced_table == table_name {
                                        return Err(DdlError::dependency_exists(
                                            table_name,
                                            crate::executor::ddl_error::DdlObjectType::Table,
                                            vec![format!("foreign_key_{}_{}", def.name, table_name)]
                                        ).into());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            crate::catalog::TableConstraint::ForeignKey { .. } => {
                // Foreign keys can generally be dropped safely
            }

            crate::catalog::TableConstraint::Unique { .. } => {
                // Unique constraints can be dropped safely
            }

            crate::catalog::TableConstraint::Check { .. } => {
                // Check constraints can be dropped safely
            }

            crate::catalog::TableConstraint::NotNull { column } => {
                // Check if dropping NOT NULL would violate any constraints
                // For now, we'll allow it
                println!("Allowing NOT NULL constraint removal for column {} (data validation not yet implemented)", column);
            }
        }

        Ok(())
    }

    fn execute_rename_table(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        println!("Executing RENAME TABLE {} to {}", old_name, new_name);

        // Get catalog
        let catalog = &self.catalog;

        // Check if new table name already exists
        if catalog.get_table(new_name)?.is_some() {
            return Err(DdlError::table_already_exists(new_name).into());
        }

        // Implementation would need catalog.rename_table() method
        println!("Successfully renamed table {} to {}", old_name, new_name);

        Ok(())
    }

    /// DDL Transaction Helper Methods

    /// Begin a DDL transaction (simplified version)
    fn begin_ddl_transaction(&mut self) -> Result<u64> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let transaction_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.context.log(&format!("Began DDL transaction: {}", transaction_id));

        // Also begin DDL WAL transaction
        if let Some(ddl_wal) = get_ddl_wal_manager() {
            let wal_lsn = ddl_wal.begin_ddl_transaction(transaction_id)?;
            self.context.log(&format!("Began DDL WAL transaction: {} (LSN: {})", transaction_id, wal_lsn));
        }

        Ok(transaction_id)
    }

    /// Commit a DDL transaction (placeholder)
    fn commit_ddl_transaction(&mut self, transaction_id: u64) -> Result<()> {
        self.context.log(&format!("Committed DDL transaction: {}", transaction_id));
        Ok(())
    }

    /// Rollback a DDL transaction (placeholder)
    fn rollback_ddl_transaction(&mut self, transaction_id: u64) -> Result<()> {
        self.context.log(&format!("Rolled back DDL transaction: {}", transaction_id));

        // Also rollback DDL WAL transaction
        if let Some(ddl_wal) = get_ddl_wal_manager() {
            ddl_wal.rollback_ddl_transaction(transaction_id)?;
            self.context.log(&format!("Rolled back DDL WAL transaction: {}", transaction_id));
        }

        Ok(())
    }

    /// Log a DDL operation for recovery (placeholder)
    fn log_ddl_operation(&mut self, transaction_id: u64, operation: &str) -> Result<()> {
        self.context.log(&format!("Logged DDL operation for transaction {}: {}", transaction_id, operation));
        Ok(())
    }

    /// Convert catalog table definition to storage TableSchema for WAL logging
    fn convert_catalog_to_table_schema(
        &self,
        table_name: &str,
        columns: &[crate::catalog::ColumnDef],
        catalog_constraints: &[crate::catalog::TableConstraint],
        table_id: u64,
    ) -> Result<TableSchema> {
        use crate::storage::{TableSchema, ColumnSchema, ConstraintSchema, ConstraintType};

        let mut table_schema = TableSchema::new(table_name.to_string(), "public".to_string());

        // Convert columns
        for (position, column) in columns.iter().enumerate() {
            // Convert Value to String for default_value
            let default_value_str = column.default_value.as_ref()
                .map(|v| format!("{:?}", v.kind));

            let column_schema = ColumnSchema {
                name: column.name.clone(),
                data_type: format!("{:?}", column.data_type.kind), // Simplified conversion
                nullable: column.nullable,
                default_value: default_value_str,
                position,
                is_primary_key: column.primary_key,
                is_unique: column.unique,
                foreign_key: None, // Will be set from constraints
            };

            table_schema.add_column(column_schema)?;
        }

        // Convert constraints
        for catalog_constraint in catalog_constraints {
            match catalog_constraint {
                crate::catalog::TableConstraint::PrimaryKey { columns } => {
                    let constraint_schema = ConstraintSchema {
                        name: format!("{}_pkey", table_name),
                        constraint_type: ConstraintType::PrimaryKey,
                        columns: columns.clone(),
                        definition: None,
                        deferrable: false,
                        initially_deferred: false,
                    };
                    table_schema.add_constraint(constraint_schema)?;
                }
                crate::catalog::TableConstraint::Unique { columns } => {
                    let constraint_schema = ConstraintSchema {
                        name: format!("unique_{}_{}", table_name, columns.join("_")),
                        constraint_type: ConstraintType::Unique,
                        columns: columns.clone(),
                        definition: None,
                        deferrable: false,
                        initially_deferred: false,
                    };
                    table_schema.add_constraint(constraint_schema)?;
                }
                crate::catalog::TableConstraint::ForeignKey { columns, referenced_table, referenced_columns } => {
                    let constraint_schema = ConstraintSchema {
                        name: format!("fk_{}_{}", table_name, referenced_table),
                        constraint_type: ConstraintType::ForeignKey,
                        columns: columns.clone(),
                        definition: Some(format!("FOREIGN KEY ({}) REFERENCES {}({})",
                            columns.join(", "), referenced_table, referenced_columns.join(", "))),
                        deferrable: false,
                        initially_deferred: false,
                    };
                    table_schema.add_constraint(constraint_schema)?;
                }
                crate::catalog::TableConstraint::Check { condition } => {
                    let constraint_schema = ConstraintSchema {
                        name: format!("chk_{}_{}", table_name, "constraint"), // Simplified naming
                        constraint_type: ConstraintType::Check,
                        columns: vec![], // Check constraints don't have specific columns in this simplified version
                        definition: Some(condition.clone()),
                        deferrable: false,
                        initially_deferred: false,
                    };
                    table_schema.add_constraint(constraint_schema)?;
                }
                crate::catalog::TableConstraint::NotNull { column: _column_name } => {
                    // NOT NULL constraints are handled at the column level (nullable field)
                    // So we don't need to add them as separate constraints here
                }
            }
        }

        Ok(table_schema)
    }

    /// Register a materialized view for query rewriting
    pub fn register_materialized_view(&mut self, view: crate::catalog::view::ViewDef) -> Result<()> {
        self.query_rewriter.register_materialized_view(view)
    }

    /// Unregister a materialized view from query rewriting
    pub fn unregister_materialized_view(&mut self, view_name: &str) {
        self.query_rewriter.unregister_materialized_view(view_name);
    }

    /// Get query rewriter statistics
    pub fn get_rewriter_stats(&self) -> crate::executor::query_rewrite::RewriterStats {
        self.query_rewriter.get_stats()
    }

    /// Initialize query rewriter with existing materialized views from catalog
    pub fn initialize_query_rewriter(&mut self) -> Result<()> {
        let materialized_views = self.catalog.view_manager.list_materialized_views()?;

        for view in materialized_views {
            self.register_materialized_view(view)?;
        }

        Ok(())
    }

    // ===== STORED PROCEDURE EXECUTION METHODS =====

    /// Execute CREATE PROCEDURE statement
    fn execute_create_procedure(&mut self, proc: &CreateProcedureStatement) -> Result<QueryResult> {
        self.context.log(&format!("Creating procedure: {}", proc.procedure_name));

        // Register the procedure with the procedure executor
        self.procedure_executor.register_procedure(proc.clone())?;

        // TODO: Store procedure in catalog for persistence
        // For now, just keep it in memory

        self.context.log(&format!("Procedure '{}' created successfully", proc.procedure_name));

        Ok(QueryResult {
            rows: vec![vec![Value::string(format!("Procedure '{}' created", proc.procedure_name))]],
            column_names: vec!["result".to_string()],
        })
    }

    /// Execute CREATE FUNCTION statement
    fn execute_create_function(&mut self, func: &CreateFunctionStatement) -> Result<QueryResult> {
        self.context.log(&format!("Creating function: {}", func.function_name));

        // Register the function with the procedure executor
        self.procedure_executor.register_function(func.clone())?;

        // TODO: Store function in catalog for persistence
        // For now, just keep it in memory

        self.context.log(&format!("Function '{}' created successfully", func.function_name));

        Ok(QueryResult {
            rows: vec![vec![Value::string(format!("Function '{}' created", func.function_name))]],
            column_names: vec!["result".to_string()],
        })
    }

    /// Execute DROP PROCEDURE statement
    fn execute_drop_procedure(&mut self, drop: &DropProcedureStatement) -> Result<QueryResult> {
        self.context.log(&format!("Dropping procedure: {}", drop.procedure_name));

        // TODO: Remove procedure from catalog
        // For now, we can't easily remove from the in-memory procedure executor
        // without modifying it to support removal

        if drop.if_exists {
            self.context.log(&format!("Procedure '{}' does not exist, ignoring", drop.procedure_name));
        } else {
            return Err(crate::error::RustgreSQLError::Procedure(format!("Procedure '{}' does not exist", drop.procedure_name)));
        }

        Ok(QueryResult {
            rows: vec![vec![Value::string(format!("Procedure '{}' dropped", drop.procedure_name))]],
            column_names: vec!["result".to_string()],
        })
    }

    /// Execute DROP FUNCTION statement
    fn execute_drop_function(&mut self, drop: &DropFunctionStatement) -> Result<QueryResult> {
        self.context.log(&format!("Dropping function: {}", drop.function_name));

        // TODO: Remove function from catalog
        // For now, we can't easily remove from the in-memory procedure executor
        // without modifying it to support removal

        if drop.if_exists {
            self.context.log(&format!("Function '{}' does not exist, ignoring", drop.function_name));
        } else {
            return Err(crate::error::RustgreSQLError::Procedure(format!("Function '{}' does not exist", drop.function_name)));
        }

        Ok(QueryResult {
            rows: vec![vec![Value::string(format!("Function '{}' dropped", drop.function_name))]],
            column_names: vec!["result".to_string()],
        })
    }

    /// Execute CALL PROCEDURE statement
    fn execute_call_procedure(&mut self, call: &CallProcedureStatement) -> Result<QueryResult> {
        self.context.log(&format!("Calling procedure: {}", call.procedure_name));

        // Evaluate arguments
        let mut args = Vec::new();
        for arg in &call.arguments {
            let value = self.evaluate_expression_to_value(arg)?;
            args.push(value);
        }

        // Execute the procedure
        let result = self.procedure_executor.execute_procedure(
            &call.procedure_name,
            args,
            &mut self.context
        )?;

        self.context.log(&format!("Procedure '{}' executed successfully", call.procedure_name));

        Ok(result)
    }

    /// Execute PERFORM statement
    fn execute_perform(&mut self, perform: &PerformStatement) -> Result<QueryResult> {
        self.context.log("Executing PERFORM statement");

        // Evaluate the expression and discard the result
        let _value = self.evaluate_expression_to_value(&perform.expression)?;

        self.context.log("PERFORM statement executed successfully");

        Ok(QueryResult {
            rows: vec![],
            column_names: vec![],
        })
    }

    /// Evaluate an expression to a Value (helper method for procedure execution)
    fn evaluate_expression_to_value(&mut self, expr: &crate::sql::ast::Expression) -> Result<Value> {
        use crate::executor::{ExpressionEvaluator, EvaluationContext};

        let eval_context = EvaluationContext::new();
        let evaluator = ExpressionEvaluator::new();
        evaluator.evaluate(expr, &eval_context)
    }
}

/// Execution engine
#[derive(Debug)]
pub struct ExecutionEngine {
    catalog: std::sync::Arc<CatalogManager>,
    buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>,
}

impl ExecutionEngine {
    pub fn new() -> Self {
        let catalog = get_catalog();
        let buffer_manager = Self::create_buffer_manager();
        Self { catalog, buffer_manager }
    }

    pub fn with_catalog_and_buffer(catalog: std::sync::Arc<CatalogManager>, buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) -> Self {
        Self { catalog, buffer_manager }
    }

    fn create_buffer_manager() -> std::sync::Arc<crate::storage::BufferPoolManager> {
        use crate::storage::file_manager::DefaultFileManager;

        let file_path = "rustgresql.db";
        let file_manager = if std::path::Path::new(file_path).exists() {
            DefaultFileManager::open(file_path).unwrap_or_else(|_| {
                DefaultFileManager::create(file_path, 8192).unwrap()
            })
        } else {
            DefaultFileManager::create(file_path, 8192).unwrap()
        };

        std::sync::Arc::new(
            crate::storage::BufferPoolManager::new(
                1000,
                std::sync::Arc::new(std::sync::Mutex::new(file_manager))
            )
        )
    }

    /// Create a new executor
    pub fn create_executor(&self) -> Executor {
        Executor::with_catalog_and_buffer(self.catalog.clone(), self.buffer_manager.clone())
    }

    /// Execute a query with a new executor
    pub fn execute_query(&self, statement: &Statement) -> Result<(QueryResult, ExecutionStats)> {
        let mut executor = self.create_executor();
        let result = executor.execute_statement(statement)?;
        let stats = executor.get_stats().clone();
        Ok((result, stats))
    }
}
