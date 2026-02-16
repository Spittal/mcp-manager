use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::AppError;
use crate::mcp::proxy::ProxyState;
use crate::persistence::{save_enabled_integrations, save_servers};
use crate::state::{ServerConfig, ServerStatus, ServerTransport, SharedState};

/// How to parse a tool's config file.
#[derive(Debug, Clone)]
enum ConfigFormat {
    /// {"mcpServers": {"name": {...}}} — Claude Code, Cursor, Claude Desktop, Windsurf
    McpServers,
    /// {"mcp": {"name": {"type":"local","command":[...],"environment":{...}}}} — OpenCode
    OpenCode,
    /// {"context_servers": {"name": {"command":"...","args":[...],"env":{...}}}} — Zed
    Zed,
    /// TOML with [mcp_servers.name] — Codex
    CodexToml,
}

/// Internal definition for a supported AI tool.
struct ToolDef {
    id: String,
    name: String,
    config_path: PathBuf,
    detection_paths: Vec<PathBuf>,
    config_format: ConfigFormat,
    /// Whether this tool supports writing the mcp-manager proxy entry.
    supports_proxy: bool,
}

/// An existing MCP server found in a tool's config file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingMcpServer {
    /// The key in the mcpServers object (e.g. "grafana-dev").
    pub name: String,
    pub transport: String,
    /// For stdio: the command.
    pub command: Option<String>,
    /// For stdio: arguments.
    pub args: Option<Vec<String>>,
    /// For http: the URL.
    pub url: Option<String>,
}

/// Info about an AI tool, sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub enabled: bool,
    pub config_path: String,
    pub configured_port: u16,
    /// Whether this tool supports the proxy enable/disable flow.
    pub supports_proxy: bool,
    /// Existing MCP servers in this tool's config that could be imported.
    pub existing_servers: Vec<ExistingMcpServer>,
}

fn get_tool_definitions(home: &Path) -> Vec<ToolDef> {
    let mut tools = vec![
        // --- Proxy-enabled tools (can write mcp-manager entry) ---
        ToolDef {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            config_path: home.join(".claude").join("mcp.json"),
            detection_paths: vec![home.join(".claude")],
            config_format: ConfigFormat::McpServers,
            supports_proxy: true,
        },
        ToolDef {
            id: "cursor".into(),
            name: "Cursor".into(),
            config_path: home.join(".cursor").join("mcp.json"),
            detection_paths: vec![
                home.join(".cursor"),
                PathBuf::from("/Applications/Cursor.app"),
            ],
            config_format: ConfigFormat::McpServers,
            supports_proxy: true,
        },
        ToolDef {
            id: "claude-desktop".into(),
            name: "Claude Desktop".into(),
            config_path: home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            detection_paths: vec![PathBuf::from("/Applications/Claude.app")],
            config_format: ConfigFormat::McpServers,
            supports_proxy: true,
        },
    ];

    // Windsurf: check both possible locations, prefer existing config
    let codeium_path = home.join(".codeium/windsurf/mcp_config.json");
    let windsurf_path = home.join(".windsurf/mcp.json");
    let config_path = if windsurf_path.exists() {
        windsurf_path
    } else {
        codeium_path
    };

    tools.push(ToolDef {
        id: "windsurf".into(),
        name: "Windsurf".into(),
        config_path,
        detection_paths: vec![
            home.join(".codeium/windsurf"),
            home.join(".windsurf"),
            PathBuf::from("/Applications/Windsurf.app"),
        ],
        config_format: ConfigFormat::McpServers,
        supports_proxy: true,
    });

    // --- Read-only sources (detect servers but don't write proxy config) ---

    tools.push(ToolDef {
        id: "mcp-json".into(),
        name: "MCP Config".into(),
        config_path: home.join(".mcp.json"),
        // Always "installed" — just check if the file exists
        detection_paths: vec![home.join(".mcp.json")],
        config_format: ConfigFormat::McpServers,
        supports_proxy: false,
    });

    tools.push(ToolDef {
        id: "claude-code-user".into(),
        name: "Claude Code (User)".into(),
        config_path: home.join(".claude.json"),
        detection_paths: vec![home.join(".claude.json")],
        config_format: ConfigFormat::McpServers,
        supports_proxy: false,
    });

    tools.push(ToolDef {
        id: "opencode".into(),
        name: "OpenCode".into(),
        config_path: home.join(".config/opencode/opencode.json"),
        detection_paths: vec![home.join(".config/opencode")],
        config_format: ConfigFormat::OpenCode,
        supports_proxy: false,
    });

    tools.push(ToolDef {
        id: "codex".into(),
        name: "Codex".into(),
        config_path: home.join(".codex/config.toml"),
        detection_paths: vec![home.join(".codex")],
        config_format: ConfigFormat::CodexToml,
        supports_proxy: false,
    });

    tools.push(ToolDef {
        id: "zed".into(),
        name: "Zed".into(),
        config_path: home.join(".config/zed/settings.json"),
        detection_paths: vec![
            home.join(".config/zed"),
            PathBuf::from("/Applications/Zed.app"),
        ],
        config_format: ConfigFormat::Zed,
        supports_proxy: false,
    });

    tools
}

