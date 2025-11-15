//! Value representations

use crate::Result;

/// Null value
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullValue;

/// Value kind
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ValueKind {
    Integer(i64),
    String(String),
    Boolean(bool),
    Float(f64),
    Null(NullValue),
}

/// Value
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Value {
    pub kind: ValueKind,
}

impl Value {
    pub fn integer(value: i64) -> Self {
        Self { kind: ValueKind::Integer(value) }
    }

    pub fn string(value: String) -> Self {
        Self { kind: ValueKind::String(value) }
    }

    pub fn boolean(value: bool) -> Self {
        Self { kind: ValueKind::Boolean(value) }
    }

    pub fn float(value: f64) -> Self {
        Self { kind: ValueKind::Float(value) }
    }

    pub fn null() -> Self {
        Self { kind: ValueKind::Null(NullValue) }
    }
}
