use std::fmt::Debug;

/// A simple user record.
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
}
