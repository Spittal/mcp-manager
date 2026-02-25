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
    /// Existing MCP servers in this tool's config that could be imported.
    pub existing_servers: Vec<ExistingMcpServer>,
}

fn get_tool_definitions(home: &Path) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            config_path: home.join(".claude").join("mcp.json"),
            detection_paths: vec![home.join(".claude")],
            config_format: ConfigFormat::McpServers,
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
        },
        ToolDef {
            id: "claude-desktop".into(),
            name: "Claude Desktop".into(),
            config_path: home.join("Library/Application Support/Claude/claude_desktop_config.json"),
            detection_paths: vec![PathBuf::from("/Applications/Claude.app")],
            config_format: ConfigFormat::McpServers,
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
    });

    tools.push(ToolDef {
        id: "mcp-json".into(),
        name: "MCP Config".into(),
        config_path: home.join(".mcp.json"),
        // Always "installed" — just check if the file exists
        detection_paths: vec![home.join(".mcp.json")],
        config_format: ConfigFormat::McpServers,
    });

    tools.push(ToolDef {
        id: "claude-code-user".into(),
        name: "Claude Code (User)".into(),
        config_path: home.join(".claude.json"),
        detection_paths: vec![home.join(".claude.json")],
        config_format: ConfigFormat::McpServers,
    });

    tools.push(ToolDef {
        id: "opencode".into(),
        name: "OpenCode".into(),
        config_path: home.join(".config/opencode/opencode.json"),
        detection_paths: vec![home.join(".config/opencode")],
        config_format: ConfigFormat::OpenCode,
    });

    tools.push(ToolDef {
        id: "codex".into(),
        name: "Codex".into(),
        config_path: home.join(".codex/config.toml"),
        detection_paths: vec![home.join(".codex")],
        config_format: ConfigFormat::CodexToml,
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
            transport: if has_url {
                "http".into()
            } else {
                "stdio".into()
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
            url: if has_url {
                value.get("url").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            },
        });
    }

    (enabled, port, existing)
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

    let mut enabled = false;
    let mut port = 0u16;
    let mut existing = Vec::new();

    for (key, value) in servers_obj {
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");

        if is_proxy_url(entry_url) {
            enabled = true;
            if port == 0 {
                port = extract_port_from_url(entry_url);
            }
            continue;
        }

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
            transport: if is_remote {
                "http".into()
            } else {
                "stdio".into()
            },
            command,
            args,
            url: value.get("url").and_then(|v| v.as_str()).map(String::from),
        });
    }

    (enabled, port, existing)
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

    let mut enabled = false;
    let mut port = 0u16;
    let mut existing = Vec::new();

    for (key, value) in servers_obj {
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");

        if is_proxy_url(entry_url) {
            enabled = true;
            if port == 0 {
                port = extract_port_from_url(entry_url);
            }
            continue;
        }

        let has_url = !entry_url.is_empty();

        // Zed uses the same flat format: command, args, env at top level
        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if has_url {
                "http".into()
            } else {
                "stdio".into()
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
            url: if has_url {
                Some(entry_url.to_string())
            } else {
                None
            },
        });
    }

    (enabled, port, existing)
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

    let mut enabled = false;
    let mut port = 0u16;
    let mut existing = Vec::new();

    for (key, value) in servers_table {
        let entry_url = value.get("url").and_then(|v| v.as_str()).unwrap_or("");

        if is_proxy_url(entry_url) {
            enabled = true;
            if port == 0 {
                port = extract_port_from_url(entry_url);
            }
            continue;
        }

        let has_url = !entry_url.is_empty();

        existing.push(ExistingMcpServer {
            name: key.clone(),
            transport: if has_url {
                "http".into()
            } else {
                "stdio".into()
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
            url: if has_url {
                Some(entry_url.to_string())
            } else {
                None
            },
        });
    }

    (enabled, port, existing)
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
        // Skip legacy mcp-manager entry and proxy URLs
        if key == "mcp-manager" {
            continue;
        }
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");
        if is_proxy_url(entry_url) {
            continue;
        }

        let has_url = value.get("url").and_then(|v| v.as_str()).is_some();
        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
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
            env: json_obj_to_env(value, "env"),
            url: if has_url {
                value.get("url").and_then(|v| v.as_str()).map(String::from)
            } else {
                None
            },
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
            managed_by: None,
            registry_name: None,
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
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");
        if is_proxy_url(entry_url) {
            continue;
        }

        let server_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("local");
        let is_remote = server_type == "remote";

        // OpenCode: "command" is an array, "environment" instead of "env"
        let (command, args) = if let Some(cmd_arr) = value.get("command").and_then(|v| v.as_array())
        {
            let parts: Vec<String> = cmd_arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if parts.is_empty() {
                (None, None)
            } else {
                (
                    Some(parts[0].clone()),
                    if parts.len() > 1 {
                        Some(parts[1..].to_vec())
                    } else {
                        None
                    },
                )
            }
        } else {
            (None, None)
        };

        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
            enabled: true,
            transport: if is_remote {
                ServerTransport::Http
            } else {
                ServerTransport::Stdio
            },
            command,
            args,
            env: json_obj_to_env(value, "environment"),
            url: value.get("url").and_then(|v| v.as_str()).map(String::from),
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
            managed_by: None,
            registry_name: None,
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
        let entry_url = value.get("url").and_then(|u| u.as_str()).unwrap_or("");
        if is_proxy_url(entry_url) {
            continue;
        }

        let has_url = !entry_url.is_empty();
        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
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
            env: json_obj_to_env(value, "env"),
            url: if has_url {
                Some(entry_url.to_string())
            } else {
                None
            },
            headers: json_obj_to_env(value, "headers"),
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
            managed_by: None,
            registry_name: None,
        });
    }
    Ok(result)
}

