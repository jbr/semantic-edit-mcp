#![allow(clippy::collapsible_if)]

mod editor;
mod languages;
mod selector;
mod state;
mod tools;
mod validation;

use mcplease::server_info;
use state::SemanticEditTools;
use std::env;
use tools::Tools;

const INSTRUCTIONS: &str = r#"Use stage_operation to preview changes, retarget_staged to adjust targeting, and commit_staged to apply.
The purpose of the stage/retarget/commit pattern is so you can review a diff and adjust placement prior to persisting your change to disk.
There is only one operation staged at a time, and there is no need to "unstage" the operation; just replace it with another operation.
"#;

fn main() {
    let mut state = SemanticEditTools::new(
        env::var("MCP_SESSION_STORAGE_PATH")
            .ok()
            .as_deref()
            .or(Some("~/.ai-tools/sessions/semantic-edit.json")),
    )
    .unwrap();

    mcplease::run::<Tools, _>(&mut state, server_info!(), Some(INSTRUCTIONS)).unwrap()
}
