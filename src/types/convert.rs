//! Type conversion

use crate::Result;

/// Conversion error
#[derive(Debug)]
pub struct ConversionError(String);

/// Type converter
#[derive(Debug)]
pub struct TypeConverter;

impl TypeConverter {
    pub fn new() -> Self {
        Self
    }
}
