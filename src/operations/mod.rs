//! Operations module for semantic editing
//!
//! This module provides the core text-anchored node selection and editing operations.
//! The design uses content as anchor points and AST structure for precise targeting.

pub mod edit_operation;
pub mod selector;

// Re-export main types for convenience
pub use edit_operation::{EditOperation, ExecutionResult};
pub use selector::Selector;
