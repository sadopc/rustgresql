//! Comprehensive DDL error handling framework
//!
//! This module provides detailed error types and handling utilities for DDL operations,
//! including specific error codes, context information, and recovery suggestions.

use crate::error::{RustgreSQLError, Result};
use std::collections::HashMap;
use std::fmt;

/// DDL operation types for error context
#[derive(Debug, Clone, PartialEq)]
pub enum DdlOperation {
    Create,
    Drop,
    Alter,
    Truncate,
    Rename,
}

impl fmt::Display for DdlOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdlOperation::Create => write!(f, "CREATE"),
            DdlOperation::Drop => write!(f, "DROP"),
            DdlOperation::Alter => write!(f, "ALTER"),
            DdlOperation::Truncate => write!(f, "TRUNCATE"),
            DdlOperation::Rename => write!(f, "RENAME"),
        }
    }
}

/// DDL object types for error context
#[derive(Debug, Clone, PartialEq)]
pub enum DdlObjectType {
    Table,
    Index,
    Schema,
    Column,
    Constraint,
    View,
    Sequence,
}

impl fmt::Display for DdlObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdlObjectType::Table => write!(f, "table"),
            DdlObjectType::Index => write!(f, "index"),
            DdlObjectType::Schema => write!(f, "schema"),
            DdlObjectType::Column => write!(f, "column"),
            DdlObjectType::Constraint => write!(f, "constraint"),
            DdlObjectType::View => write!(f, "view"),
            DdlObjectType::Sequence => write!(f, "sequence"),
        }
    }
}

/// Detailed DDL error codes for precise error handling
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DdlErrorCode {
    // Table errors (4000-4099)
    TableNotFound = 4000,
    TableAlreadyExists = 4001,
    TableNotEmpty = 4002,
    TableInUse = 4003,
    TableDependencyExists = 4004,
    TableCorrupted = 4005,

    // Column errors (4100-4199)
    ColumnNotFound = 4100,
    ColumnAlreadyExists = 4101,
    ColumnInUse = 4102,
    ColumnDependencyExists = 4103,
    InvalidColumnDefinition = 4104,
    ColumnTypeMismatch = 4105,

    // Constraint errors (4200-4299)
    ConstraintNotFound = 4200,
    ConstraintAlreadyExists = 4201,
    ConstraintViolation = 4202,
    InvalidConstraintDefinition = 4203,
    ConstraintDependencyExists = 4204,
    ConstraintInUse = 4205,

    // Index errors (4300-4399)
    IndexNotFound = 4300,
    IndexAlreadyExists = 4301,
    IndexInUse = 4302,
    InvalidIndexDefinition = 4303,
    IndexDependencyExists = 4304,

    // Schema errors (4400-4499)
    SchemaNotFound = 4400,
    SchemaAlreadyExists = 4401,
    SchemaNotEmpty = 4402,
    SchemaInUse = 4403,

    // Dependency errors (4500-4599)
    DependencyNotFound = 4500,
    CircularDependency = 4501,
    DependencyDepthExceeded = 4502,
    DependencyBroken = 4503,

    // Transaction errors (4600-4699)
    DdlTransactionFailed = 4600,
    DdlRollbackFailed = 4601,
    ConcurrentDdlConflict = 4602,
    DdlTimeout = 4603,

    // General DDL errors (4700-4799)
    InvalidDdlSyntax = 4700,
    UnsupportedDdlOperation = 4701,
    InsufficientPrivileges = 4702,
    ResourceLimitExceeded = 4703,
    DdlFeatureNotImplemented = 4704,
}

/// Recovery suggestions for DDL errors
#[derive(Debug, Clone)]
pub enum RecoverySuggestion {
    /// No specific suggestion available
    None,

    /// Use DROP IF EXISTS
    UseIfExists,

    /// Use CASCADE to drop dependent objects
    UseCascade,

    /// Wait and retry the operation
    RetryAfterDelay(u64), // milliseconds

    /// Drop dependent objects first
    DropDependents(Vec<String>),

    /// Check object existence first
    CheckExistence,

    /// Use different name
    UseDifferentName,

    /// Contact administrator
    ContactAdministrator,

    /// Custom suggestion
    Custom(String),
}

/// Comprehensive DDL error with context and recovery suggestions
#[derive(Debug, Clone)]
pub struct DdlError {
    /// Error code for programmatic handling
    pub code: DdlErrorCode,

    /// Human-readable error message
    pub message: String,

    /// DDL operation that failed
    pub operation: DdlOperation,

    /// Type of object being operated on
    pub object_type: DdlObjectType,

    /// Name of the object (if applicable)
    pub object_name: Option<String>,

    /// Additional context information
    pub context: HashMap<String, String>,

    /// Recovery suggestions
    pub recovery_suggestion: RecoverySuggestion,

