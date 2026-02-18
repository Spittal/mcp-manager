mod embedding;
mod oauth;
mod providers;
pub mod registry;
mod server;

pub use embedding::*;
pub use oauth::*;
pub use server::*;

use std::collections::HashMap;
use std::sync::Mutex;

/// A log entry buffered before the frontend is ready.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BufferedLog {
    pub server_id: String,
    pub level: String,
    pub message: String,
}

pub struct AppState {
    pub servers: Vec<ServerConfig>,
    pub connections: HashMap<String, ConnectionState>,
    /// IDs of AI tool integrations that MCP Manager is configured to manage.
    pub enabled_integrations: Vec<String>,
    pub embedding_config: EmbeddingConfig,
    /// Logs emitted before the frontend event listener is ready.
    pub log_buffer: Vec<BufferedLog>,
    /// When true, integrations get a single discovery endpoint instead of per-server entries.
    pub tool_discovery_enabled: bool,
}

pub struct ConnectionState {
    pub tools: Vec<McpTool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            connections: HashMap::new(),
            enabled_integrations: Vec::new(),
            embedding_config: EmbeddingConfig::default(),
            log_buffer: Vec::new(),
            tool_discovery_enabled: false,
        }
    }
}

pub type SharedState = Mutex<AppState>;