fn find_tool_def(home: &Path, id: &str) -> Result<ToolDef, AppError> {
    get_tool_definitions(home)
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::IntegrationNotFound(id.to_string()))
}

/// Check whether a URL looks like one of our proxy endpoints.
fn is_proxy_url(url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.scheme() == "http"
            && matches!(parsed.host_str(), Some("localhost") | Some("127.0.0.1"))
            && (parsed.path().starts_with("/mcp/") || parsed.path() == "/mcp")
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Config parsing — format-specific
// ---------------------------------------------------------------------------

/// Parse a tool's config file and return (enabled, port, existing_servers).
fn parse_config(path: &Path, format: &ConfigFormat) -> (bool, u16, Vec<ExistingMcpServer>) {
    match format {
        ConfigFormat::McpServers => parse_mcp_servers(path),
        ConfigFormat::OpenCode => parse_opencode(path),
        ConfigFormat::Zed => parse_zed(path),
        ConfigFormat::CodexToml => parse_codex_toml(path),
    }
}

/// Standard mcpServers format (Claude, Cursor, Windsurf, etc.)
fn parse_mcp_servers(path: &Path) -> (bool, u16, Vec<ExistingMcpServer>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (false, 0, Vec::new()),
    };
    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (false, 0, Vec::new()),
    };

    let servers_obj = match config.get("mcpServers").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return (false, 0, Vec::new()),
    };

    let mut enabled = false;
    let mut port = 0u16;
    let mut existing = Vec::new();

    for (key, value) in servers_obj {
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");

        // Detect our proxy entries
        if key == "mcp-manager" || is_proxy_url(entry_url) {
            enabled = true;
            if port == 0 {
                port = extract_port_from_url(entry_url);
            }
            continue;
        }

        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if has_url { "http".into() } else { "stdio".into() },
            command: value
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            url: if has_url {
                value.get("url").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            },
        });
    }

    (enabled, port, existing)
}

/// Parse a JSON MCP server entry into a ServerConfig for import.
fn server_config_from_json(key: &str, value: &serde_json::Value) -> ServerConfig {
    let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

    ServerConfig {
        id: Uuid::new_v4().to_string(),
        name: key.to_string(),
        enabled: true,
        transport: if has_url {
            ServerTransport::Http
        } else {
            ServerTransport::Stdio
        },
        command: value
            .get("command")
            .and_then(|v| v.as_str())
            .map(String::from),
        args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        }),
        env: value.get("env").and_then(|v| v.as_object()).map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<String, String>>()
        }),
        url: if has_url {
            value.get("url").and_then(|v| v.as_str()).map(String::from)
        } else {
            None
        },
        headers: value
            .get("headers")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect::<HashMap<String, String>>()
            }),
        tags: None,
        status: Some(ServerStatus::Disconnected),
        last_connected: None,
        managed: None,
    }
}

