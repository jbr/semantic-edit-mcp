use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Represents a user in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub username: String,
    pub email: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_active: bool,
    pub profile: UserProfile,
}

/// User profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub preferences: UserPreferences,
}

impl User {
    /// Creates a new user with default settings
    pub fn new(id: u64, username: String, email: String) -> Self {
        Self {
            id,
            username,
            email,
            created_at: chrono::Utc::now(),
            is_active: true,
            profile: UserProfile::default(),
        }
    }

    /// Validates the user's email format
    pub fn validate_email(&self) -> Result<(), String> {
        if !self.email.contains('@') {
            return Err("Invalid email format: missing @ symbol".to_string());
        }
        Ok(())
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            first_name: None,
            last_name: None,
            bio: None,
            avatar_url: None,
            preferences: UserPreferences::default(),
        }
    }
}
