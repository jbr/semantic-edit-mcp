#![allow(clippy::collapsible_if)]

mod languages;
mod operations;
mod session;
mod state;
mod tools;
mod traits;
mod types;
mod validation;

use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use anyhow::Result;
use env_logger::{Builder, Target};
use state::SemanticEditTools;
pub use types::{
    ContentResponse, InitializeResponse, McpMessage, McpResponse, RequestType, ToolsListResponse,
};

const INSTRUCTIONS: &str = "Semantic code editing with tree-sitter. Use stage_operation to preview changes, retarget_staged to adjust targeting, and commit_staged to apply.";

fn main() -> Result<()> {
    let mut state =
        SemanticEditTools::new(std::env::var("MCP_SESSION_STORAGE_PATH").ok().as_deref())?;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    if let Ok(log_location) = std::env::var("LOG_LOCATION") {
        let path = PathBuf::from(&*shellexpand::tilde(&log_location));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        Builder::from_default_env()
            .target(Target::Pipe(Box::new(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap(),
            )))
            .init();
    }

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                log::trace!("<- {line}");
                if let Ok(McpMessage::Request(request)) = serde_json::from_str(&line) {
                    let response = request
                        .call
                        .execute(request.id, &mut state, Some(INSTRUCTIONS));
                    let response_str = serde_json::to_string(&response)?;
                    log::trace!("-> {response_str}");
                    stdout.write_all(response_str.as_bytes())?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
            }
            Err(e) => {
                log::error!("Error reading line: {e}");
                break;
            }
        }
    }

    Ok(())
}
