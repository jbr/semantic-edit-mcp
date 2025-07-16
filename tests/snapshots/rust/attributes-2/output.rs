use serde::{Deserialize, Serialize};

/// Administrative user with elevated privileges
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminUser {
    pub user: User,
    pub permissions: Vec<String>,
}
/// Represents a user in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: u64,
    pub username: String,
}

/// User profile information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfile {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}
