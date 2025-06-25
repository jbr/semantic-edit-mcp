use std::path::PathBuf;

use crate::{state::SemanticEditTools, traits::WithExamples, types::Example};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Set the working context path for a session
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(rename = "set_context")]
pub struct SetContext {
    /// Directory path to set as context.
    /// Subsequent to calling this, any relative paths will be relative to this directory
    path: String,
    // temporarily commented out
    // /// Session identifier can be absolutely any string, as long as it's unlikely to collide with another session, (ie not "claude")
    // /// You will need to provide this to subsequent tool calls, so short and memorable but unique is probably best. Be creative!
    // ///
    // /// This is currently necessary in order to isolate state between conversations because MCP does
    // /// not currently provide any session identifier.
    // /// Hopefully eventually this will be handled by the protocol.",
    // session_id: String,
}

impl WithExamples for SetContext {
    fn examples() -> Option<Vec<Example<Self>>> {
        Some(vec![Example {
            description: "setting context to a development project",
            item: Self {
                path: "/usr/local/projects/cobol".into(),
                //                session_id: "GraceHopper1906".into(),
            },
        }])
    }
}

impl SetContext {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { path } = self;
        let path = PathBuf::from(&*shellexpand::tilde(&path));
        let response = format!(
            "Set context to {path} for session.\n",
            path = path.display()
        );
        state.set_context(None, path)?;
        Ok(response)
    }
}