fn import_codex_toml(path: &Path) -> Result<Vec<ServerConfig>, AppError> {
    let content = std::fs::read_to_string(path)?;
    let config: toml::Value = content
        .parse()
        .map_err(|e| AppError::Protocol(format!("Invalid TOML: {e}")))?;
    let servers_table = match config.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return Ok(Vec::new()),
    };
    let mut result = Vec::new();
    for (key, value) in servers_table {
        let entry_url = value.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if is_proxy_url(entry_url) {
            continue;
        }

        let has_url = !entry_url.is_empty();

        let env = value.get("env").and_then(|v| v.as_table()).map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<String, String>>()
        });

        result.push(ServerConfig {
            id: Uuid::new_v4().to_string(),
            name: key.clone(),
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
            env,
            url: if has_url {
                Some(entry_url.to_string())
            } else {
                None
            },
            headers: None,
            tags: None,
            status: Some(ServerStatus::Disconnected),
            last_connected: None,
            managed: None,
            managed_by: None,
            registry_name: None,
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

/// Build a single discovery endpoint URL entry.
fn discovery_proxy_url(port: u16, tool_id: &str) -> (String, String) {
    (
        "mcp-manager".to_string(),
        format!("http://localhost:{port}/mcp/discovery?client={tool_id}"),
    )
}

/// Build proxy URL entries for all currently connected servers.
/// In discovery mode, returns a single entry pointing to the discovery endpoint.
fn connected_proxy_urls(app: &AppHandle, port: u16, tool_id: &str) -> Vec<(String, String)> {
    let state = app.state::<SharedState>();
    let s = state.lock().unwrap();

    if s.tool_discovery_enabled {
        // Always expose the discovery endpoint when discovery mode is on.
        // The endpoint itself handles "no servers connected" gracefully via
        // list_servers / discover_tools responses. Gating on has_connected
        // causes a startup race: proxy starts before servers reconnect,
        // writing empty mcpServers to integration configs.
        return vec![discovery_proxy_url(port, tool_id)];
    }

    s.servers
        .iter()
        .filter(|srv| srv.status == Some(ServerStatus::Connected))
        .map(|srv| {
            (
                srv.name.clone(),
                format!("http://localhost:{port}/mcp/{}?client={tool_id}", srv.id),
            )
        })
        .collect()
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
            existing_servers,
        });
    }

    Ok(results)
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
    let port = proxy_state.port().await;

    // Import existing servers from the config file (format-agnostic)
    let candidates = read_importable_servers(&tool)?;

    let imported_count = {
        let mut s = state.lock().unwrap();

        let mut imported = 0;
        for server in candidates {
            if let Some(idx) = s.servers.iter().position(|srv| srv.name == server.name) {
                info!(
                    "Replacing existing server '{}' with import from {}",
                    server.name, tool.name
                );
                s.servers[idx] = server;
            } else {
                info!("Imported MCP server '{}' from {}", server.name, tool.name);
                s.servers.push(server);
            }
            imported += 1;
        }

        // Mark this tool as managed
        if !s.enabled_integrations.contains(&id) {
            s.enabled_integrations.push(id.clone());
        }

        save_servers(&app, &s.servers);
        save_enabled_integrations(&app, &s.enabled_integrations);

        imported
    }; // lock dropped here

    if imported_count > 0 {
        info!("Imported {imported_count} MCP server(s) from {}", tool.name);
        crate::tray::rebuild_tray_menu(&app);
    }

    // Write proxy entries for all currently connected servers
    write_managed_config(&app, &tool.config_path, port, &tool.id, &tool.config_format)?;

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
            existing_servers: Vec::new(),
        });
    }

    // Remove our proxy entries from the config file
    remove_managed_entries(&tool.config_path, &tool.config_format)?;

    info!("Disabled MCP Manager integration for {}", tool.name);

    let (_, _, existing_servers) = parse_config(&tool.config_path, &tool.config_format);

    Ok(AiToolInfo {
        id: tool.id,
        name: tool.name,
        installed: true,
        enabled: false,
        config_path: tool.config_path.display().to_string(),
        configured_port: 0,
        existing_servers,
    })
}