    /// SQL state code (PostgreSQL-compatible)
    pub sql_state: String,
}

impl DdlError {
    /// Create a new DDL error
    pub fn new(
        code: DdlErrorCode,
        message: String,
        operation: DdlOperation,
        object_type: DdlObjectType,
        object_name: Option<String>,
    ) -> Self {
        let (recovery_suggestion, sql_state) = Self::determine_recovery_suggestion(&code);

        Self {
            code,
            message,
            operation,
            object_type,
            object_name,
            context: HashMap::new(),
            recovery_suggestion,
            sql_state,
        }
    }

    /// Add context information to the error
    pub fn with_context(mut self, key: String, value: String) -> Self {
        self.context.insert(key, value);
        self
    }

    /// Set a custom recovery suggestion
    pub fn with_recovery(mut self, suggestion: RecoverySuggestion) -> Self {
        self.recovery_suggestion = suggestion;
        self
    }

    /// Determine appropriate recovery suggestion based on error code
    fn determine_recovery_suggestion(code: &DdlErrorCode) -> (RecoverySuggestion, String) {
        match code {
            DdlErrorCode::TableNotFound | DdlErrorCode::ColumnNotFound |
            DdlErrorCode::ConstraintNotFound | DdlErrorCode::IndexNotFound => {
                (RecoverySuggestion::UseIfExists, "42P01".to_string())
            }

            DdlErrorCode::TableAlreadyExists | DdlErrorCode::ColumnAlreadyExists |
            DdlErrorCode::ConstraintAlreadyExists | DdlErrorCode::IndexAlreadyExists => {
                (RecoverySuggestion::UseDifferentName, "42P07".to_string())
            }

            DdlErrorCode::TableDependencyExists | DdlErrorCode::ColumnDependencyExists |
            DdlErrorCode::ConstraintDependencyExists | DdlErrorCode::IndexDependencyExists => {
                (RecoverySuggestion::UseCascade, "2BP01".to_string())
            }

            DdlErrorCode::TableInUse | DdlErrorCode::ColumnInUse |
            DdlErrorCode::ConstraintInUse | DdlErrorCode::IndexInUse => {
                (RecoverySuggestion::RetryAfterDelay(1000), "55006".to_string())
            }

            DdlErrorCode::InvalidDdlSyntax => {
                (RecoverySuggestion::None, "42601".to_string())
            }

            DdlErrorCode::CircularDependency => {
                (RecoverySuggestion::Custom("Break the circular dependency by removing one of the relationships".to_string()), "0A000".to_string())
            }

            _ => (RecoverySuggestion::None, "XX000".to_string()),
        }
    }
}

impl fmt::Display for DdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let obj_desc = match (&self.object_type, &self.object_name) {
            (obj_type, Some(name)) => format!("{} '{}'", obj_type, name),
            (obj_type, None) => format!("{}", obj_type),
        };

        write!(
            f,
            "DDL Error {}: {} during {} operation on {} (SQLSTATE: {})",
            self.code as i32, self.message, self.operation, obj_desc, self.sql_state
        )
    }
}

impl std::error::Error for DdlError {}

impl From<DdlError> for RustgreSQLError {
    fn from(err: DdlError) -> Self {
        RustgreSQLError::Ddl(err.to_string())
    }
}

/// Convenience constructors for common DDL errors
impl DdlError {
    /// Table not found error
    pub fn table_not_found(table_name: &str, operation: DdlOperation) -> Self {
        Self::new(
            DdlErrorCode::TableNotFound,
            format!("Table '{}' does not exist", table_name),
            operation,
            DdlObjectType::Table,
            Some(table_name.to_string()),
        )
    }

    /// Table already exists error
    pub fn table_already_exists(table_name: &str) -> Self {
        Self::new(
            DdlErrorCode::TableAlreadyExists,
            format!("Table '{}' already exists", table_name),
            DdlOperation::Create,
            DdlObjectType::Table,
            Some(table_name.to_string()),
        )
    }

    /// Index already exists error
    pub fn index_already_exists(index_name: &str) -> Self {
        Self::new(
            DdlErrorCode::IndexAlreadyExists,
            format!("Index '{}' already exists", index_name),
            DdlOperation::Create,
            DdlObjectType::Index,
            Some(index_name.to_string()),
        )
    }

    /// Column not found error
    pub fn column_not_found(column_name: &str, table_name: &str, operation: DdlOperation) -> Self {
        Self::new(
            DdlErrorCode::ColumnNotFound,
            format!("Column '{}' does not exist in table '{}'", column_name, table_name),
            operation,
            DdlObjectType::Column,
            Some(column_name.to_string()),
        ).with_context("table_name".to_string(), table_name.to_string())
    }

