use crate::{editor::Editor, selector::Selector, state::SemanticEditTools};

use anyhow::{anyhow, Result};
use mcplease::{
    traits::{Tool, WithExamples},
    types::Example,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Change the targeting of an already-staged operation without rewriting the content
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "retarget_edit")]
#[group(skip)]
pub struct RetargetEdit {
    #[serde(flatten)]
    #[clap(flatten)]
    pub selector: Selector,
}

impl WithExamples for RetargetEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![
            // Example {
            //     description: "After staging content to add a struct field, retarget from field_declaration to field_declaration_list for better insertion point",
            //     item: Self {
            //         selector: NodeSelector {
            //             anchor_text: "pub created_at:".into(),
            //             ancestor_node_type: Some("field_declaration_list".into()),
            //             position: None,
            //         },
            //     },
            // },
            // Example {
            //     description: "Move JSON insertion from inside an object to after the entire object pair",
            //     item: Self {
            //         selector: NodeSelector {
            //             anchor_text: "\"cache\"".into(),
            //             ancestor_node_type: Some("pair".into()),
            //             position: None,
            //         },
            //     },
            // },
            // Example {
            //     description: "Adjust function insertion from declaration_list to function_item scope",
            //     item: Self {
            //         selector: NodeSelector {
            //             anchor_text: "pub fn validate_email".into(),
            //             ancestor_node_type: Some("function_item".into()),
            //             position: None,
            //         },
            //     },
            // },
            // Example {
            //     description: "Use exploration mode first to see all targeting options before retargeting",
            //     item: Self {
            //         selector: NodeSelector {
            //             anchor_text: "impl User".into(),
            //             ancestor_node_type: None,
            //             position: None,
            //         },
            //     },
            // },
        ]
    }
}

impl Tool<SemanticEditTools> for RetargetEdit {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { selector } = self;

        let staged_operation = state
            .modify_staged_operation(None, |op| op.retarget(selector))?
            .ok_or_else(|| anyhow!("no operation staged"))?;

        let editor =
            Editor::from_staged_operation(staged_operation.clone(), state.language_registry())?;
        let (message, staged_operation) = editor.preview()?;
        if staged_operation.is_some() {
            // leave failed operations in place
            state.preview_edit(None, staged_operation)?;
        }
        Ok(message)
    }
}