/// OpenCode format: {"mcp": {"name": {"type":"local","command":[...],"environment":{...}}}}
fn parse_opencode(path: &Path) -> (bool, u16, Vec<ExistingMcpServer>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (false, 0, Vec::new()),
    };
    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (false, 0, Vec::new()),
    };

    let servers_obj = match config.get("mcp").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return (false, 0, Vec::new()),
    };

    let mut existing = Vec::new();

    for (key, value) in servers_obj {
        let server_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("local");
        let is_remote = server_type == "remote";

        // OpenCode uses "command" as an array: ["npx", "-y", "some-server"]
        let (command, args) = if let Some(cmd_arr) = value.get("command").and_then(|v| v.as_array())
        {
            let parts: Vec<String> = cmd_arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if parts.is_empty() {
                (None, None)
            } else {
                let cmd = Some(parts[0].clone());
                let a = if parts.len() > 1 {
                    Some(parts[1..].to_vec())
                } else {
                    None
                };
                (cmd, a)
            }
        } else {
            (None, None)
        };

        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if is_remote { "http".into() } else { "stdio".into() },
            command,
            args,
            url: value.get("url").and_then(|v| v.as_str()).map(String::from),
        });
    }

    // OpenCode doesn't support our proxy entry, so never "enabled"
    (false, 0, existing)
}

/// Zed format: {"context_servers": {"name": {"command":"...","args":[...],"env":{...}}}}
fn parse_zed(path: &Path) -> (bool, u16, Vec<ExistingMcpServer>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (false, 0, Vec::new()),
    };
    // Zed settings.json may contain comments — strip them before parsing
    let stripped = strip_json_comments(&content);
    let config: serde_json::Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(_) => return (false, 0, Vec::new()),
    };

    let servers_obj = match config.get("context_servers").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return (false, 0, Vec::new()),
    };

    let mut existing = Vec::new();

    for (key, value) in servers_obj {
        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

        // Zed uses the same flat format: command, args, env at top level
        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if has_url { "http".into() } else { "stdio".into() },
            command: value.get("command").and_then(|v| v.as_str()).map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            url: if has_url {
                value.get("url").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            },
        });
    }

    (false, 0, existing)
}

/// Codex TOML format: [mcp_servers.name] with command, args, url, etc.
fn parse_codex_toml(path: &Path) -> (bool, u16, Vec<ExistingMcpServer>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (false, 0, Vec::new()),
    };
    let config: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return (false, 0, Vec::new()),
    };

    let servers_table = match config.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return (false, 0, Vec::new()),
    };

    let mut existing = Vec::new();

    for (key, value) in servers_table {
        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if has_url { "http".into() } else { "stdio".into() },
            command: value.get("command").and_then(|v| v.as_str()).map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            url: if has_url {
                value.get("url").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            },
        });
    }

    (false, 0, existing)
}

// ---------------------------------------------------------------------------
// Import — read full ServerConfig from each format (including env)
// ---------------------------------------------------------------------------

/// Read a tool's config and return ready-to-insert ServerConfig entries.
fn read_importable_servers(tool: &ToolDef) -> Result<Vec<ServerConfig>, AppError> {
    if !tool.config_path.exists() {
        return Ok(Vec::new());
    }
    match &tool.config_format {
        ConfigFormat::McpServers => import_mcp_servers(&tool.config_path),
        ConfigFormat::OpenCode => import_opencode(&tool.config_path),
        ConfigFormat::Zed => import_zed(&tool.config_path),
        ConfigFormat::CodexToml => import_codex_toml(&tool.config_path),
    }
}

fn json_obj_to_env(value: &serde_json::Value, key: &str) -> Option<HashMap<String, String>> {
    value.get(key).and_then(|v| v.as_object()).map(|obj| {
        obj.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    })
}

fn import_mcp_servers(path: &Path) -> Result<Vec<ServerConfig>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AppError::Protocol(format!("Invalid JSON: {e}")))?;
    let servers_obj = match config.get("mcpServers").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for (key, value) in servers_obj {
        if key == "mcp-manager" {
            continue;
        }
        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();
        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
            enabled: true,
            transport: if has_url { ServerTransport::Http } else { ServerTransport::Stdio },
            command: value.get("command").and_then(|v| v.as_str()).map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
            }),
            env: json_obj_to_env(value, "env"),
            url: if has_url { value.get("url").and_then(|v| v.as_str()).map(String::from) } else { None },
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
        });
    }
    Ok(result)
}