    /// Column already exists error
    pub fn column_already_exists(column_name: &str, table_name: &str) -> Self {
        Self::new(
            DdlErrorCode::ColumnAlreadyExists,
            format!("Column '{}' already exists in table '{}'", column_name, table_name),
            DdlOperation::Alter,
            DdlObjectType::Column,
            Some(column_name.to_string()),
        ).with_context("table_name".to_string(), table_name.to_string())
    }

    /// Constraint violation error
    pub fn constraint_violation(constraint_name: &str, details: &str) -> Self {
        Self::new(
            DdlErrorCode::ConstraintViolation,
            format!("Constraint '{}' violated: {}", constraint_name, details),
            DdlOperation::Create, // Can occur during various operations
            DdlObjectType::Constraint,
            Some(constraint_name.to_string()),
        )
    }

    /// Index not found error
    pub fn index_not_found(index_name: &str, operation: DdlOperation) -> Self {
        Self::new(
            DdlErrorCode::IndexNotFound,
            format!("Index '{}' does not exist", index_name),
            operation,
            DdlObjectType::Index,
            Some(index_name.to_string()),
        )
    }

    /// Dependency exists error
    pub fn dependency_exists(object_name: &str, object_type: DdlObjectType, dependents: Vec<String>) -> Self {
        let message = if dependents.len() == 1 {
            format!("{} '{}' cannot be dropped because it depends on '{}'",
                    object_type, object_name, dependents[0])
        } else {
            format!("{} '{}' cannot be dropped because it has {} dependent objects",
                    object_type, object_name, dependents.len())
        };

        Self::new(
            DdlErrorCode::TableDependencyExists, // Using table dependency as general case
            message,
            DdlOperation::Drop,
            object_type,
            Some(object_name.to_string()),
        ).with_recovery(RecoverySuggestion::DropDependents(dependents))
    }

    /// Invalid constraint definition error
    pub fn invalid_constraint_definition(constraint_name: &str, reason: &str) -> Self {
        Self::new(
            DdlErrorCode::InvalidConstraintDefinition,
            format!("Invalid constraint definition '{}': {}", constraint_name, reason),
            DdlOperation::Create,
            DdlObjectType::Constraint,
            Some(constraint_name.to_string()),
        )
    }

    /// Concurrent DDL conflict error
    pub fn concurrent_ddl_conflict(object_name: &str, operation: DdlOperation) -> Self {
        Self::new(
            DdlErrorCode::ConcurrentDdlConflict,
            format!("Concurrent DDL operation detected on '{}'", object_name),
            operation,
            DdlObjectType::Table, // Most common case
            Some(object_name.to_string()),
        )
    }

    /// Unsupported DDL operation error
    pub fn unsupported_operation(operation: &str, reason: &str) -> Self {
        Self::new(
            DdlErrorCode::UnsupportedDdlOperation,
            format!("Unsupported DDL operation '{}': {}", operation, reason),
            DdlOperation::Create, // Generic operation
            DdlObjectType::Table, // Generic object type
            None,
        )
    }

    /// DDL transaction error
    pub fn ddl_transaction(message: String) -> Self {
        Self::new(
            DdlErrorCode::DdlTransactionFailed,
            message,
            DdlOperation::Create, // Generic operation
            DdlObjectType::Table, // Generic object type
            None,
        )
    }

    /// Invalid default value error
    pub fn invalid_default_value(column_name: &str, reason: &str) -> Self {
        Self::new(
            DdlErrorCode::InvalidColumnDefinition,
            format!("Invalid default value for column '{}': {}", column_name, reason),
            DdlOperation::Create,
            DdlObjectType::Column,
            Some(column_name.to_string()),
        )
    }

    /// Invalid check condition error
    pub fn invalid_check_condition(reason: &str) -> Self {
        Self::new(
            DdlErrorCode::InvalidConstraintDefinition,
            format!("Invalid CHECK condition: {}", reason),
            DdlOperation::Create,
            DdlObjectType::Constraint,
            None,
        )
    }

    /// Constraint not found error
    pub fn constraint_not_found(constraint_name: &str, reason: &str) -> Self {
        Self::new(
            DdlErrorCode::ConstraintNotFound,
            format!("Constraint '{}' not found: {}", constraint_name, reason),
            DdlOperation::Alter,
            DdlObjectType::Constraint,
            Some(constraint_name.to_string()),
        )
    }

    /// Column in use error
    pub fn column_in_use(column_name: &str, table_name: &str) -> Self {
        Self::new(
            DdlErrorCode::ColumnInUse,
            format!("Column '{}' cannot be dropped because it is referenced by constraints or indexes", column_name),
            DdlOperation::Alter,
            DdlObjectType::Column,
            Some(column_name.to_string()),
        ).with_context("table_name".to_string(), table_name.to_string())
    }

