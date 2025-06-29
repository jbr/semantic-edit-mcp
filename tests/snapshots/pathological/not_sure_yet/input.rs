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
    pub phone_number: Option<String>,
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
    /// Enhanced email validation with detailed error messages
    pub fn validate_email_strict(&self) -> Result<(), String> {
        let email = &self.email;

        if email.is_empty() {
            return Err("Email cannot be empty".to_string());
        }

        if !email.contains('@') {
            return Err("Invalid email format: missing @ symbol".to_string());
        }

        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 {
            return Err("Invalid email format: multiple @ symbols".to_string());
        }

        let (local, domain) = (parts[0], parts[1]);

        if local.is_empty() {
            return Err("Invalid email format: empty local part".to_string());
        }

        if domain.is_empty() || !domain.contains('.') {
            return Err("Invalid email format: invalid domain".to_string());
        }

        Ok(())
    }
    /// Checks if user has any notification preferences enabled
    pub fn has_notifications_enabled(&self) -> bool {
        self.profile.preferences.notifications.email_notifications
            || self.profile.preferences.notifications.push_notifications
            || self.profile.preferences.notifications.sms_notifications
    }

    /// Deactivates the user account
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    /// Reactivates the user account
    pub fn reactivate(&mut self) {
        self.is_active = true;
    }

    /// Gets the age of the account in days
    pub fn account_age_days(&self) -> i64 {
        let now = chrono::Utc::now();
        (now - self.created_at).num_days()
    }
    /// Checks if the user prefers dark mode theme
    pub fn prefers_dark_mode(&self) -> bool {
        matches!(self.profile.preferences.theme, Theme::Dark)
    }

    /// Gets user's contact info in a formatted string
    pub fn get_contact_summary(&self) -> String {
        match &self.phone_number {
            Some(phone) => format!("📧 {} | 📞 {}", self.email, phone),
            None => format!("📧 {}", self.email),
        }
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
            sms_notifications: true,
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

    /// Finds users by email domain
    pub fn find_by_email_domain(&self, domain: &str) -> Vec<&User> {
        self.users
            .values()
            .filter(|u| u.email.ends_with(&format!("@{}", domain)))
            .collect()
    }
    /// Gets users by their activity status
    pub fn get_users_by_status(&self, is_active: bool) -> Vec<&User> {
        self.users
            .values()
            .filter(|u| u.is_active == is_active)
            .collect()
    }
    /// Gets all users with profile pictures set
    pub fn get_users_with_avatars(&self) -> Vec<&User> {
        self.users
            .values()
            .filter(|u| u.profile.avatar_url.is_some())
            .collect()
    }

    /// Searches users by partial name match (first name, last name, or username)
    pub fn search_users(&self, query: &str) -> Vec<&User> {
        let query_lower = query.to_lowercase();
        self.users
            .values()
            .filter(|u| {
                u.username.to_lowercase().contains(&query_lower)
                    || u.profile
                        .first_name
                        .as_ref()
                        .map_or(false, |name| name.to_lowercase().contains(&query_lower))
                    || u.profile
                        .last_name
                        .as_ref()
                        .map_or(false, |name| name.to_lowercase().contains(&query_lower))
            })
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
    fn test_find_by_email_domain() {
        let mut repo = UserRepository::new();
        repo.add_user("user1".to_string(), "user1@example.com".to_string())
            .unwrap();
        repo.add_user("user2".to_string(), "user2@test.com".to_string())
            .unwrap();
        repo.add_user("user3".to_string(), "user3@example.com".to_string())
            .unwrap();

        let example_users = repo.find_by_email_domain("example.com");
        let test_users = repo.find_by_email_domain("test.com");

        assert_eq!(example_users.len(), 2);
        assert_eq!(test_users.len(), 1);
        assert_eq!(test_users[0].username, "user2");
    }
    #[test]
    fn test_user_activation() {
        let mut user = User::new(1, "testuser".to_string(), "test@example.com".to_string());
        assert!(user.is_active);

        user.deactivate();
        assert!(!user.is_active);

        user.reactivate();
        assert!(user.is_active);
    }

    #[test]
    fn test_account_age() {
        let user = User::new(1, "testuser".to_string(), "test@example.com".to_string());
        let age = user.account_age_days();
        assert!(age >= 0); // Should be 0 or positive
    }

    #[test]
    fn test_new_user_methods() {
        let mut user = User::new(1, "testuser".to_string(), "test@example.com".to_string());

        // Test dark mode preference (should default to Auto, not Dark)
        assert!(!user.prefers_dark_mode());

        // Test contact summary without phone
        let summary = user.get_contact_summary();
        assert!(summary.contains("📧 test@example.com"));
        assert!(!summary.contains("📞"));

        // Add phone number and test again
        user.phone_number = Some("+1234567890".to_string());
        let summary_with_phone = user.get_contact_summary();
        assert!(summary_with_phone.contains("📧 test@example.com"));
        assert!(summary_with_phone.contains("📞 +1234567890"));
    }

    #[test]
    fn test_user_repository_search() {
        let mut repo = UserRepository::new();
        let mut user1 = User::new(1, "johndoe".to_string(), "john@example.com".to_string());
        user1.profile.first_name = Some("John".to_string());
        user1.profile.last_name = Some("Doe".to_string());

        let mut user2 = User::new(2, "janedoe".to_string(), "jane@example.com".to_string());
        user2.profile.first_name = Some("Jane".to_string());
        user2.profile.last_name = Some("Doe".to_string());

        repo.users.insert(1, user1);
        repo.users.insert(2, user2);
        repo.next_id = 3;

        // Search by first name
        let john_results = repo.search_users("john");
        assert_eq!(john_results.len(), 1);
        assert_eq!(john_results[0].username, "johndoe");

        // Search by last name
        let doe_results = repo.search_users("doe");
        assert_eq!(doe_results.len(), 2);

        // Search by username
        let username_results = repo.search_users("jane");
        assert_eq!(username_results.len(), 1);
        assert_eq!(username_results[0].username, "janedoe");
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