fn import_opencode(path: &Path) -> Result<Vec<ServerConfig>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AppError::Protocol(format!("Invalid JSON: {e}")))?;
    let servers_obj = match config.get("mcp").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for (key, value) in servers_obj {
        let server_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("local");
        let is_remote = server_type == "remote";

        // OpenCode: "command" is an array, "environment" instead of "env"
        let (command, args) = if let Some(cmd_arr) = value.get("command").and_then(|v| v.as_array()) {
            let parts: Vec<String> = cmd_arr.iter().filter_map(|v| v.as_str().map(String::from)).collect();
            if parts.is_empty() {
                (None, None)
            } else {
                (Some(parts[0].clone()), if parts.len() > 1 { Some(parts[1..].to_vec()) } else { None })
            }
        } else {
            (None, None)
        };

        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
            enabled: true,
            transport: if is_remote { ServerTransport::Http } else { ServerTransport::Stdio },
            command,
            args,
            env: json_obj_to_env(value, "environment"),
            url: value.get("url").and_then(|v| v.as_str()).map(String::from),
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
        });
    }
    Ok(result)
}

fn import_zed(path: &Path) -> Result<Vec<ServerConfig>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let stripped = strip_json_comments(&content);
    let config: serde_json::Value = serde_json::from_str(&stripped)
        .map_err(|e| AppError::Protocol(format!("Invalid JSON: {e}")))?;
    let servers_obj = match config.get("context_servers").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for (key, value) in servers_obj {
        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();
        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
            enabled: true,
            transport: if has_url { ServerTransport::Http } else { ServerTransport::Stdio },
            command: value.get("command").and_then(|v| v.as_str()).map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
            }),
            env: json_obj_to_env(value, "env"),
            url: if has_url { value.get("url").and_then(|v| v.as_str()).map(String::from) } else { None },
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
        });
    }
    Ok(result)
}

fn import_codex_toml(path: &Path) -> Result<Vec<ServerConfig>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let config: toml::Value = content.parse()
        .map_err(|e| AppError::Protocol(format!("Invalid TOML: {e}")))?;
    let servers_table = match config.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for (key, value) in servers_table {
        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

        let env = value.get("env").and_then(|v| v.as_table()).map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<String, String>>()
        });

        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
            enabled: true,
            transport: if has_url { ServerTransport::Http } else { ServerTransport::Stdio },
            command: value.get("command").and_then(|v| v.as_str()).map(String::from),
            args: value.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
            }),
            env,
            url: if has_url { value.get("url").and_then(|v| v.as_str()).map(String::from) } else { None },
            headers: None,
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
        });
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract port number from a URL like "http://localhost:12345/mcp".
fn extract_port_from_url(url: &str) -> u16 {
    if let Ok(parsed) = url::Url::parse(url) {
        return parsed.port().unwrap_or(0);
    }
    0
}

