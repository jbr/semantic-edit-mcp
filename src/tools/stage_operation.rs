use crate::languages::LanguageName;
use crate::operations::selector::Position;
use crate::operations::{EditOperation, ExecutionResult, NodeSelector};
use crate::state::{SemanticEditTools, StagedOperation};
use crate::traits::WithExamples;
use crate::types::Example;
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stage an operation for execution and preview what it would do
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "stage_operation")]
pub struct StageOperation {
    /// Path to the source file.
    /// If a session has been configured, this can be a relative path to the session root.
    pub file_path: String,

    /// Text-anchored node selector using exact text and AST navigation. 🎯 BEST PRACTICE: Start by omitting ancestor_node_type to explore all available targeting options, then use the suggested selector from the rich exploration results.
    pub selector: NodeSelector,

    /// Content to insert. For "around" position, use `{{content}}` as a placeholder for the original code. 🎯 "around" use cases: Use around when you need atomic transformations that can't be done with multiple inserts (due to syntax requirements). Examples: error handling (Result<{{content}}>), conditionals (if cond { {{content}} }), async blocks (async { {{content}} }), or any structure requiring matching braces/brackets.
    ///
    /// Omit this content when using "position": "replace" in order to perform a delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Optional language hint. If not provided, language will be detected from file extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<LanguageName>,
    // this is commented out temporarily as an experiment in usability
    // /// Optional session identifier
    // pub session_id: Option<String>,
}

impl WithExamples for StageOperation {
    fn examples() -> Option<Vec<Example<Self>>> {
        Some(vec![
            Example {
                description: "Step 1: Explore available node types (discovery mode)",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: NodeSelector {
                        anchor_text: "fn hello".into(),
                        ancestor_node_type: None,
                        position: None,
                    },
                    content: Some("// Discovery first - see what options are available".into()),
                    language: None,
                },
            },
            Example {
                description: "Step 2: Use discovered node type for precise targeting",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: NodeSelector {
                        anchor_text: "fn hello".into(),
                        ancestor_node_type: Some("function_item".into()),
                        position: Some(Position::Replace),
                    },
                    content: Some("fn hello() { println!(\"Hello, world!\"); }".into()),
                    language: None,
                },
            },
            Example {
                description: "Remove a function",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: NodeSelector {
                        anchor_text: "fn unused_function".into(),
                        ancestor_node_type: Some("function_item".into()),
                        position: Some(Position::Replace),
                    },
                    content: None,
                    language: None,
                },
            },
            Example {
                description: "Insert after a use statement",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: NodeSelector {
                        anchor_text: "use std::collections::HashMap;".into(),
                        ancestor_node_type: Some("use_declaration".into()),
                        position: Some(Position::After),
                    },
                    content: Some("use std::fs;".into()),
                    language: None,
                },
            },
            Example {
                description: "Add error handling around function call",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: NodeSelector {
                        anchor_text: "parse_config()".into(),
                        ancestor_node_type: Some("call_expression".into()),
                        position: Some(Position::Around),
                    },
                    content: Some("match {{content}} {\n    Ok(config) => config,\n    Err(e) => return Err(format!(\"Config error: {}\", e))\n}".into()),
                    language: None,
                },
            },
        ])
    }
}

impl StageOperation {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self {
            file_path,
            selector,
            content,
            language,
        } = self;

        let file_path = state.resolve_path(&file_path, None)?;

        let operation = EditOperation {
            target: selector,
            content,
        };

        let language = state
            .language_registry()
            .get_language_with_hint(&file_path, language)?;

        let ExecutionResult::ResponseOnly(message) = operation.apply(language, &file_path, true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };

        let staged_operation = StagedOperation {
            operation,
            file_path: file_path.clone(),
            language_name: language.name(),
        };

        state.stage_operation(None, staged_operation)?;

        Ok(message)
    }
}
