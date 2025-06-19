pub fn validate_email(email: &str) -> Result<(), String> {
    if !email.contains('@') {
        return Err("Invalid email format".to_string());
    }
    Ok(())
}