/// Strip single-line (//) and multi-line (/* */) comments from JSON.
/// Needed for Zed's settings.json which allows comments.
fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(&ch) = chars.peek() {
        if in_string {
            out.push(ch);
            chars.next();
            if ch == '\\' {
                // Skip escaped character
                if let Some(&next) = chars.peek() {
                    out.push(next);
                    chars.next();
                }
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
            out.push(ch);
            chars.next();
        } else if ch == '/' {
            chars.next();
            match chars.peek() {
                Some(&'/') => {
                    // Single-line comment — skip to end of line
                    for c in chars.by_ref() {
                        if c == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some(&'*') => {
                    // Multi-line comment — skip to */
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {
                    out.push('/');
                }
            }
        } else {
            out.push(ch);
            chars.next();
        }
    }

    out
}

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn detect_integrations(
    state: State<'_, SharedState>,
    proxy_state: State<'_, ProxyState>,
) -> Result<Vec<AiToolInfo>, AppError> {
    let home = home_dir()?;
    let tools = get_tool_definitions(&home);
    let _port = proxy_state.port().await;

    let enabled_ids: Vec<String> = {
        let s = state.lock().unwrap();
        s.enabled_integrations.clone()
    };

    let mut results = Vec::new();
    for tool in tools {
        let installed = tool.detection_paths.iter().any(|p| p.exists());
        let enabled = enabled_ids.contains(&tool.id);

        let (_, configured_port, existing_servers) = if installed {
            parse_config(&tool.config_path, &tool.config_format)
        } else {
            (false, 0, Vec::new())
        };

        results.push(AiToolInfo {
            id: tool.id,
            name: tool.name,
            installed,
            enabled,
            config_path: tool.config_path.display().to_string(),
            configured_port,
            supports_proxy: tool.supports_proxy,
            existing_servers,
        });
    }

    Ok(results)
}

#[tauri::command]
pub async fn import_from_tool(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<usize, AppError> {
    let home = home_dir()?;
    let tool = find_tool_def(&home, &id)?;
    let candidates = read_importable_servers(&tool)?;

    let imported = {
        let mut s = state.lock().unwrap();
        let existing_names: Vec<String> = s.servers.iter().map(|srv| srv.name.clone()).collect();

        let mut imported = 0;
        for server in candidates {
            if existing_names.contains(&server.name) {
                info!("Skipping import of '{}' — already exists", server.name);
                continue;
            }
            info!("Imported MCP server '{}' from {}", server.name, tool.name);
            s.servers.push(server);
            imported += 1;
        }

        if imported > 0 {
            save_servers(&app, &s.servers);
        }

        // Mark this tool as managed (even if 0 new servers — they were already imported before)
        if !s.enabled_integrations.contains(&id) {
            s.enabled_integrations.push(id.clone());
            save_enabled_integrations(&app, &s.enabled_integrations);
        }

        imported
    }; // lock dropped here — rebuild_tray_menu also acquires it

    if imported > 0 {
        crate::tray::rebuild_tray_menu(&app);
    }

    info!("Imported {imported} server(s) from {}", tool.name);
    Ok(imported)
}

#[tauri::command]
pub async fn enable_integration(
    app: AppHandle,
    proxy_state: State<'_, ProxyState>,
    state: State<'_, SharedState>,
    id: String,
) -> Result<AiToolInfo, AppError> {
    let home = home_dir()?;
    let tool = find_tool_def(&home, &id)?;

    if !tool.supports_proxy {
        return Err(AppError::Protocol(format!(
            "{} does not support proxy integration",
            tool.name
        )));
    }

    let port = proxy_state.port().await;

    // Read existing config to find servers to migrate
    let existing_config: serde_json::Value = if tool.config_path.exists() {
        let content = std::fs::read_to_string(&tool.config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Import existing MCP servers into MCP Manager
    let mut imported_count = 0;
    if let Some(servers_obj) = existing_config.get("mcpServers").and_then(|v| v.as_object()) {
        let mut s = state.lock().unwrap();
        let existing_names: Vec<String> = s.servers.iter().map(|srv| srv.name.clone()).collect();

        for (key, value) in servers_obj {
            // Skip the legacy mcp-manager entry
            if key == "mcp-manager" {
                continue;
            }

            // Skip entries that point to our proxy
            let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if is_proxy_url(entry_url) {
                continue;
            }

            // Skip if a server with this name already exists in MCP Manager
            if existing_names.contains(key) {
                info!("Skipping import of '{key}' — already exists in MCP Manager");
                continue;
            }

            let server = server_config_from_json(key, value);
            info!("Imported MCP server '{}' from {}", key, tool.name);
            s.servers.push(server);
            imported_count += 1;
        }

        // Add this integration to enabled list
        if !s.enabled_integrations.contains(&id) {
            s.enabled_integrations.push(id.clone());
        }

        save_servers(&app, &s.servers);
        save_enabled_integrations(&app, &s.enabled_integrations);
    } else {
        // No servers to import, just enable the integration
        let mut s = state.lock().unwrap();
        if !s.enabled_integrations.contains(&id) {
            s.enabled_integrations.push(id.clone());
        }
        save_enabled_integrations(&app, &s.enabled_integrations);
    }

    if imported_count > 0 {
        info!(
            "Imported {imported_count} MCP server(s) from {}",
            tool.name
        );
        crate::tray::rebuild_tray_menu(&app);
    }

    // Write per-server proxy entries for all currently connected servers
    write_per_server_config(&app, &tool.config_path, port, &tool.id)?;

    info!(
        "Enabled MCP Manager integration for {} (port {})",
        tool.name, port
    );

    Ok(AiToolInfo {
        id: tool.id,
        name: tool.name,
        installed: true,
        enabled: true,
        config_path: tool.config_path.display().to_string(),
        configured_port: port,
        supports_proxy: true,
        existing_servers: Vec::new(),
    })
}

#[tauri::command]
pub async fn disable_integration(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<AiToolInfo, AppError> {
    let home = home_dir()?;
    let tool = find_tool_def(&home, &id)?;

    if !tool.supports_proxy {
        return Err(AppError::Protocol(format!(
            "{} does not support proxy integration",
            tool.name
        )));
    }

    // Remove from enabled list
    {
        let mut s = state.lock().unwrap();
        s.enabled_integrations.retain(|i| i != &id);
        save_enabled_integrations(&app, &s.enabled_integrations);
    }

    if !tool.config_path.exists() {
        return Ok(AiToolInfo {
            id: tool.id,
            name: tool.name,
            installed: true,
            enabled: false,
            config_path: tool.config_path.display().to_string(),
            configured_port: 0,
            supports_proxy: true,
            existing_servers: Vec::new(),
        });
    }

    // Remove our proxy entries from the config file
    remove_proxy_entries(&tool.config_path)?;

    info!("Disabled MCP Manager integration for {}", tool.name);

    let (_, _, existing_servers) = parse_config(&tool.config_path, &tool.config_format);

    Ok(AiToolInfo {
        id: tool.id,
        name: tool.name,
        installed: true,
        enabled: false,
        config_path: tool.config_path.display().to_string(),
        configured_port: 0,
        supports_proxy: true,
        existing_servers,
    })
}

/// Write per-server proxy entries to a tool's config file.
/// Each connected server gets its own entry in mcpServers.
/// `tool_id` is appended as `?client=` so the proxy can identify the calling AI tool.
fn write_per_server_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
) -> Result<(), AppError> {
    let state = app.state::<SharedState>();
    let s = state.lock().unwrap();

    let mut mcp_servers = serde_json::Map::new();
    for srv in &s.servers {
        if srv.status != Some(ServerStatus::Connected) {
            continue;
        }
        mcp_servers.insert(
            srv.name.clone(),
            serde_json::json!({
                "url": format!("http://localhost:{port}/mcp/{}?client={tool_id}", srv.id)
            }),
        );
    }

    drop(s);

    // Read existing config to preserve other top-level keys
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["mcpServers"] = serde_json::Value::Object(mcp_servers);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;

    Ok(())
}

/// Remove all proxy entries from a tool's config file (entries with our proxy URLs).
fn remove_proxy_entries(path: &Path) -> Result<(), AppError> {
    let content = std::fs::read_to_string(path)?;
    let mut config: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        let proxy_keys: Vec<String> = servers
            .iter()
            .filter(|(k, v)| {
                *k == "mcp-manager"
                    || v.get("url")
                        .and_then(|u| u.as_str())
                        .map(is_proxy_url)
                        .unwrap_or(false)
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in proxy_keys {
            servers.remove(&key);
        }
    }

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;

    Ok(())
}

/// Update all enabled integration configs with current connected servers.
/// Called on proxy startup, server connect/disconnect, and enable/disable.
pub fn update_all_integration_configs(app: &AppHandle, port: u16) -> Result<(), AppError> {
    let home = home_dir()?;
    let tools = get_tool_definitions(&home);

    let enabled_ids: Vec<String> = {
        let state = app.state::<SharedState>();
        let s = state.lock().unwrap();
        s.enabled_integrations.clone()
    };

    for tool in tools {
        if !enabled_ids.contains(&tool.id) || !tool.supports_proxy || !tool.config_path.exists() {
            continue;
        }

        if let Err(e) = write_per_server_config(app, &tool.config_path, port, &tool.id) {
            warn!("Failed to update config for {}: {e}", tool.name);
        } else {
            info!("Updated {} config with per-server proxy entries", tool.name);
        }
    }

    Ok(())
}