// ---------------------------------------------------------------------------
// Format-aware config writers — write proxy entries for connected servers
// ---------------------------------------------------------------------------

/// Write proxy entries for all connected servers to a tool's config file.
fn write_managed_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
    format: &ConfigFormat,
) -> Result<(), AppError> {
    match format {
        ConfigFormat::McpServers => write_mcp_servers_config(app, path, port, tool_id),
        ConfigFormat::OpenCode => write_opencode_config(app, path, port, tool_id),
        ConfigFormat::Zed => write_zed_config(app, path, port, tool_id),
        ConfigFormat::CodexToml => write_codex_config(app, path, port, tool_id),
    }
}

fn write_mcp_servers_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
) -> Result<(), AppError> {
    let entries = connected_proxy_urls(app, port, tool_id);

    let mut mcp_servers = serde_json::Map::new();
    for (name, url) in entries {
        mcp_servers.insert(name, serde_json::json!({ "type": "http", "url": url }));
    }

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

fn write_opencode_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
) -> Result<(), AppError> {
    let entries = connected_proxy_urls(app, port, tool_id);

    let mut mcp = serde_json::Map::new();
    for (name, url) in entries {
        mcp.insert(
            name,
            serde_json::json!({
                "type": "remote",
                "url": url
            }),
        );
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["mcp"] = serde_json::Value::Object(mcp);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;

    Ok(())
}

fn write_zed_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
) -> Result<(), AppError> {
    let entries = connected_proxy_urls(app, port, tool_id);

    let mut context_servers = serde_json::Map::new();
    for (name, url) in entries {
        context_servers.insert(name, serde_json::json!({ "url": url }));
    }

    // Strip comments for parsing, but we'll write clean JSON back
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let stripped = strip_json_comments(&content);
        serde_json::from_str::<serde_json::Value>(&stripped).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["context_servers"] = serde_json::Value::Object(context_servers);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;

    Ok(())
}

