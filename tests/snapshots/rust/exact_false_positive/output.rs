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

/// User preferences and settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub theme: Theme,
    pub language: String,
    pub notifications: NotificationSettings,
    pub privacy: PrivacySettings,
}

/// Available themes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Theme {
    Light,
    Dark,
    Auto,
}

/// Notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email_notifications: bool,
    pub push_notifications: bool,
    pub sms_notifications: bool,
    pub frequency: NotificationFrequency,
}

/// How often to send notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationFrequency {
    Immediate,
    Hourly,
    Daily,
    Weekly,
    Never,
}

/// Privacy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub profile_visibility: ProfileVisibility,
    pub show_email: bool,
    pub show_last_seen: bool,
    pub allow_direct_messages: bool,
}

/// Who can see the user's profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfileVisibility {
    Public,
    FriendsOnly,
    Private,
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

    /// Updates the user's profile information
    pub fn update_profile(&mut self, profile: UserProfile) {
        self.profile = profile;
    }

    /// Checks if the user has completed their profile
    pub fn is_profile_complete(&self) -> bool {
        self.profile.first_name.is_some() && self.profile.last_name.is_some()
    }

    /// Gets the user's display name
    pub fn display_name(&self) -> String {
        match (&self.profile.first_name, &self.profile.last_name) {
            (Some(first), Some(last)) => format!("{} {}", first, last),
            (Some(first), None) => first.clone(),
            (None, Some(last)) => last.clone(),
            (None, None) => self.username.clone(),
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

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            language: "en".to_string(),
            notifications: NotificationSettings::default(),
            privacy: PrivacySettings::default(),
        }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            email_notifications: true,
            push_notifications: true,
            sms_notifications: false,
            frequency: NotificationFrequency::Immediate,
        }
    }
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            profile_visibility: ProfileVisibility::Public,
            show_email: false,
            show_last_seen: true,
            allow_direct_messages: true,
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Theme::Light => write!(f, "Light"),
            Theme::Dark => write!(f, "Dark"),
            Theme::Auto => write!(f, "Auto"),
        }
    }
}

/// User repository for database operations
pub struct UserRepository {
    users: HashMap<u64, User>,
    next_id: u64,
}

impl UserRepository {
    /// Creates a new user repository
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            next_id: 1,
        }
    }

    /// Adds a new user to the repository
    pub fn add_user(&mut self, username: String, email: String) -> Result<&User, String> {
        // Check if username already exists
        if self.users.values().any(|u| u.username == username) {
            return Err("Username already exists".to_string());
        }

        // Check if email already exists
        if self.users.values().any(|u| u.email == email) {
            return Err("Email already exists".to_string());
        }

        let id = self.next_id;
        self.next_id += 1;

        println!("Creating user with ID: {}", id);
        let user = User::new(id, username, email);
        self.users.insert(id, user);
        Ok(self.users.get(&id).unwrap())
    }

    /// Finds a user by ID
    pub fn find_by_id(&self, id: u64) -> Option<&User> {
        self.users.get(&id)
    }

    /// Finds a user by username
    pub fn find_by_username(&self, username: &str) -> Option<&User> {
        self.users.values().find(|u| u.username == username)
    }

    /// Updates a user's information
    pub fn update_user(&mut self, id: u64, user: User) -> Result<&User, String> {
        if !self.users.contains_key(&id) {
            return Err("User not found".to_string());
        }

        self.users.insert(id, user);
        Ok(self.users.get(&id).unwrap())
    }

    /// Deletes a user by ID
    pub fn delete_user(&mut self, id: u64) -> Result<(), String> {
        if self.users.remove(&id).is_none() {
            return Err("User not found".to_string());
        }
        Ok(())
    }

    /// Gets all active users
    pub fn get_active_users(&self) -> Vec<&User> {
        self.users.values().filter(|u| u.is_active).collect()
    }

    /// Gets the total number of users
    pub fn count(&self) -> usize {
        self.users.len()
    }
    /// Gets users by their activity status
    pub fn get_users_by_status(&self, is_active: bool) -> Vec<&User> {
        self.users
            .values()
            .filter(|u| u.is_active == is_active)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new(1, "testuser".to_string(), "test@example.com".to_string());
        assert_eq!(user.id, 1);
        assert_eq!(user.username, "testuser");
        assert_eq!(user.email, "test@example.com");
        assert!(user.is_active);
    }

    #[test]
    fn test_display_name() {
        let mut user = User::new(1, "testuser".to_string(), "test@example.com".to_string());

        // Should return username when no first/last name
        assert_eq!(user.display_name(), "testuser");

        // Should return first name only
        user.profile.first_name = Some("John".to_string());
        assert_eq!(user.display_name(), "John");

        // Should return full name
        user.profile.last_name = Some("Doe".to_string());
        assert_eq!(user.display_name(), "John Doe");
    }

    #[test]
    fn test_user_repository() {
        let mut repo = UserRepository::new();

        // Test user creation
        let user = repo
            .add_user("testuser".to_string(), "test@example.com".to_string())
            .unwrap();
        assert_eq!(user.username, "testuser");

        // Test duplicate username
        let result = repo.add_user("testuser".to_string(), "other@example.com".to_string());
        assert!(result.is_err());

        // Test finding user
        let found = repo.find_by_username("testuser");
        assert!(found.is_some());
        assert_eq!(found.unwrap().email, "test@example.com");
    }
    #[test]
    fn test_email_validation() {
        let user = User::new(1, "testuser".to_string(), "invalid_email".to_string());
        assert!(user.validate_email().is_err());

        let user2 = User::new(2, "testuser2".to_string(), "valid@example.com".to_string());
        assert!(user2.validate_email().is_ok());
    }
    #[test]
    fn test_user_status_filtering() {
        let mut repo = UserRepository::new();
        let _user1 = repo
            .add_user("active_user".to_string(), "active@example.com".to_string())
            .unwrap();

        let mut user2 = User::new(
            2,
            "inactive_user".to_string(),
            "inactive@example.com".to_string(),
        );
        user2.is_active = false;
        repo.users.insert(2, user2);
        repo.next_id = 3;

        let active_users = repo.get_users_by_status(true);
        let inactive_users = repo.get_users_by_status(false);

        assert_eq!(active_users.len(), 1);
        assert_eq!(inactive_users.len(), 1);
        assert_eq!(active_users[0].username, "active_user");
        assert_eq!(inactive_users[0].username, "inactive_user");
    }
    #[test]
    fn test_user_count() {
        let mut repo = UserRepository::new();
        assert_eq!(repo.count(), 0);

        repo.add_user("user1".to_string(), "user1@example.com".to_string())
            .unwrap();
        assert_eq!(repo.count(), 1);

        repo.add_user("user2".to_string(), "user2@example.com".to_string())
            .unwrap();
        assert_eq!(repo.count(), 2);
    }
}
