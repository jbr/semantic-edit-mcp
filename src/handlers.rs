use crate::server::{Tool, McpResponse};
use anyhow::Result;
use serde_json::{Value, json};

pub struct RequestHandler;

impl RequestHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_initialize(&self, id: Value) -> McpResponse {
        McpResponse::success(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "semantic-edit-mcp",
                    "version": "0.1.0"
                }
            }),
        )
    }

    pub fn handle_tools_list(&self, id: Value, tools: &[Tool]) -> McpResponse {
        McpResponse::success(
            id,
            json!({
                "tools": tools
            }),
        )
    }
}