fn write_codex_config(
    app: &AppHandle,
    path: &Path,
    port: u16,
    tool_id: &str,
) -> Result<(), AppError> {
    let entries = connected_proxy_urls(app, port, tool_id);

    let mut mcp_servers = toml::map::Map::new();
    for (name, url) in entries {
        let mut entry = toml::map::Map::new();
        entry.insert("url".into(), toml::Value::String(url));
        mcp_servers.insert(name, toml::Value::Table(entry));
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        content
            .parse::<toml::Value>()
            .unwrap_or(toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    if let Some(table) = config.as_table_mut() {
        table.insert("mcp_servers".into(), toml::Value::Table(mcp_servers));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(&config)
        .map_err(|e| AppError::Protocol(format!("Failed to serialize TOML: {e}")))?;
    std::fs::write(path, content)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Format-aware config removers — remove proxy entries on disable
// ---------------------------------------------------------------------------

/// Remove all proxy entries from a tool's config file.
fn remove_managed_entries(path: &Path, format: &ConfigFormat) -> Result<(), AppError> {
    match format {
        ConfigFormat::McpServers => remove_mcp_servers_entries(path),
        ConfigFormat::OpenCode => remove_opencode_entries(path),
        ConfigFormat::Zed => remove_zed_entries(path),
        ConfigFormat::CodexToml => remove_codex_entries(path),
    }
}

fn remove_mcp_servers_entries(path: &Path) -> Result<(), AppError> {
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

fn remove_opencode_entries(path: &Path) -> Result<(), AppError> {
    let content = std::fs::read_to_string(path)?;
    let mut config: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        let proxy_keys: Vec<String> = mcp
            .iter()
            .filter(|(_, v)| {
                v.get("url")
                    .and_then(|u| u.as_str())
                    .map(is_proxy_url)
                    .unwrap_or(false)
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in proxy_keys {
            mcp.remove(&key);
        }
    }

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;

    Ok(())
}

fn remove_zed_entries(path: &Path) -> Result<(), AppError> {
    let content = std::fs::read_to_string(path)?;
    let stripped = strip_json_comments(&content);
    let mut config: serde_json::Value = serde_json::from_str(&stripped)?;

    if let Some(servers) = config
        .get_mut("context_servers")
        .and_then(|v| v.as_object_mut())
    {
        let proxy_keys: Vec<String> = servers
            .iter()
            .filter(|(_, v)| {
                v.get("url")
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

fn remove_codex_entries(path: &Path) -> Result<(), AppError> {
    let content = std::fs::read_to_string(path)?;
    let mut config: toml::Value = content
        .parse()
        .map_err(|e| AppError::Protocol(format!("Invalid TOML: {e}")))?;

    if let Some(table) = config.as_table_mut() {
        if let Some(servers) = table.get_mut("mcp_servers").and_then(|v| v.as_table_mut()) {
            let proxy_keys: Vec<String> = servers
                .iter()
                .filter(|(_, v)| {
                    v.get("url")
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
    }

    let content = toml::to_string_pretty(&config)
        .map_err(|e| AppError::Protocol(format!("Failed to serialize TOML: {e}")))?;
    std::fs::write(path, content)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Format-aware native config writers — write original server configs on exit
// ---------------------------------------------------------------------------

/// Write original (non-proxy) server configs to a tool's config file.
/// This is the inverse of `write_managed_config`: it replaces proxy entries
/// with the actual server configurations so they work without MCP Manager.
fn write_native_config(
    servers: &[ServerConfig],
    path: &Path,
    format: &ConfigFormat,
) -> Result<(), AppError> {
    match format {
        ConfigFormat::McpServers => write_native_mcp_servers(servers, path),
        ConfigFormat::OpenCode => write_native_opencode(servers, path),
        ConfigFormat::Zed => write_native_zed(servers, path),
        ConfigFormat::CodexToml => write_native_codex(servers, path),
    }
}

fn write_native_mcp_servers(servers: &[ServerConfig], path: &Path) -> Result<(), AppError> {
    let mut mcp_servers = serde_json::Map::new();
    for srv in servers {
        let entry = match srv.transport {
            ServerTransport::Stdio => {
                let mut obj = serde_json::Map::new();
                if let Some(cmd) = &srv.command {
                    obj.insert("command".into(), serde_json::Value::String(cmd.clone()));
                }
                if let Some(args) = &srv.args {
                    obj.insert("args".into(), serde_json::json!(args));
                }
                if let Some(env) = &srv.env {
                    if !env.is_empty() {
                        obj.insert("env".into(), serde_json::json!(env));
                    }
                }
                serde_json::Value::Object(obj)
            }
            ServerTransport::Http => {
                let mut obj = serde_json::Map::new();
                obj.insert("type".into(), serde_json::Value::String("http".into()));
                if let Some(url) = &srv.url {
                    obj.insert("url".into(), serde_json::Value::String(url.clone()));
                }
                if let Some(headers) = &srv.headers {
                    if !headers.is_empty() {
                        obj.insert("headers".into(), serde_json::json!(headers));
                    }
                }
                serde_json::Value::Object(obj)
            }
        };
        mcp_servers.insert(srv.name.clone(), entry);
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["mcpServers"] = serde_json::Value::Object(mcp_servers);

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn write_native_opencode(servers: &[ServerConfig], path: &Path) -> Result<(), AppError> {
    let mut mcp = serde_json::Map::new();
    for srv in servers {
        let entry = match srv.transport {
            ServerTransport::Stdio => {
                let mut cmd_arr: Vec<String> = Vec::new();
                if let Some(cmd) = &srv.command {
                    cmd_arr.push(cmd.clone());
                }
                if let Some(args) = &srv.args {
                    cmd_arr.extend(args.iter().cloned());
                }
                let mut obj = serde_json::Map::new();
                obj.insert("type".into(), serde_json::Value::String("local".into()));
                obj.insert("command".into(), serde_json::json!(cmd_arr));
                if let Some(env) = &srv.env {
                    if !env.is_empty() {
                        obj.insert("environment".into(), serde_json::json!(env));
                    }
                }
                serde_json::Value::Object(obj)
            }
            ServerTransport::Http => {
                let mut obj = serde_json::Map::new();
                obj.insert("type".into(), serde_json::Value::String("remote".into()));
                if let Some(url) = &srv.url {
                    obj.insert("url".into(), serde_json::Value::String(url.clone()));
                }
                serde_json::Value::Object(obj)
            }
        };
        mcp.insert(srv.name.clone(), entry);
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["mcp"] = serde_json::Value::Object(mcp);

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn write_native_zed(servers: &[ServerConfig], path: &Path) -> Result<(), AppError> {
    let mut context_servers = serde_json::Map::new();
    for srv in servers {
        let entry = match srv.transport {
            ServerTransport::Stdio => {
                let mut obj = serde_json::Map::new();
                if let Some(cmd) = &srv.command {
                    obj.insert("command".into(), serde_json::Value::String(cmd.clone()));
                }
                if let Some(args) = &srv.args {
                    obj.insert("args".into(), serde_json::json!(args));
                }
                if let Some(env) = &srv.env {
                    if !env.is_empty() {
                        obj.insert("env".into(), serde_json::json!(env));
                    }
                }
                serde_json::Value::Object(obj)
            }
            ServerTransport::Http => {
                let mut obj = serde_json::Map::new();
                if let Some(url) = &srv.url {
                    obj.insert("url".into(), serde_json::Value::String(url.clone()));
                }
                serde_json::Value::Object(obj)
            }
        };
        context_servers.insert(srv.name.clone(), entry);
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let stripped = strip_json_comments(&content);
        serde_json::from_str::<serde_json::Value>(&stripped).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["context_servers"] = serde_json::Value::Object(context_servers);

    let content = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, content)?;
    Ok(())
}

fn write_native_codex(servers: &[ServerConfig], path: &Path) -> Result<(), AppError> {
    let mut mcp_servers = toml::map::Map::new();
    for srv in servers {
        let mut entry = toml::map::Map::new();
        match srv.transport {
            ServerTransport::Stdio => {
                if let Some(cmd) = &srv.command {
                    entry.insert("command".into(), toml::Value::String(cmd.clone()));
                }
                if let Some(args) = &srv.args {
                    let arr: Vec<toml::Value> = args
                        .iter()
                        .map(|a| toml::Value::String(a.clone()))
                        .collect();
                    entry.insert("args".into(), toml::Value::Array(arr));
                }
                if let Some(env) = &srv.env {
                    if !env.is_empty() {
                        let env_table: toml::map::Map<String, toml::Value> = env
                            .iter()
                            .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
                            .collect();
                        entry.insert("env".into(), toml::Value::Table(env_table));
                    }
                }
            }
            ServerTransport::Http => {
                if let Some(url) = &srv.url {
                    entry.insert("url".into(), toml::Value::String(url.clone()));
                }
            }
        }
        mcp_servers.insert(srv.name.clone(), toml::Value::Table(entry));
    }

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        content
            .parse::<toml::Value>()
            .unwrap_or(toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    if let Some(table) = config.as_table_mut() {
        table.insert("mcp_servers".into(), toml::Value::Table(mcp_servers));
    }

    let content = toml::to_string_pretty(&config)
        .map_err(|e| AppError::Protocol(format!("Failed to serialize TOML: {e}")))?;
    std::fs::write(path, content)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sync enabled configs with current connected servers
// ---------------------------------------------------------------------------

/// Restore all enabled integration configs to native (non-proxy) server entries.
/// Called on app exit so configs work without MCP Manager running.
pub fn restore_all_integration_configs(app: &AppHandle) -> Result<(), AppError> {
    let home = home_dir()?;
    let tools = get_tool_definitions(&home);

    let (enabled_ids, servers) = {
        let state = app.state::<SharedState>();
        let s = state.lock().unwrap();
        (s.enabled_integrations.clone(), s.servers.clone())
    };

    for tool in tools {
        if !enabled_ids.contains(&tool.id) || !tool.config_path.exists() {
            continue;
        }

        if let Err(e) = write_native_config(&servers, &tool.config_path, &tool.config_format) {
            warn!("Failed to restore native config for {}: {e}", tool.name);
        } else {
            info!("Restored native config for {}", tool.name);
        }
    }

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
        if !enabled_ids.contains(&tool.id) || !tool.config_path.exists() {
            continue;
        }

        if let Err(e) =
            write_managed_config(app, &tool.config_path, port, &tool.id, &tool.config_format)
        {
            warn!("Failed to update config for {}: {e}", tool.name);
        } else {
            info!("Updated {} config with per-server proxy entries", tool.name);
        }
    }

    Ok(())
}
