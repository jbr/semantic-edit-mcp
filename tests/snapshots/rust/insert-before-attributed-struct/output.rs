use std::fmt::Debug;

/// Marker trait.
pub trait Entity {}
/// A simple user record.
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
}
