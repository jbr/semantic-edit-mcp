use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::languages::LanguageRegistry;
use crate::operations::{EditOperation, NodeSelector};
use crate::server::{Tool, ToolCallParams};
use crate::staging::{StagedOperation, StagingStore};
use anyhow::{Result, anyhow};
use lru::LruCache;
use serde_json::{Value, json};
use tokio::sync::Mutex;

pub struct ToolRegistry {
    language_registry: LanguageRegistry,
    tools: Vec<Tool>,
    cache: Arc<Mutex<lru::LruCache<String, String>>>,
}

#[derive(Debug)]
pub enum ExecutionResult {
    ResponseOnly(String),
    ChangeStaged(String, StagedOperation),
    Change {
        response: String,
        output: String,
        output_path: String,
    },
}

impl ExecutionResult {
    pub(crate) async fn write(self) -> Result<String> {
        match self {
            ExecutionResult::ResponseOnly(response) => Ok(response),
            Self::ChangeStaged(response, staged_operation) => {
                let _ = staged_operation;
                Ok(response)
            }
            ExecutionResult::Change {
                response,
                output,
                output_path,
            } => {
                tokio::fs::write(output_path, output).await?;
                Ok(response)
            }
        }
    }
}

impl ToolRegistry {
    pub fn new() -> Result<Self> {
        let tools = vec![
            serde_json::from_str(include_str!("../schemas/stage_operation.json"))?,
            serde_json::from_str(include_str!("../schemas/retarget_staged.json"))?,
            serde_json::from_str(include_str!("../schemas/commit_staged.json"))?,
            serde_json::from_str(include_str!("../schemas/open_file.json"))?,
        ];

        Ok(Self {
            tools,
            language_registry: LanguageRegistry::new()?,
            cache: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(50).unwrap()))),
        })
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        self.tools.clone()
    }

    pub async fn execute_tool(
        &self,
        tool_call: &ToolCallParams,
        staging_store: &StagingStore,
    ) -> Result<ExecutionResult> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        match tool_call.name.as_str() {
            "stage_operation" => self.stage_operation(args, staging_store).await,
            "retarget_staged" => self.handle_retarget_staged(args, staging_store).await,
            "commit_staged" => self.commit_staged(staging_store).await,
            "open_file" => self.open_file(args).await,
            tool_call => Err(anyhow!("Tool {tool_call} not recognized")),
        }
    }

    async fn stage_operation(
        &self,
        args: &Value,
        staging_store: &StagingStore,
    ) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?
            .to_string();

        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let target = NodeSelector::new_from_value(args)?;

        let operation = EditOperation { target, content };

        let language = self
            .language_registry
            .get_language_with_hint(&file_path, language_hint.as_deref())?;

        let ExecutionResult::ResponseOnly(message) = operation.apply(language, &file_path, true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };
        let staged_operation = StagedOperation {
            operation,
            file_path,
            language_name: language.name(),
        };
        staging_store.stage(staged_operation.clone());
        Ok(ExecutionResult::ChangeStaged(message, staged_operation))
    }

    pub async fn commit_staged(&self, staging_store: &StagingStore) -> Result<ExecutionResult> {
        staging_store
            .take_staged_operation()
            .ok_or_else(|| anyhow!("No operation is currently staged"))?
            .commit(&self.language_registry)
    }

    async fn handle_retarget_staged(
        &self,
        args: &Value,
        staging_store: &StagingStore,
    ) -> std::result::Result<ExecutionResult, anyhow::Error> {
        let selector = NodeSelector::new_from_value(args)?;
        let staged = staging_store
            .modify_staged_operation(|op| op.retarget(selector))
            .ok_or_else(|| anyhow!("no operation staged"))?;
        let language = self
            .language_registry
            .get_language(staged.language_name())
            .ok_or_else(|| anyhow!("language not recognized"))?;

        let ExecutionResult::ResponseOnly(message) =
            staged.operation.apply(language, &staged.file_path, true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };

        staging_store.stage(staged.clone());
        Ok(ExecutionResult::ChangeStaged(message, staged))
    }

    async fn open_file(&self, args: &Value) -> std::result::Result<ExecutionResult, anyhow::Error> {
        let cache = Arc::clone(&self.cache);
        let file_paths = args
            .get("file_paths")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("file_paths array is required"))?;

        if file_paths.is_empty() {
            return Err(anyhow!("file_paths array cannot be empty"));
        }

        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let diff = args
            .get("diff_since")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Validate diff_since usage
        if diff.is_some() && file_paths.len() > 1 {
            return Err(anyhow!(
                "diff_since is not supported when opening multiple files. \
                Please open files individually to use diff tracking, or omit diff_since to open all files fresh."
            ));
        }

        let mut response_parts = Vec::new();

        for file_path_value in file_paths {
            let file_path = file_path_value
                .as_str()
                .ok_or_else(|| anyhow!("Each file path must be a string"))?;

            let language = self
                .language_registry
                .get_language_with_hint(file_path, language_hint.as_deref())?;

            let contents = tokio::fs::read_to_string(file_path).await?;
            let canonicalized_file_path = tokio::fs::canonicalize(file_path).await?;

            if let Some(since) = &diff {
                if let Some(earlier) = cache
                    .lock()
                    .await
                    .get(&format!("{}#{since}", canonicalized_file_path.display()))
                {
                    let new_identifier = hash_content(&contents);
                    let mut diff_options = diffy::DiffOptions::new();
                    diff_options.set_original_filename(format!("{file_path}#{since}"));
                    diff_options.set_modified_filename(format!("{file_path}#{new_identifier}"));
                    let patch = diff_options.create_patch(earlier, &contents);
                    let formatter = diffy::PatchFormatter::new().missing_newline_message(false);
                    return Ok(ExecutionResult::ResponseOnly(format!(
                        "New identifier: {new_identifier}\n\nTo fetch changed content for this file,\
 use {{\"tool\": \"read_file\", \"file_path\": \"{file_path}\", \"diff_since\": \"{new_identifier}\"}}\n\n {}",
                        formatter.fmt_patch(&patch)
                    )));
                }
            }

            let separator = hash_content(&contents);

            let mut parser = language.tree_sitter_parser()?;
            let tree = parser
                .parse(&contents, None)
                .ok_or_else(|| anyhow!("could not parse {file_path} as {}", language.name()))?;

            let language_docs = language.docs();
            let tree_str = tree.root_node().to_string();

            let file_response = format!(
                "Separator/version identifier: {separator}\nPath: {file_path}\n\
{language_docs}\n\
To fetch changed content for this file, use {{\"tool\": \"read_file\", \"file_path\":\
 \"{file_path}\", \"diff_since\": \"{separator}\"}}\n\
\n
{separator} file contents {separator}\n\n{contents}\n\n\
{separator} syntax {separator}\n {tree_str}"
            );

            response_parts.push(file_response);

            cache.lock().await.put(
                format!("{}#{separator}", canonicalized_file_path.display()),
                contents,
            );
        }

        let response = if response_parts.len() == 1 {
            response_parts.into_iter().next().unwrap()
        } else {
            response_parts.join(&format!("\n\n{}\n\n", "=".repeat(80)))
        };

        Ok(ExecutionResult::ResponseOnly(response))
    }
}

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:010x}", hash % 0x10000000000) // 10 hex chars
}
