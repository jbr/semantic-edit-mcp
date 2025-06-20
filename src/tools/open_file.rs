use anyhow::{anyhow, Result};
use diffy::{DiffOptions, PatchFormatter};
use lru::LruCache;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::languages::{LanguageCommon, LanguageRegistry};
use crate::tools::ExecutionResult;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct OpenFile<'a> {
    file_paths: Vec<String>,
    language_hint: Option<String>,
    diff_since: Option<String>,
    cache: &'a Mutex<LruCache<String, String>>,
    language_registry: &'a LanguageRegistry,
}

impl<'a> OpenFile<'a> {
    pub fn new(
        args: &Value,
        cache: &'a Mutex<LruCache<String, String>>,
        language_registry: &'a LanguageRegistry,
    ) -> Result<Self> {
        let file_paths = args
            .get("file_paths")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("file_paths array is required"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| anyhow!("Each file path must be a string"))
                    .map(|s| s.to_string())
            })
            .collect::<Result<Vec<_>>>()?;

        if file_paths.is_empty() {
            return Err(anyhow!("file_paths array cannot be empty"));
        }

        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let diff_since = args
            .get("diff_since")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(Self {
            file_paths,
            language_hint,
            diff_since,
            cache,
            language_registry,
        })
    }

    pub async fn execute(self) -> Result<ExecutionResult> {
        self.validate_diff_usage()?;

        let mut response_parts = Vec::new();
        let mut hasher = DefaultHasher::new();

        let mut contents = vec![];
        for file_path in &self.file_paths {
            if let Some(diff_response) = self.process_file(file_path).await? {
                // Early return for diff response
                return Ok(ExecutionResult::ResponseOnly(diff_response));
            }
            let content = tokio::fs::read_to_string(file_path).await?;
            content.hash(&mut hasher);
            contents.push((content, file_path));
        }

        let hash = hasher.finish();
        let separator = format!("{:010x}", hash % 0x10000000000); // 10 hex chars

        for (content, file_path) in contents {
            let language = self
                .language_registry
                .get_language_with_hint(file_path, self.language_hint.as_deref())?;

            let file_response =
                self.generate_file_response(file_path, &content, &separator, language)?;
            response_parts.push(file_response);

            // Cache the content for future diff requests
            let canonicalized_file_path = tokio::fs::canonicalize(file_path).await?;
            self.cache.lock().await.put(
                format!("{}#{separator}", canonicalized_file_path.display()),
                content,
            );
        }

        let response = self.format_response(response_parts, separator);
        Ok(ExecutionResult::ResponseOnly(response))
    }

    fn validate_diff_usage(&self) -> Result<()> {
        if self.diff_since.is_some() && self.file_paths.len() > 1 {
            return Err(anyhow!(
                "diff_since is not supported when opening multiple files. \
                Please open files individually to use diff tracking, or omit diff_since to open all files fresh."
            ));
        }
        Ok(())
    }

    async fn process_file(&self, file_path: &str) -> Result<Option<String>> {
        if let Some(since) = &self.diff_since {
            let contents = tokio::fs::read_to_string(file_path).await?;
            let canonicalized_file_path = tokio::fs::canonicalize(file_path).await?;

            if let Some(earlier) = self
                .cache
                .lock()
                .await
                .get(&format!("{}#{since}", canonicalized_file_path.display()))
            {
                return Ok(Some(
                    self.handle_diff_request(file_path, &contents, earlier)
                        .await?,
                ));
            }
        }
        Ok(None)
    }

    async fn handle_diff_request(
        &self,
        file_path: &str,
        contents: &str,
        earlier: &str,
    ) -> Result<String> {
        let new_identifier = hash_content(contents);
        let mut diff_options = DiffOptions::new();
        diff_options
            .set_original_filename(format!("{file_path}#{}", self.diff_since.as_ref().unwrap()));
        diff_options.set_modified_filename(format!("{file_path}#{new_identifier}"));
        let patch = diff_options.create_patch(earlier, contents);
        let formatter = PatchFormatter::new().missing_newline_message(false);

        Ok(format!(
            "New identifier: {new_identifier}\n\nTo fetch changed content for this file,\
 use {{\"tool\": \"open_file\", \"file_path\": \"{file_path}\", \"diff_since\": \"{new_identifier}\"}}\n\n {}",
            formatter.fmt_patch(&patch)
        ))
    }

    fn generate_file_response(
        &self,
        file_path: &str,
        contents: &str,
        separator: &str,
        language: &LanguageCommon,
    ) -> Result<String> {
        let mut parser = language.tree_sitter_parser()?;
        let tree = parser
            .parse(contents, None)
            .ok_or_else(|| anyhow!("could not parse {file_path} as {}", language.name()))?;

        let language_docs = language.docs();
        let tree_str = tree.root_node().to_sexp();

        Ok(format!(
            "{eq}{separator} {file_path} META {separator}{eq}\n\
             {language_docs}\n\n\
             To fetch changed content for this file, use {{\"tool\": \"open_file\", \"file_path\":\
             \"{file_path}\", \"diff_since\": \"{separator}\"}}\n\
             \n\
             {eq}{separator} {file_path} CONTENTS {separator}{eq}\n{contents}\n\
             {eq}{separator} {file_path} SYNTAX {separator}{eq}\n{tree_str}\n\
             {eq}{separator} {file_path} END {separator}{eq}",
            eq = "=".repeat(10),
        ))
    }

    fn format_response(&self, response_parts: Vec<String>, separator: String) -> String {
        format!(
            "Separator/version identifier: {separator}\n\n{}",
            response_parts.join("\n\n\n")
        )
    }
}

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:010x}", hash % 0x10000000000) // 10 hex chars
}
