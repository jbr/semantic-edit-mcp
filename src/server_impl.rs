use crate::server::{Tool, McpMessage, McpRequest, McpResponse, ToolCallParams};
use crate::tools::ToolRegistry;
use crate::handlers::RequestHandler;
use crate::parsers::TreeSitterParser;
use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct SemanticEditServer {
    tools: Vec<Tool>,
    _parser: TreeSitterParser,
    tool_registry: ToolRegistry,
    request_handler: RequestHandler,
}

impl SemanticEditServer {
    pub fn new() -> Result<Self> {
        let _parser = TreeSitterParser::new()?;
        let tool_registry = ToolRegistry::new()?;
        let tools = tool_registry.get_tools();
        let request_handler = RequestHandler::new();

        Ok(Self {
            tools,
            _parser,
            tool_registry,
            request_handler,
        })
    }

    pub async fn run(self) -> Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let message: McpMessage = match serde_json::from_str(trimmed) {
                        Ok(msg) => msg,
                        Err(e) => {
                            eprintln!("Failed to parse message: {e}");
                            continue;
                        }
                    };

                    if let Some(response) = self.handle_message(message).await {
                        let response_json = serde_json::to_string(&response)?;
                        stdout.write_all(response_json.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from stdin: {e}");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&self, message: McpMessage) -> Option<McpResponse> {
        match message {
            McpMessage::Request(request) => Some(self.handle_request(request).await),
            McpMessage::Notification(notification) => {
                self.handle_notification(notification).await;
                None
            }
        }
    }

    async fn handle_request(&self, request: McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => self.request_handler.handle_initialize(request.id),
            "tools/list" => self.request_handler.handle_tools_list(request.id, &self.tools),
            "tools/call" => self.handle_tool_call(request.id, request.params).await,
            _ => McpResponse::error(
                request.id,
                -32601,
                "Method not found".to_string(),
            ),
        }
    }

    async fn handle_notification(&self, _notification: crate::server::McpNotification) {
        // Handle notifications if needed
    }

    async fn handle_tool_call(&self, id: Value, params: Option<Value>) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return McpResponse::error(id, -32602, "Invalid params".to_string());
            }
        };

        let tool_call: ToolCallParams = match serde_json::from_value(params) {
            Ok(tc) => tc,
            Err(e) => {
                return McpResponse::error(
                    id,
                    -32602,
                    format!("Invalid tool call params: {e}"),
                );
            }
        };

        let result = match self.tool_registry.execute_tool(&tool_call).await {
            Ok(output) => output,
            Err(e) => {
                return McpResponse::error(
                    id,
                    -32603,
                    format!("Tool execution failed: {e}"),
                );
            }
        };

        McpResponse::success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": result
                }]
            }),
        )
    }
}
