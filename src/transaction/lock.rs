//! Lock manager

use crate::Result;

/// Lock type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockType {
    Shared,
    Exclusive,
}

/// Lock mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LockMode {
    Immediate,
    Wait,
}

/// Lock manager
#[derive(Debug)]
pub struct LockManager;

impl LockManager {
    pub fn new() -> Self {
        Self
    }
}
