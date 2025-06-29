use crate::languages::LanguageName;
use crate::state::SemanticEditTools;
use crate::traits::WithExamples;
use crate::types::Example;
use anyhow::{anyhow, Result};
use diffy::{DiffOptions, PatchFormatter};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Open files for semantic editing
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "open_files")]
pub struct OpenFiles {
    /// Array of file paths to open. Can be a single file or multiple files.
    /// Each file path may be either absolute or — if session_id is present — relative to the session.
    file_paths: Vec<String>,

    /// Optional language hint. If provided, all files will be parsed as this language type. If not provided, language will be detected from file extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<LanguageName>,

    /// Unique identifier returned when viewing a file. Provide this to see changes since a known version.
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_since: Option<String>,

    /// Optional session identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

impl WithExamples for OpenFiles {
    fn examples() -> Vec<Example<Self>> {
        Some(vec![
            Example {
                description: "Open a single rust file",
                item: Self {
                    file_paths: vec!["/absolute/path/to/src/lib.rs".into()],
                    language: None,
                    diff_since: None,
                    session_id: None,
                },
            },
            Example {
                description: "Open multiple files relative to a session",
                item: Self {
                    file_paths: vec![
                        "src/main.rs".into(),
                        "src/lib.rs".into(),
                        "tests/mod.rs".into(),
                    ],
                    language: None,
                    diff_since: None,
                    session_id: Some("app-name/feature-name".into()),
                },
            },
            Example {
                description: "Open multiple files with language override",
                item: Self {
                    file_paths: vec![
                        "/absolute/path/to/config.local".into(),
                        "/absolute/path/to/config.prod".into(),
                    ],
                    language: Some(LanguageName::Json),
                    diff_since: None,
                    session_id: None,
                },
            },
        ])
    }
}

impl OpenFiles {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self {
            file_paths,
            language,
            diff_since,
            session_id,
        } = self;

        if file_paths.is_empty() {
            return Err(anyhow!("file_paths array cannot be empty"));
        }

        // Validate diff usage
        if diff_since.is_some() && file_paths.len() > 1 {
            return Err(anyhow!(
                "diff_since is not supported when opening multiple files. \
                Please open files individually to use diff tracking, or omit diff_since to open all files fresh."
            ));
        }

        let mut response_parts = Vec::new();
        let mut hasher = DefaultHasher::new();

        let file_paths = file_paths
            .into_iter()
            .map(|path_str| state.resolve_path(&path_str, session_id.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;

        let mut contents = vec![];
        for file_path in &file_paths {
            // Check for diff request first
            if let Some(since) = &diff_since {
                let current_content = std::fs::read_to_string(file_path)?;

                let cache_key = format!("{}#{}", file_path.display(), since);
                if let Some(earlier_content) = state.file_cache().lock().unwrap().get(&cache_key) {
                    return Ok(handle_diff_request(
                        file_path,
                        &current_content,
                        earlier_content,
                        since,
                    ));
                }
            }

            let content = std::fs::read_to_string(file_path)?;
            content.hash(&mut hasher);
            contents.push((content, file_path.clone()));
        }

        let hash = hasher.finish();
        let separator = format!("{:010x}", hash % 0x10000000000); // 10 hex chars

        for (content, file_path) in contents {
            let language = state
                .language_registry()
                .get_language_with_hint(&file_path, language);

            let file_response =
                generate_file_response(&file_path, &content, &separator, language.ok())?;
            response_parts.push(file_response);

            // Cache the content for future diff requests
            let canonicalized_file_path = std::fs::canonicalize(&file_path)?;
            let cache_key = format!("{}#{}", canonicalized_file_path.display(), separator);
            state.file_cache().lock().unwrap().put(cache_key, content);
        }

        let response = format!(
            "Separator/version identifier: {}\n\n{}",
            separator,
            response_parts.join("\n\n\n")
        );
        Ok(response)
    }
}

fn handle_diff_request(file_path: &Path, current: &str, earlier: &str, since: &str) -> String {
    let new_identifier = hash_content(current);
    let mut diff_options = DiffOptions::new();
    diff_options.set_original_filename(format!(
        "{file_path}#{since}",
        file_path = file_path.display()
    ));
    diff_options.set_modified_filename(format!(
        "{file_path}#{new_identifier}",
        file_path = file_path.display()
    ));
    let patch = diff_options.create_patch(earlier, current);
    let formatter = PatchFormatter::new().missing_newline_message(false);

    format!(
        "New identifier: {new_identifier}\n\nTo fetch changed content for this file, use {{\"tool\": \"open_files\", \"file_path\": \"{file_path}\", \"diff_since\": \"{new_identifier}\"}}\n\n{}",
        formatter.fmt_patch(&patch),
        file_path = file_path.display()
    )
}

fn generate_file_response(
    file_path: &Path,
    contents: &str,
    separator: &str,
    language: Option<&crate::languages::LanguageCommon>,
) -> Result<String> {
    let eq = "=".repeat(10);
    let (syntax_section, docs_section) = if let Some(language) = language {
        let mut parser = language.tree_sitter_parser()?;
        let tree = parser.parse(contents, None).ok_or_else(|| {
            anyhow!(
                "could not parse {} as {}",
                file_path.display(),
                language.name()
            )
        })?;

        let language_docs = language.docs();
        let tree_str = tree.root_node().to_sexp();
        (
            format!(
                "{eq}{separator} {file_path} SYNTAX {separator}{eq}\n{tree_str}\n",
                file_path = file_path.display()
            ),
            language_docs,
        )
    } else {
        ("".into(), "This file format is not recognized. You will need to specify a language in order to operate on it".into())
    };

    Ok(format!(
        "{eq}{separator} {file_path} META {separator}{eq}\n\
         {docs_section}\n\
         To fetch changed content for this file, use {{\"tool\": \"open_files\", \"file_path\":\
         \"{file_path}\", \"diff_since\": \"{separator}\"}}\n\
         {eq}{separator} {file_path} CONTENTS {separator}{eq}\n{contents}\n\
         {syntax_section}\
         {eq}{separator} {file_path} END {separator}{eq}",
        eq = "=".repeat(10),
        file_path = file_path.display()
    ))
}

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:010x}", hash % 0x10000000000) // 10 hex chars
}
