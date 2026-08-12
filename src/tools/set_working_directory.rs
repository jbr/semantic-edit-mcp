use crate::state::SemanticEditTools;
use anyhow::Result;
use mcplease::{
    traits::{Tool, ToolMeta},
    types::{Example, RequestContext, ToolAnnotations},
};
use serde::{Deserialize, Serialize};

/// Set the working context path for a session
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema, clap::Args)]
#[serde(rename = "set_working_directory")]
#[group(skip)]
pub struct SetWorkingDirectory {
    /// New working directory. All relative paths will be relative to this path
    path: String,
}

impl ToolMeta for SetWorkingDirectory {
    fn title() -> Option<&'static str> {
        Some("Set working directory")
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            // Writes session state, not files.
            read_only_hint: Some(false),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
            ..Default::default()
        })
    }

    fn examples() -> Vec<Example<Self>> {
        vec![Example {
            description: "setting context to a development project",
            item: Self {
                path: "/usr/local/projects/cobol".into(),
            },
        }]
    }
}

impl Tool<SemanticEditTools> for SetWorkingDirectory {
    type Output = String;

    fn execute(self, state: &mut SemanticEditTools, _context: &RequestContext) -> Result<String> {
        let new_context_path = state.resolve_path(&self.path, None)?;
        let response = format!("Set context to {}", new_context_path.display());
        state.set_working_directory(new_context_path, None)?;
        Ok(response)
    }
}
