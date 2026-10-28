fn render(header: &str) -> String {
    if header.is_empty() {
        String::new()
    } else {
        let body = r#"
line one
  line two
"#;
        let footer = "end";
        format!("{header}{body}")
    }
}