    /// Constraint already exists error
    pub fn constraint_already_exists(constraint_name: &str, reason: &str) -> Self {
        Self::new(
            DdlErrorCode::ConstraintAlreadyExists,
            format!("Constraint '{}' already exists: {}", constraint_name, reason),
            DdlOperation::Alter,
            DdlObjectType::Constraint,
            Some(constraint_name.to_string()),
        )
    }
}

/// DDL error utilities
pub struct DdlErrorUtils;

impl DdlErrorUtils {
    /// Check if an error is a retryable DDL error
    pub fn is_retryable_error(error: &RustgreSQLError) -> bool {
        match error {
            RustgreSQLError::Ddl(msg) => {
                msg.contains("concurrent") ||
                msg.contains("timeout") ||
                msg.contains("in use")
            }
            _ => false,
        }
    }

    /// Extract DDL error code from error message if available
    pub fn extract_error_code(error: &RustgreSQLError) -> Option<DdlErrorCode> {
        match error {
            RustgreSQLError::Ddl(msg) => {
                // Try to extract error code from message format "DDL Error XXXX:"
                if let Some(start) = msg.find("DDL Error ") {
                    if let Some(end) = msg[start..].find(':') {
                        let code_str = &msg[start + 10..start + end];
                        if let Ok(code_num) = code_str.parse::<u32>() {
                            return match code_num {
                                4000 => Some(DdlErrorCode::TableNotFound),
                                4001 => Some(DdlErrorCode::TableAlreadyExists),
                                4100 => Some(DdlErrorCode::ColumnNotFound),
                                4101 => Some(DdlErrorCode::ColumnAlreadyExists),
                                4200 => Some(DdlErrorCode::ConstraintNotFound),
                                4201 => Some(DdlErrorCode::ConstraintAlreadyExists),
                                4202 => Some(DdlErrorCode::ConstraintViolation),
                                4300 => Some(DdlErrorCode::IndexNotFound),
                                4301 => Some(DdlErrorCode::IndexAlreadyExists),
                                _ => None,
                            };
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Create a formatted error response for client applications
    pub fn format_client_error(error: &DdlError) -> serde_json::Value {
        let recovery_suggestion = match &error.recovery_suggestion {
            RecoverySuggestion::UseIfExists => "Use IF EXISTS clause".to_string(),
            RecoverySuggestion::UseCascade => "Use CASCADE option".to_string(),
            RecoverySuggestion::RetryAfterDelay(ms) => format!("Retry after {}ms", ms),
            RecoverySuggestion::DropDependents(deps) => format!("Drop dependent objects: {:?}", deps),
            RecoverySuggestion::CheckExistence => "Check object existence first".to_string(),
            RecoverySuggestion::UseDifferentName => "Use a different name".to_string(),
            RecoverySuggestion::ContactAdministrator => "Contact database administrator".to_string(),
            RecoverySuggestion::Custom(msg) => msg.clone(),
            RecoverySuggestion::None => "No specific suggestion available".to_string(),
        };

        serde_json::json!({
            "error": {
                "code": error.code as i32,
                "message": &error.message,
                "operation": format!("{}", error.operation),
                "object_type": format!("{}", error.object_type),
                "object_name": &error.object_name,
                "sql_state": &error.sql_state,
                "recovery_suggestion": recovery_suggestion,
                "context": error.context
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ddl_error_creation() {
        let error = DdlError::table_not_found("users", DdlOperation::Drop);

        assert_eq!(error.code, DdlErrorCode::TableNotFound);
        assert_eq!(error.operation, DdlOperation::Drop);
        assert_eq!(error.object_type, DdlObjectType::Table);
        assert_eq!(error.object_name, Some("users".to_string()));
    }

    #[test]
    fn test_ddl_error_with_context() {
        let error = DdlError::column_not_found("email", "users", DdlOperation::Alter)
            .with_context("schema".to_string(), "public".to_string());

        assert_eq!(error.context.get("table_name"), Some(&"users".to_string()));
        assert_eq!(error.context.get("schema"), Some(&"public".to_string()));
    }

    #[test]
    fn test_dependency_error() {
        let dependents = vec!["orders_user_id_fkey".to_string()];
        let error = DdlError::dependency_exists("users", DdlObjectType::Table, dependents.clone());

        match error.recovery_suggestion {
            RecoverySuggestion::DropDependents(ref deps) => {
                assert_eq!(*deps, dependents);
            }
            _ => panic!("Expected DropDependents recovery suggestion"),
        }
    }

    #[test]
    fn test_error_conversion() {
        let ddl_error = DdlError::table_already_exists("products");
        let rustgresql_error: RustgreSQLError = ddl_error.into();

        match rustgresql_error {
            RustgreSQLError::Ddl(msg) => {
                assert!(msg.contains("products"));
                assert!(msg.contains("already exists"));
            }
            _ => panic!("Expected Ddl error variant"),
        }
    }
}