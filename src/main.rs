#![allow(clippy::collapsible_if)]
#![deny(dead_code)]

mod editor;
mod indentation;
mod languages;
mod selector;
mod state;
mod tools;
mod validation;

#[cfg(test)]
mod tests;

use mcplease::server_info;
use state::SemanticEditTools;
use std::env;
use tools::Tools;

const INSTRUCTIONS: &str = r#"Use preview_edit to preview changes, retarget_edit to adjust targeting, and persist_edit to apply.
The purpose of the preview/retarget/persist pattern is so you can review a diff and adjust placement prior to persisting your change to disk.
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
