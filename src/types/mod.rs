//! Type system module
//!
//! Defines data types and type conversion

pub mod data_type;
pub mod value;
pub mod convert;

pub use data_type::{DataType, DataTypeKind};
pub use value::{Value, ValueKind, NullValue};
pub use convert::{TypeConverter, ConversionError};