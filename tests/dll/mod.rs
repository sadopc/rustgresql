//! DDL Test Module
//!
//! Comprehensive test suite for DDL (Data Definition Language) functionality.
//! Includes unit tests, integration tests, performance tests, and error handling tests.

pub mod unit_tests;
pub mod integration_tests;

// Re-export commonly used test utilities
pub use unit_tests::*;
pub use integration_tests::*;