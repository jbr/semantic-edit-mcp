#![allow(clippy::collapsible_if)]
#![deny(dead_code)]

mod editor;
mod education;
mod indentation;
mod languages;
mod searcher;
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

const INSTRUCTIONS: &str = r#"Use edit to make a change: when the edit validates, it is written to disk immediately and the response shows the resulting diff. Review that diff — if the edit landed somewhere other than intended, retarget_edit moves it to a corrected anchor in one step, and undo_edit reverts it (single-level). find_anchor checks where an anchor matches before editing.
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
