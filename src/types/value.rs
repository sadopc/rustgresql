//! Value representations

use crate::Result;
use chrono::{DateTime, Utc};
use std::hash::{Hash, Hasher};

/// Null value
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NullValue;

/// Value kind
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ValueKind {
    Integer(i64),
    String(String),
    Boolean(bool),
    Float(f64),
    Timestamp(chrono::DateTime<chrono::Utc>),
    List(Vec<Value>),
    Null(NullValue),
}

impl Eq for ValueKind {}

impl Hash for ValueKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ValueKind::Integer(i) => {
                0u8.hash(state);
                i.hash(state);
            }
            ValueKind::String(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            ValueKind::Boolean(b) => {
                2u8.hash(state);
                b.hash(state);
            }
            ValueKind::Float(f) => {
                3u8.hash(state);
                // Convert float to bits for hashing to handle NaN correctly
                f.to_bits().hash(state);
            }
            ValueKind::Timestamp(t) => {
                4u8.hash(state);
                t.hash(state);
            }
            ValueKind::List(list) => {
                5u8.hash(state);
                list.hash(state);
            }
            ValueKind::Null(n) => {
                6u8.hash(state);
                n.hash(state);
            }
        }
    }
}

/// Value
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Value {
    pub kind: ValueKind,
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
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

    pub fn timestamp(value: DateTime<Utc>) -> Self {
        Self { kind: ValueKind::Timestamp(value) }
    }

    pub fn list(values: Vec<Value>) -> Self {
        Self { kind: ValueKind::List(values) }
    }

    pub fn null() -> Self {
        Self { kind: ValueKind::Null(NullValue) }
    }
}
