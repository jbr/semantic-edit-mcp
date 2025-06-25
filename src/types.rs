use crate::state::SemanticEditTools;
use crate::tools::Tools;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::{borrow::Cow, collections::HashMap};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpMessage {
    Request(McpRequest),
    Notification(McpNotification),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(flatten)]
    pub call: RequestType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum RequestType {
    #[serde(rename = "initialize")]
    Initialize(InitializeRequest),

    #[serde(rename = "tools/list")]
    ToolsList(Value),

    #[serde(rename = "tools/call")]
    ToolsCall(Tools),
}

impl RequestType {
    pub fn execute(
        self,
        id: Value,
        state: &mut SemanticEditTools,
        instructions: Option<&'static str>,
    ) -> McpResponse {
        match self {
            RequestType::Initialize(_) => McpResponse::success(
                id,
                InitializeResponse::default().with_instructions(instructions),
            ),

            RequestType::ToolsList(_) => McpResponse::success(
                id,
                ToolsListResponse {
                    tools: Tools::schema(),
                },
            ),

            RequestType::ToolsCall(tool) => match tool.execute(state) {
                Ok(string) => McpResponse::success(id, ContentResponse::text(string)),
                Err(e) => McpResponse::error(id, -32601, e.to_string()),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    capabilities: Value,
    client_info: Info,
    protocol_version: String,
}

#[derive(Debug, Serialize, Deserialize, fieldwork::Fieldwork)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    protocol_version: &'static str,
    capabilities: Capabilities,
    server_info: Info,
    #[fieldwork(with)]
    instructions: Option<&'static str>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Example<T> {
    pub description: &'static str,
    #[serde(flatten)]
    pub item: T,
}

impl Default for InitializeResponse {
    fn default() -> Self {
        Self {
            protocol_version: "2024-11-05",
            capabilities: Capabilities::default(),
            server_info: Info {
                name: env!("CARGO_PKG_NAME").into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    pub name: Cow<'static, str>,
    pub version: Cow<'static, str>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub tools: HashMap<(), ()>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ToolsListResponse {
    pub tools: Vec<ToolSchema>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSchema {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: InputSchema,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InputSchema {
    #[serde(rename = "object")]
    Object {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        properties: HashMap<String, Box<InputSchema>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_properties: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<Value>>,
    },
    #[serde(rename = "string")]
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        r#enum: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<String>>,
    },

    #[serde(rename = "boolean")]
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },

    #[serde(rename = "integer")]
    Integer {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },

    #[serde(rename = "array")]
    Array {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        items: Box<InputSchema>,
    },

    #[serde(rename = "null")]
    Null,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ContentResponse {
    content: Vec<TextContent>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextContent {
    pub r#type: &'static str,
    pub text: String,
}

impl ContentResponse {
    pub fn text(text: String) -> Self {
        Self {
            content: vec![TextContent {
                r#type: "text",
                text,
            }],
        }
    }
}

impl McpResponse {
    pub fn success(id: Value, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(McpError {
                code,
                message,
                data: None,
            }),
        }
    }
}
