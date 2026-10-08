#![allow(clippy::collapsible_if)]
#![deny(dead_code)]

mod editor;
mod education;
mod indentation;
mod item;
mod languages;
mod searcher;
mod selector;
mod state;
mod tools;
mod validation;

#[cfg(test)]
mod tests;

use anyhow::Result;
use async_compat::Compat;
use clap::Parser;
use mcplease::{Cli, ServerConfig, server_info, traits::Dispatch, types::RequestContext};
use state::SemanticEditTools;
use std::path::PathBuf;
use tools::Tools;

const INSTRUCTIONS: &str = r#"Use edit to make a change: when the edit validates, it is written to disk immediately and the response shows the resulting diff. Review that diff — if the edit landed somewhere other than intended, retarget_edit moves it to a corrected anchor in one step, and undo_edit reverts it; both name the file the edit was made to. find_anchor checks where an anchor matches before editing. Paths are absolute, or relative to the directory this server was started in.
"#;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// The directory relative paths resolve against [default: the current directory]
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Cli<Tools>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // stderr is the one place a stdio server may log.
    env_logger::init();
    let Args { root, command } = Args::parse();
    let root = match root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };
    let state = SemanticEditTools::new(root)?;

    match command {
        Cli::Tool(tool) => {
            let output = tool.call_to_text(state, RequestContext::default()).await?;
            println!("{output}");
        }

        Cli::Serve => {
            let config = ServerConfig::new(server_info!()).with_instructions(INSTRUCTIONS);
            let stdin = Compat::new(tokio::io::stdin());
            let stdout = Compat::new(tokio::io::stdout());
            let mut calls = mcplease::calls::<Tools, _>(stdin, stdout, state, &config);
            while let Some(call) = calls.next().await {
                tokio::spawn(call?);
            }
        }
    }

    Ok(())
}
