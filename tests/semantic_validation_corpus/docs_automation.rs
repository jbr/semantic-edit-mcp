// docs-gen/src/main.rs
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
struct CodeExample {
    input_start: usize,
    input_end: usize,
    input_code: String,
    output_start: usize,
    output_end: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docs_path = "docs.md";
    let content = fs::read_to_string(docs_path)?;

    let examples = find_expandable_examples(&content)?;
    let mut new_content = content.clone();

    // Process from end to beginning to keep indices valid
    for example in examples.iter().rev() {
        let expanded = expand_code(&example.input_code)?;
        let formatted = format_expanded_output(&expanded)?;

        // Replace the output section
        new_content.replace_range(example.output_start..example.output_end, &formatted);
    }

    fs::write(docs_path, new_content)?;
    println!("Updated {} examples in {}", examples.len(), docs_path);

    Ok(())
}

fn find_expandable_examples(content: &str) -> Result<Vec<CodeExample>, Box<dyn std::error::Error>> {
    use regex::Regex;

    const PATTERN: &str = r"(?s)```rust\n(.*?)\n```\s*\n\s*generates:\s*\n\s*```rust\n(.*?)\n```";
    let mut examples = Vec::new();

    // Regex to find: ```rust ... ``` generates: ```rust ... ```
    let pattern = Regex::new(PATTERN)?;

    for captures in pattern.captures_iter(content) {
        let full_match = captures.get(0).unwrap();
        let input_code = captures.get(1).unwrap().as_str().to_string();
        let output_section = captures.get(2).unwrap();

        // Find the actual positions in the original string
        let input_start = full_match.start();
        let input_end = captures.get(1).unwrap().end() + 4; // +4 for "\n```"
        let output_start = output_section.start();
        let output_end = output_section.end();

        examples.push(CodeExample {
            input_start,
            input_end,
            input_code,
            output_start,
            output_end,
        });
    }

    Ok(examples)
}

fn expand_code(input: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Create a temporary crate
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();

    // Write Cargo.toml
    let cargo_toml = format!(
        r#"
[package]
name = "fieldwork-expand-temp"
version = "0.1.0"
edition = "2021"

[dependencies]
fieldwork = {{ path = "{}" }}
"#,
        std::env::current_dir()?.display()
    );

    fs::write(temp_path.join("Cargo.toml"), cargo_toml)?;

    // Create src directory and main.rs
    fs::create_dir(temp_path.join("src"))?;
    let main_rs = format!("use fieldwork::Fieldwork;\n\n{}", input);
    fs::write(temp_path.join("src").join("main.rs"), main_rs)?;

    // Run cargo expand
    let output = Command::new("cargo")
        .current_dir(temp_path)
        .args(&["expand", "--bin", "fieldwork-expand-temp"])
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "cargo expand failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn format_expanded_output(expanded: &str) -> Result<String, Box<dyn std::error::Error>> {
    let lines: Vec<&str> = expanded.lines().collect();
    let mut result = Vec::new();

    result.push("// GENERATED - DO NOT EDIT".to_string());

    let mut in_impl = false;
    let mut in_struct = false;

    for line in lines {
        let trimmed = line.trim();

        // Skip use statements and other preamble
        if trimmed.starts_with("use ") || trimmed.starts_with("#!") {
            continue;
        }

        // Detect struct definitions and comment them out
        if trimmed.starts_with("struct ") || trimmed.starts_with("pub struct ") {
            in_struct = true;
            result.push(format!("# {}", line));
            continue;
        }

        // Detect impl blocks
        if trimmed.starts_with("impl ") {
            in_struct = false;
            in_impl = true;
            result.push(line.to_string());
            continue;
        }

        if in_struct {
            result.push(format!("# {}", line));
        } else if in_impl || (!in_struct && !line.trim().is_empty()) {
            result.push(line.to_string());
        }
    }

    Ok(result.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_examples() {
        let content = r#"
# Example

```rust
#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
struct User {
    name: String,
}
```

generates:

```rust
// OLD CONTENT
impl User {
    // old stuff
}
```
"#;

        let examples = find_expandable_examples(content).unwrap();
        assert_eq!(examples.len(), 1);
        assert!(examples[0].input_code.contains("struct User"));
    }
}
