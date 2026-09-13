/// The request envelope, shared source for both renderers.
fn request_value(
    model: &str,
    limit: usize,
) -> String {
    format!("{model}-{limit}")
}

/// Render the request as YAML.
pub fn render_request_yaml(model: &str) -> String {
    request_value(model, 1)
}
