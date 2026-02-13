use std::collections::HashMap;

use tauri::AppHandle;
use tokio::sync::Mutex;
use tracing::info;

use crate::error::AppError;
use crate::mcp::http_transport::HttpTransport;
use crate::mcp::transport::StdioTransport;
use crate::mcp::types::*;

/// Transport abstraction — either stdio (local process) or HTTP (remote server).
enum Transport {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

/// MCP client wrapping either a stdio or HTTP transport.
pub struct McpClient {
    transport: Transport,
    pub server_capabilities: Option<ServerCapabilities>,
    pub server_info: Option<ServerInfo>,
    pub tools: Vec<McpToolDef>,
}

impl McpClient {
    /// Spawn an MCP server, perform the initialization handshake, and discover tools.
    pub async fn connect_stdio(
        app: &AppHandle,
        server_id: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, AppError> {
        let transport = StdioTransport::spawn(app, server_id, command, args, env)?;

        let mut client = Self {
            transport: Transport::Stdio(transport),
            server_capabilities: None,
            server_info: None,
            tools: Vec::new(),
        };

        client.initialize().await?;
        client.discover_tools().await?;

        Ok(client)
    }

    /// Connect to a remote MCP server via HTTP, perform initialization, and discover tools.
    pub async fn connect_http(
        url: &str,
        headers: HashMap<String, String>,
        access_token: Option<String>,
    ) -> Result<Self, AppError> {
        let transport = HttpTransport::connect(url, headers, access_token).await?;

        let mut client = Self {
            transport: Transport::Http(transport),
            server_capabilities: None,
            server_info: None,
            tools: Vec::new(),
        };

        client.initialize().await?;
        client.discover_tools().await?;

        Ok(client)
    }

    /// Send the MCP initialize request and notifications/initialized.
    async fn initialize(&mut self) -> Result<(), AppError> {
        let params = InitializeParams {
            protocol_version: "2025-03-26".to_string(),
            capabilities: ClientCapabilities {
                roots: None,
                sampling: None,
            },
            client_info: ClientInfo {
                name: "MCP Manager".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let params_json = serde_json::to_value(&params)
            .map_err(|e| AppError::Protocol(format!("Failed to serialize init params: {e}")))?;

        let response = self
            .send_request("initialize", Some(params_json))
            .await?;

        let result: InitializeResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| AppError::Protocol("No result in initialize response".into()))?,
        )
        .map_err(|e| AppError::Protocol(format!("Failed to parse initialize result: {e}")))?;

        info!(
            "MCP server initialized: {} v{}",
            result.server_info.name, result.server_info.version
        );

        self.server_capabilities = Some(result.capabilities);
        self.server_info = Some(result.server_info);

        // Send initialized notification
        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// Send tools/list and store the results.
    async fn discover_tools(&mut self) -> Result<(), AppError> {
        let response = self
            .send_request("tools/list", Some(serde_json::json!({})))
            .await?;

        let result = response
            .result
            .ok_or_else(|| AppError::Protocol("No result in tools/list response".into()))?;

        #[derive(serde::Deserialize)]
        struct ToolsListResult {
            tools: Vec<McpToolDef>,
        }

        let tools_result: ToolsListResult = serde_json::from_value(result)
            .map_err(|e| AppError::Protocol(format!("Failed to parse tools list: {e}")))?;

        info!("Discovered {} tools", tools_result.tools.len());
        self.tools = tools_result.tools;

        Ok(())
    }

    /// Call a tool by name with the given arguments.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, AppError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let response = self
            .send_request("tools/call", Some(params))
            .await?;

        let result = response
            .result
            .ok_or_else(|| AppError::Protocol("No result in tools/call response".into()))?;

        let call_result: CallToolResult = serde_json::from_value(result)
            .map_err(|e| AppError::Protocol(format!("Failed to parse tool call result: {e}")))?;

        Ok(call_result)
    }

    /// Shut down the client.
    pub fn shutdown(&self) {
        match &self.transport {
            Transport::Stdio(t) => t.shutdown(),
            Transport::Http(_) => {
                // HTTP transport has no persistent process to kill.
                // Session cleanup (DELETE) would require async; dropping the
                // transport is sufficient — the server will expire the session.
                tracing::debug!("HTTP transport shutdown");
            }
        }
    }

    // -- Private helpers delegating to the active transport --

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, AppError> {
        match &self.transport {
            Transport::Stdio(t) => t.send_request(method, params).await,
            Transport::Http(t) => t.send_request(method, params).await,
        }
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), AppError> {
        match &self.transport {
            Transport::Stdio(t) => t.send_notification(method, params).await,
            Transport::Http(t) => t.send_notification(method, params).await,
        }
    }
}

/// Result from calling a tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<McpContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Holds active MCP client connections, keyed by server ID.
/// This is separate from AppState because McpClient is not Send-safe
/// behind a std::sync::Mutex (it contains tokio types).
pub struct McpConnections {
    clients: HashMap<String, McpClient>,
}

impl McpConnections {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: String, client: McpClient) {
        self.clients.insert(id, client);
    }

    pub fn remove(&mut self, id: &str) -> Option<McpClient> {
        self.clients.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&McpClient> {
        self.clients.get(id)
    }
}

pub type SharedConnections = Mutex<McpConnections>;
