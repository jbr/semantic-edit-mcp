use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::BufReader;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

mod editors;
mod operations;
mod parsers;
mod validation;

use editors::rust::RustEditor;
use operations::{EditOperation, NodeSelector};
use parsers::{TreeSitterParser, detect_language_from_path};
use validation::SyntaxValidator;

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum McpMessage {
    Request(McpRequest),
    Notification(McpNotification),
}

#[derive(Debug, Serialize, Deserialize)]
struct McpRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpNotification {
    jsonrpc: String,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McpError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct ToolCallParams {
    name: String,
    arguments: Option<Value>,
}

struct SemanticEditServer {
    tools: Vec<Tool>,
    _parser: TreeSitterParser,
}

impl SemanticEditServer {
    fn new() -> Result<Self> {
        let _parser = TreeSitterParser::new()?;

        let tools = vec![
            Tool {
                name: "replace_node".to_string(),
                description: "Replace an entire AST node with new content".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "selector": {
                            "type": "object",
                            "description": "Node selector (by name, type, query, or position). RECOMMENDED: Use semantic selectors (name/type/query) for reliable targeting. Position-based selection may select unexpected small nodes.",
                            "properties": {
                                "type": {"type": "string", "description": "Node type (e.g., 'function_item') - RECOMMENDED for reliable selection"},
                                "name": {"type": "string", "description": "Name of the node - RECOMMENDED for reliable selection"},
                                "query": {"type": "string", "description": "Tree-sitter query - RECOMMENDED for precise selection"},
                                "line": {"type": "number", "description": "⚠️  Line number (1-indexed) - May select small tokens, use with caution"},
                                "column": {"type": "number", "description": "⚠️  Column number (1-indexed) - May select small tokens, use with caution"},
                                "scope": {"type": "string", "description": "Optional scope hint for position selection: 'token' (default), 'expression', 'statement', 'item'"}
                            }
                        },
                        "new_content": {
                            "type": "string",
                            "description": "New content to replace the node"
                        }
                    },
                    "required": ["file_path", "selector", "new_content"]
                }),
            },
            Tool {
                name: "insert_before_node".to_string(),
                description: "Insert content before a specified AST node".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "selector": {
                            "type": "object",
                            "description": "Node selector for the target node",
                            "properties": {
                                "type": {"type": "string", "description": "Node type - RECOMMENDED"},
                                "name": {"type": "string", "description": "Node name - RECOMMENDED"},
                                "query": {"type": "string", "description": "Tree-sitter query - RECOMMENDED"},
                                "line": {"type": "number", "description": "⚠️  Line (1-indexed) - use with caution"},
                                "column": {"type": "number", "description": "⚠️  Column (1-indexed) - use with caution"},
                                "scope": {"type": "string", "description": "Scope hint: 'token', 'expression', 'statement', 'item'"}
                            }
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to insert"
                        }
                    },
                    "required": ["file_path", "selector", "content"]
                }),
            },
            Tool {
                name: "insert_after_node".to_string(),
                description: "Insert content after a specified AST node".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "selector": {
                            "type": "object",
                            "description": "Node selector for the target node",
                            "properties": {
                                "type": {"type": "string", "description": "Node type - RECOMMENDED"},
                                "name": {"type": "string", "description": "Node name - RECOMMENDED"},
                                "query": {"type": "string", "description": "Tree-sitter query - RECOMMENDED"},
                                "line": {"type": "number", "description": "⚠️  Line (1-indexed) - use with caution"},
                                "column": {"type": "number", "description": "⚠️  Column (1-indexed) - use with caution"},
                                "scope": {"type": "string", "description": "Scope hint: 'token', 'expression', 'statement', 'item'"}
                            }
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to insert"
                        }
                    },
                    "required": ["file_path", "selector", "content"]
                }),
            },
            Tool {
                name: "wrap_node".to_string(),
                description: "Wrap an AST node with new syntax".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "selector": {
                            "type": "object",
                            "description": "Node selector for the target node",
                            "properties": {
                                "type": {"type": "string", "description": "Node type - RECOMMENDED"},
                                "name": {"type": "string", "description": "Node name - RECOMMENDED"},
                                "query": {"type": "string", "description": "Tree-sitter query - RECOMMENDED"},
                                "line": {"type": "number", "description": "⚠️  Line (1-indexed) - use with caution"},
                                "column": {"type": "number", "description": "⚠️  Column (1-indexed) - use with caution"},
                                "scope": {"type": "string", "description": "Scope hint: 'token', 'expression', 'statement', 'item'"}
                            }
                        },
                        "wrapper_template": {
                            "type": "string",
                            "description": "Template for wrapping (use {{content}} as placeholder)"
                        }
                    },
                    "required": ["file_path", "selector", "wrapper_template"]
                }),
            },
            Tool {
                name: "validate_syntax".to_string(),
                description: "Validate that a file or code snippet has correct syntax".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file (optional if content provided)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Code content to validate (optional if file_path provided)"
                        },
                        "language": {
                            "type": "string",
                            "description": "Programming language (rust, typescript, etc.)",
                            "default": "rust"
                        }
                    }
                }),
            },
            Tool {
                name: "get_node_info".to_string(),
                description: "Get information about a node at a specific location".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the source file"
                        },
                        "selector": {
                            "type": "object",
                            "description": "Node selector",
                            "properties": {
                                "type": {"type": "string", "description": "Node type - RECOMMENDED"},
                                "name": {"type": "string", "description": "Node name - RECOMMENDED"},
                                "query": {"type": "string", "description": "Tree-sitter query - RECOMMENDED"},
                                "line": {"type": "number", "description": "⚠️  Line (1-indexed) - use with caution"},
                                "column": {"type": "number", "description": "⚠️  Column (1-indexed) - use with caution"},
                                "scope": {"type": "string", "description": "Scope hint: 'token', 'expression', 'statement', 'item'"}
                            }
                        }
                    },
                    "required": ["file_path", "selector"]
                }),
            },
        ];

        Ok(Self { tools, _parser })
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
            "initialize" => self.handle_initialize(request.id),
            "tools/list" => self.handle_tools_list(request.id),
            "tools/call" => self.handle_tool_call(request.id, request.params).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
            },
        }
    }

    async fn handle_notification(&self, _notification: McpNotification) {
        // Handle notifications if needed
    }

    fn handle_initialize(&self, id: Value) -> McpResponse {
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "semantic-edit-mcp",
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, id: Value) -> McpResponse {
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "tools": self.tools
            })),
            error: None,
        }
    }

    async fn handle_tool_call(&self, id: Value, params: Option<Value>) -> McpResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(McpError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                        data: None,
                    }),
                };
            }
        };

        let tool_call: ToolCallParams = match serde_json::from_value(params) {
            Ok(tc) => tc,
            Err(e) => {
                return McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(McpError {
                        code: -32602,
                        message: format!("Invalid tool call params: {e}"),
                        data: None,
                    }),
                };
            }
        };

        let result = match self.execute_tool(&tool_call).await {
            Ok(output) => output,
            Err(e) => {
                return McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(McpError {
                        code: -32603,
                        message: format!("Tool execution failed: {e}"),
                        data: None,
                    }),
                };
            }
        };

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "content": [{
                    "type": "text",
                    "text": result
                }]
            })),
            error: None,
        }
    }

    async fn execute_tool(&self, tool_call: &ToolCallParams) -> Result<String> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        match tool_call.name.as_str() {
            "replace_node" => self.replace_node(args).await,
            "insert_before_node" => self.insert_before_node(args).await,
            "insert_after_node" => self.insert_after_node(args).await,
            "wrap_node" => self.wrap_node(args).await,
            "validate_syntax" => self.validate_syntax(args).await,
            "get_node_info" => self.get_node_info(args).await,
            _ => Err(anyhow!("Unknown tool: {}", tool_call.name)),
        }
    }

    async fn replace_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let new_content = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("new_content is required"))?;

        let selector = self.parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let operation = EditOperation::Replace {
            target: selector,
            new_content: new_content.to_string(),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success
            && let Some(new_code) = &result.new_content
        {
            std::fs::write(file_path, new_code)?;
        }

        Ok(format!("Replace operation result:\n{}", result.message))
    }

    async fn insert_before_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let selector = self.parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let operation = EditOperation::InsertBefore {
            target: selector,
            content: content.to_string(),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success
            && let Some(new_code) = &result.new_content
        {
            std::fs::write(file_path, new_code)?;
        }

        Ok(format!(
            "Insert before operation result:\n{}",
            result.message
        ))
    }

    async fn insert_after_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let selector = self.parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let operation = EditOperation::InsertAfter {
            target: selector,
            content: content.to_string(),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success
            && let Some(new_code) = &result.new_content
        {
            std::fs::write(file_path, new_code)?;
        }

        Ok(format!(
            "Insert after operation result:\n{}",
            result.message
        ))
    }

    async fn wrap_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let wrapper_template = args
            .get("wrapper_template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("wrapper_template is required"))?;

        let selector = self.parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let operation = EditOperation::Wrap {
            target: selector,
            wrapper_template: wrapper_template.to_string(),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success
            && let Some(new_code) = &result.new_content
        {
            std::fs::write(file_path, new_code)?;
        }

        Ok(format!("Wrap operation result:\n{}", result.message))
    }

    async fn validate_syntax(&self, args: &Value) -> Result<String> {
        if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
            let result = SyntaxValidator::validate_file(file_path)?;
            Ok(result.to_string())
        } else if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
            let language = args
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("rust");
            let result = SyntaxValidator::validate_content(content, language)?;
            Ok(result.to_string())
        } else {
            Err(anyhow!("Either file_path or content must be provided"))
        }
    }

    async fn get_node_info(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let selector = self.parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        match language.as_str() {
            "rust" => RustEditor::get_node_info(&tree, &source_code, &selector),
            _ => Err(anyhow!("Unsupported language for node info: {}", language)),
        }
    }

    fn parse_selector(&self, selector_value: Option<&Value>) -> Result<NodeSelector> {
        let selector_obj = selector_value
            .ok_or_else(|| anyhow!("selector is required"))?
            .as_object()
            .ok_or_else(|| anyhow!("selector must be an object"))?;

        if let (Some(line), Some(column)) = (
            selector_obj.get("line").and_then(|v| v.as_u64()),
            selector_obj.get("column").and_then(|v| v.as_u64()),
        ) {
            let scope = selector_obj
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Ok(NodeSelector::ByPosition {
                line: line as usize,
                column: column as usize,
                scope,
            });
        }

        if let Some(query) = selector_obj.get("query").and_then(|v| v.as_str()) {
            return Ok(NodeSelector::ByQuery {
                query: query.to_string(),
            });
        }

        if let Some(node_type) = selector_obj.get("type").and_then(|v| v.as_str()) {
            if let Some(name) = selector_obj.get("name").and_then(|v| v.as_str()) {
                return Ok(NodeSelector::ByName {
                    node_type: Some(node_type.to_string()),
                    name: name.to_string(),
                });
            } else {
                return Ok(NodeSelector::ByType {
                    node_type: node_type.to_string(),
                });
            }
        }

        if let Some(name) = selector_obj.get("name").and_then(|v| v.as_str()) {
            return Ok(NodeSelector::ByName {
                node_type: None,
                name: name.to_string(),
            });
        }

        Err(anyhow!(
            "Invalid selector: must specify position, query, type, or name"
        ))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve) | None => {
            let server = SemanticEditServer::new()?;
            run_server(server).await?;
        }
    }

    Ok(())
}

async fn run_server(server: SemanticEditServer) -> Result<()> {
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

                if let Some(response) = server.handle_message(message).await {
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
