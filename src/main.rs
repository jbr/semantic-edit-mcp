mod handlers;
mod languages;
mod operations;
mod parser;
mod server;
mod server_impl;
mod staging;
mod tools;
mod validation;

use anyhow::Result;
use clap::{Parser, Subcommand};
use server_impl::SemanticEditServer;

#[derive(Parser)]
#[command(name = "semantic-edit-mcp")]
#[command(about = "A Model Context Protocol server for semantic code editing")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server
    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve) | None => {
            let server = SemanticEditServer::new()?;
            server.run().await?;
        }
    }

    Ok(())
}
