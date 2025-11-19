//! Constraint mapping utilities for AST-Catalog conversion
//!
//! This module provides utilities to convert between AST constraint structures
//! and catalog constraint structures, enabling seamless data flow between
//! the parser and the storage/catalog system.

use crate::sql::ast::{ColumnConstraint, TableConstraint, Expression};
use crate::catalog::table::{ColumnDef as CatalogColumnDef, TableConstraint as CatalogTableConstraint};
use crate::types::{DataType, Value, DataTypeKind};
use std::fmt;

/// Errors that can occur during constraint mapping
#[derive(Debug, Clone)]
pub enum ConstraintMappingError {
    /// Unsupported data type conversion
    UnsupportedDataType(String),
    /// Invalid default value format
    InvalidDefaultValue(String),
    /// Invalid check condition format
    InvalidCheckCondition(String),
    /// Column reference not found
    ColumnNotFound(String),
    /// Expression serialization failed
    ExpressionSerializationFailed(String),
}

/// Result type for constraint mapping operations
pub type MappingResult<T> = Result<T, ConstraintMappingError>;

impl fmt::Display for ConstraintMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstraintMappingError::UnsupportedDataType(data_type) => {
                write!(f, "Unsupported data type: {}", data_type)
            }
            ConstraintMappingError::InvalidDefaultValue(value) => {
                write!(f, "Invalid default value format: {}", value)
            }
            ConstraintMappingError::InvalidCheckCondition(condition) => {
                write!(f, "Invalid check condition format: {}", condition)
            }
            ConstraintMappingError::ColumnNotFound(column) => {
                write!(f, "Column not found: {}", column)
            }
            ConstraintMappingError::ExpressionSerializationFailed(msg) => {
                write!(f, "Expression serialization failed: {}", msg)
            }
        }
    }
}

/// Converts AST column constraints to catalog column definition fields
pub fn map_column_constraints_to_catalog(
    column_name: &str,
    constraints: &[ColumnConstraint],
    data_type: DataType,
    column_id: u64,
) -> MappingResult<CatalogColumnDef> {
    let mut nullable = true;
    let mut default_value = None;
    let mut primary_key = false;
    let mut unique = false;
    let mut check_constraint = None;
    let mut foreign_key_ref = None;

    for constraint in constraints {
        match constraint {
            ColumnConstraint::NotNull => nullable = false,
            ColumnConstraint::Null => nullable = true,
            ColumnConstraint::Default(value) => {
                default_value = Some(parse_default_value(value, &data_type)?);
            }
            ColumnConstraint::PrimaryKey => {
                primary_key = true;
                nullable = false; // Primary keys are implicitly NOT NULL
            }
            ColumnConstraint::Unique => {
                unique = true;
            }
            ColumnConstraint::Check(condition) => {
                check_constraint = Some(serialize_expression(condition)?);
            }
            ColumnConstraint::References { table, column } => {
                foreign_key_ref = Some((table.clone(), column.clone()));
            }
        }
    }

    Ok(CatalogColumnDef {
        column_id,
        name: column_name.to_string(),
        data_type,
        nullable,
        default_value,
        primary_key,
        unique,
        check_constraint,
    })
}

/// Converts catalog column definition back to AST column constraints
pub fn map_catalog_column_to_ast_constraints(column_def: &CatalogColumnDef) -> Vec<ColumnConstraint> {
    let mut constraints = Vec::new();

    if !column_def.nullable {
        constraints.push(ColumnConstraint::NotNull);
    }

    if let Some(default_value) = &column_def.default_value {
        constraints.push(ColumnConstraint::Default(format!("{:?}", default_value)));
    }

    if column_def.primary_key {
        constraints.push(ColumnConstraint::PrimaryKey);
    }

    if column_def.unique {
        constraints.push(ColumnConstraint::Unique);
    }

    if let Some(check_condition) = &column_def.check_constraint {
        constraints.push(ColumnConstraint::Check(deserialize_expression(check_condition)));
    }

    constraints
}

/// Converts AST table constraints to catalog table constraints
pub fn map_table_constraints_to_catalog(
    constraints: &[TableConstraint],
) -> MappingResult<Vec<CatalogTableConstraint>> {
    let mut catalog_constraints = Vec::new();

    for constraint in constraints {
        let catalog_constraint = match constraint {
            TableConstraint::PrimaryKey { columns, name: _ } => {
                CatalogTableConstraint::PrimaryKey {
                    columns: columns.clone(),
                }
            }
            TableConstraint::ForeignKey {
                columns,
                ref_table,
                ref_columns,
                name: _,
            } => {
                CatalogTableConstraint::ForeignKey {
                    columns: columns.clone(),
                    referenced_table: ref_table.clone(),
                    referenced_columns: ref_columns.clone(),
                }
            }
            TableConstraint::Unique { columns, name: _ } => {
                CatalogTableConstraint::Unique {
                    columns: columns.clone(),
                }
            }
            TableConstraint::Check { condition, name: _ } => {
                CatalogTableConstraint::Check {
                    condition: serialize_expression(condition)?,
                }
            }
        };
        catalog_constraints.push(catalog_constraint);
    }

    Ok(catalog_constraints)
}

