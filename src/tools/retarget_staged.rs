use crate::operations::{ExecutionResult, NodeSelector};
use crate::state::SemanticEditTools;
use crate::traits::WithExamples;
use crate::types::Example;
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Change the targeting of an already-staged operation without rewriting the content
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "retarget_staged")]
pub struct RetargetStaged {
    /// Text-anchored node selector using exact text and AST navigation.
    /// 🔄 RETARGETING STRATEGY: Use exploration mode (omit ancestor_node_type) to discover options, then retarget to try different scopes (e.g., field_declaration → struct_item → source_file for broader scope, or vice versa for narrower scope).
    pub selector: NodeSelector,
}

impl WithExamples for RetargetStaged {
    fn examples() -> Option<Vec<Example<Self>>> {
        Some(vec![
            Example {
                description: "After staging content to add a struct field, retarget from field_declaration to field_declaration_list for better insertion point",
                item: Self {
                    selector: NodeSelector {
                        anchor_text: "pub created_at:".into(),
                        ancestor_node_type: Some("field_declaration_list".into()),
                        position: None,
                    },
                },
            },
            Example {
                description: "Move JSON insertion from inside an object to after the entire object pair",
                item: Self {
                    selector: NodeSelector {
                        anchor_text: "\"cache\"".into(),
                        ancestor_node_type: Some("pair".into()),
                        position: None,
                    },
                },
            },
            Example {
                description: "Adjust function insertion from declaration_list to function_item scope",
                item: Self {
                    selector: NodeSelector {
                        anchor_text: "pub fn validate_email".into(),
                        ancestor_node_type: Some("function_item".into()),
                        position: None,
                    },
                },
            },
            Example {
                description: "Use exploration mode first to see all targeting options before retargeting",
                item: Self {
                    selector: NodeSelector {
                        anchor_text: "impl User".into(),
                        ancestor_node_type: None,
                        position: None,
                    },
                },
            },
        ])
    }
}

impl RetargetStaged {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { selector } = self;

        let staged = state
            .modify_staged_operation(None, |op| op.retarget(selector))?
            .ok_or_else(|| anyhow!("no operation staged"))?;

        let language = state
            .language_registry()
            .get_language(staged.language_name)
            .ok_or_else(|| anyhow!("language not recognized"))?;

        let ExecutionResult::ResponseOnly(message) =
            staged
                .operation()
                .apply(language, staged.file_path(), true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };

        state.stage_operation(None, staged)?;
        Ok(message)
    }
}
