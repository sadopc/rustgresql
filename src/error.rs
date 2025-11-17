use thiserror::Error;

/// Database error types
#[derive(Error, Debug)]
pub enum RustgreSQLError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Page not found: {0}")]
    PageNotFound(PageId),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Type error: {0}")]
    Type(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Database corruption detected: {0}")]
    Corruption(String),

    #[error("Feature not implemented: {0}")]
    NotImplemented(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    // DDL-specific error types
    #[error("DDL error: {0}")]
    Ddl(String),

    #[error("Table error: {0}")]
    Table(String),

    #[error("Column error: {0}")]
    Column(String),

    #[error("Constraint error: {0}")]
    Constraint(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Dependency error: {0}")]
    Dependency(String),

    #[error("Transaction error in DDL: {0}")]
    DdlTransaction(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Dependent objects: {0}")]
    DependentObjects(String),

    #[error("Procedure error: {0}")]
    Procedure(String),
}

pub type Result<T> = std::result::Result<T, RustgreSQLError>;

/// Re-export PageId for use in error types
use crate::PageId;

impl From<crate::catalog::constraint_mapping::ConstraintMappingError> for RustgreSQLError {
    fn from(error: crate::catalog::constraint_mapping::ConstraintMappingError) -> Self {
        RustgreSQLError::Validation(format!("Constraint mapping error: {}", error))
    }
}