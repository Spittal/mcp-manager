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

/// Internal definition for a supported AI tool.
struct ToolDef {
    id: String,
    name: String,
    config_path: PathBuf,
    detection_paths: Vec<PathBuf>,
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
    /// Existing MCP servers in this tool's config that could be migrated.
    pub existing_servers: Vec<ExistingMcpServer>,
}

fn get_tool_definitions(home: &Path) -> Vec<ToolDef> {
    let mut tools = vec![
        ToolDef {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            config_path: home.join(".claude").join("mcp.json"),
            detection_paths: vec![home.join(".claude")],
        },
        ToolDef {
            id: "cursor".into(),
            name: "Cursor".into(),
            config_path: home.join(".cursor").join("mcp.json"),
            detection_paths: vec![
                home.join(".cursor"),
                PathBuf::from("/Applications/Cursor.app"),
            ],
        },
        ToolDef {
            id: "claude-desktop".into(),
            name: "Claude Desktop".into(),
            config_path: home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            detection_paths: vec![PathBuf::from("/Applications/Claude.app")],
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

/// Parse a tool's config file and return existing non-proxy MCP servers.
fn parse_existing_servers(path: &Path) -> Vec<ExistingMcpServer> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let config: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let servers_obj = match config.get("mcpServers").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return Vec::new(),
    };

    let mut existing = Vec::new();

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

    existing
}

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))
    })
}

#[tauri::command]
pub async fn detect_integrations(
    state: State<'_, SharedState>,
    proxy_state: State<'_, ProxyState>,
) -> Result<Vec<AiToolInfo>, AppError> {
    let home = home_dir()?;
    let tools = get_tool_definitions(&home);
    let port = proxy_state.port().await;

    let enabled_ids: Vec<String> = {
        let s = state.lock().unwrap();
        s.enabled_integrations.clone()
    };

    let mut results = Vec::new();
    for tool in tools {
        let installed = tool.detection_paths.iter().any(|p| p.exists());
        let enabled = enabled_ids.contains(&tool.id);

        // Only show existing servers if the integration is not yet enabled
        let existing_servers = if installed && !enabled {
            parse_existing_servers(&tool.config_path)
        } else {
            Vec::new()
        };

        results.push(AiToolInfo {
            id: tool.id,
            name: tool.name,
            installed,
            enabled,
            config_path: tool.config_path.display().to_string(),
            configured_port: if enabled { port } else { 0 },
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

            let has_url = value.get("url").and_then(|v| v.as_str()).is_some();

            let server = ServerConfig {
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
                headers: value.get("headers").and_then(|v| v.as_object()).map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect::<HashMap<String, String>>()
                }),
                tags: None,
                status: Some(ServerStatus::Disconnected),
                last_connected: None,
                managed: None,
            };

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

    // Remove our proxy entries from the config file
    if tool.config_path.exists() {
        remove_proxy_entries(&tool.config_path)?;
    }

    info!("Disabled MCP Manager integration for {}", tool.name);

    let existing_servers = parse_existing_servers(&tool.config_path);

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
        if !enabled_ids.contains(&tool.id) {
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