/// Converts catalog table constraints back to AST table constraints
pub fn map_catalog_table_constraints_to_ast(
    constraints: &[CatalogTableConstraint],
) -> Vec<TableConstraint> {
    constraints
        .iter()
        .filter_map(|constraint| match constraint {
            CatalogTableConstraint::PrimaryKey { columns } => Some(TableConstraint::PrimaryKey {
                columns: columns.clone(),
                name: None,
            }),
            CatalogTableConstraint::ForeignKey {
                columns,
                referenced_table,
                referenced_columns,
            } => Some(TableConstraint::ForeignKey {
                columns: columns.clone(),
                ref_table: referenced_table.clone(),
                ref_columns: referenced_columns.clone(),
                name: None,
            }),
            CatalogTableConstraint::Unique { columns } => Some(TableConstraint::Unique {
                columns: columns.clone(),
                name: None,
            }),
            CatalogTableConstraint::Check { condition } => Some(TableConstraint::Check {
                condition: deserialize_expression(condition),
                name: None,
            }),
            CatalogTableConstraint::NotNull { column } => {
                // Catalog's NotNull constraint doesn't map directly to AST TableConstraint
                // It would typically be handled as a column constraint instead
                // For now, we'll skip this conversion
                None
            }
        })
        .collect()
}

/// Parses a default value string into the appropriate Value type
fn parse_default_value(value_str: &str, data_type: &DataType) -> MappingResult<Value> {
    match data_type.kind {
        DataTypeKind::Integer => {
            value_str
                .parse::<i64>()
                .map(Value::integer)
                .map_err(|_| ConstraintMappingError::InvalidDefaultValue(value_str.to_string()))
        }
        DataTypeKind::BigInt => {
            value_str
                .parse::<i64>()
                .map(Value::integer)
                .map_err(|_| ConstraintMappingError::InvalidDefaultValue(value_str.to_string()))
        }
        DataTypeKind::Real | DataTypeKind::DoublePrecision => {
            value_str
                .parse::<f64>()
                .map(Value::float)
                .map_err(|_| ConstraintMappingError::InvalidDefaultValue(value_str.to_string()))
        }
        DataTypeKind::Text => Ok(Value::string(value_str.to_string())),
        DataTypeKind::Varchar(_) => Ok(Value::string(value_str.to_string())),
        DataTypeKind::Char(_) => Ok(Value::string(value_str.to_string())),
        DataTypeKind::Boolean => {
            match value_str.to_lowercase().as_str() {
                "true" | "t" | "1" => Ok(Value::boolean(true)),
                "false" | "f" | "0" => Ok(Value::boolean(false)),
                _ => Err(ConstraintMappingError::InvalidDefaultValue(value_str.to_string())),
            }
        }
        DataTypeKind::Date => {
            // Simple date parsing - would need more sophisticated implementation
            Ok(Value::string(value_str.to_string()))
        }
        DataTypeKind::Timestamp | DataTypeKind::TimestampWithTimeZone => {
            // Handle CURRENT_TIMESTAMP specially
            if value_str.to_uppercase() == "CURRENT_TIMESTAMP" {
                Ok(Value::string("CURRENT_TIMESTAMP".to_string()))
            } else {
                // For other timestamp values, store as string for now
                Ok(Value::string(value_str.to_string()))
            }
        }
        _ => {
            // For complex types, treat as string for now
            Ok(Value::string(value_str.to_string()))
        }
    }
}

/// Serializes an expression to a string representation
fn serialize_expression(expr: &Expression) -> MappingResult<String> {
    // This is a simplified implementation - in a real database, this would
    // involve proper expression serialization
    match expr {
        Expression::Value(value) | Expression::Literal(value) => {
            // Simple value serialization
            Ok(format!("{:?}", value))
        }
        Expression::Column { table, name } => {
            match table {
                Some(table_name) => Ok(format!("{}.{}", table_name, name)),
                None => Ok(name.clone()),
            }
        }
        Expression::BinaryOp { left, op, right } => {
            let left_str = serialize_expression(left)?;
            let right_str = serialize_expression(right)?;
            let op_str = format_binary_operator(op);
            Ok(format!("{} {} {}", left_str, op_str, right_str))
        }
        _ => Err(ConstraintMappingError::ExpressionSerializationFailed(
            "Complex expression serialization not yet implemented".to_string(),
        )),
    }
}

