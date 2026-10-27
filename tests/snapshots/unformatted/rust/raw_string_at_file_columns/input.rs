fn render(header: &str) -> String {
    if header.is_empty() {
        String::new()
    } else {
        let body = "old";
        format!("{header}{body}")
    }
}