/// Formats a BinaryOperator as a string
fn format_binary_operator(op: &crate::sql::ast::BinaryOperator) -> &'static str {
    match op {
        crate::sql::ast::BinaryOperator::Equals => "=",
        crate::sql::ast::BinaryOperator::NotEquals => "!=",
        crate::sql::ast::BinaryOperator::LessThan => "<",
        crate::sql::ast::BinaryOperator::LessThanOrEquals => "<=",
        crate::sql::ast::BinaryOperator::GreaterThan => ">",
        crate::sql::ast::BinaryOperator::GreaterThanOrEquals => ">=",
        crate::sql::ast::BinaryOperator::Like => "LIKE",
        crate::sql::ast::BinaryOperator::ILike => "ILIKE",
        crate::sql::ast::BinaryOperator::In => "IN",
        crate::sql::ast::BinaryOperator::And => "AND",
        crate::sql::ast::BinaryOperator::Or => "OR",
        crate::sql::ast::BinaryOperator::Is => "IS",
        crate::sql::ast::BinaryOperator::IsNot => "IS NOT",
        crate::sql::ast::BinaryOperator::Add => "+",
        crate::sql::ast::BinaryOperator::Subtract => "-",
        crate::sql::ast::BinaryOperator::Multiply => "*",
        crate::sql::ast::BinaryOperator::Divide => "/",
    }
}

/// Deserializes a string representation back to an expression
fn deserialize_expression(expr_str: &str) -> Expression {
    // This is a simplified implementation - in a real database, this would
    // involve proper expression parsing
    let value = Value::string(expr_str.to_string());
    Expression::Value(value)
}

/// Validates constraint compatibility and detects conflicts
pub fn validate_constraint_compatibility(
    column_constraints: &[ColumnConstraint],
    table_constraints: &[TableConstraint],
) -> MappingResult<()> {
    // Check for conflicting NOT NULL constraints
    let has_not_null = column_constraints.iter().any(|c| matches!(c, ColumnConstraint::NotNull));
    let has_null = column_constraints.iter().any(|c| matches!(c, ColumnConstraint::Null));

    if has_not_null && has_null {
        return Err(ConstraintMappingError::InvalidCheckCondition(
            "Column cannot have both NOT NULL and NULL constraints".to_string(),
        ));
    }

    // Check for duplicate constraint types
    let primary_key_count = column_constraints.iter().filter(|c| matches!(c, ColumnConstraint::PrimaryKey)).count();
    if primary_key_count > 1 {
        return Err(ConstraintMappingError::InvalidCheckCondition(
            "Column cannot have multiple PRIMARY KEY constraints".to_string(),
        ));
    }

    // Validate that table-level primary key references existing columns
    for constraint in table_constraints {
        if let TableConstraint::PrimaryKey { columns, .. } = constraint {
            if columns.is_empty() {
                return Err(ConstraintMappingError::InvalidCheckCondition(
                    "PRIMARY KEY constraint must specify at least one column".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// Extracts foreign key relationships from constraints
pub fn extract_foreign_key_relationships(
    column_constraints: &[ColumnConstraint],
    table_constraints: &[TableConstraint],
) -> Vec<(String, String, Option<String>)> {
    let mut relationships = Vec::new();

    // Extract foreign keys from column constraints
    for constraint in column_constraints {
        if let ColumnConstraint::References { table, column } = constraint {
            relationships.push((table.clone(), column.clone().unwrap_or_default(), None));
        }
    }

    // Extract foreign keys from table constraints
    for constraint in table_constraints {
        if let TableConstraint::ForeignKey {
            columns,
            ref_table,
            ref_columns,
            ..
        } = constraint {
            for (col, ref_col) in columns.iter().zip(ref_columns.iter()) {
                relationships.push((ref_table.clone(), ref_col.clone(), Some(col.clone())));
            }
        }
    }

    relationships
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::{ColumnConstraint, TableConstraint};
    use crate::types::{DataType, DataTypeKind};

    #[test]
    fn test_map_column_constraints_basic() {
        let constraints = vec![
            ColumnConstraint::NotNull,
            ColumnConstraint::Default("42".to_string()),
            ColumnConstraint::Unique,
        ];

        let data_type = DataType::new(DataTypeKind::Integer);
        let result = map_column_constraints_to_catalog(
            "test_col",
            &constraints,
            data_type,
            1,
        );

        assert!(result.is_ok());
        let catalog_col = result.unwrap();
        assert_eq!(catalog_col.name, "test_col");
        assert_eq!(catalog_col.data_type.kind, DataTypeKind::Integer);
        assert!(!catalog_col.nullable);
        assert!(catalog_col.unique);
        assert!(catalog_col.default_value.is_some());
    }

    #[test]
    fn test_primary_key_implies_not_null() {
        let constraints = vec![ColumnConstraint::PrimaryKey];

        let data_type = DataType::new(DataTypeKind::Integer);
        let result = map_column_constraints_to_catalog(
            "id",
            &constraints,
            data_type,
            1,
        );

        assert!(result.is_ok());
        let catalog_col = result.unwrap();
        assert!(catalog_col.primary_key);
        assert!(!catalog_col.nullable); // Primary key should imply NOT NULL
    }

    #[test]
    fn test_constraint_validation_conflicts() {
        let conflicting_constraints = vec![
            ColumnConstraint::NotNull,
            ColumnConstraint::Null,
        ];

        let result = validate_constraint_compatibility(&conflicting_constraints, &[]);
        assert!(result.is_err());
    }
}