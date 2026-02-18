use std::collections::HashMap;

use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info};

use crate::error::AppError;
use crate::mcp::client::{McpClient, SharedConnections};
use crate::mcp::oauth;
use crate::mcp::proxy::ProxyState;
use crate::state::{
    ConnectionState, McpTool, ServerStatus, ServerTransport, SharedOAuthStore, SharedState,
};

#[tauri::command]
pub async fn connect_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    connections: State<'_, SharedConnections>,
    oauth_store: State<'_, SharedOAuthStore>,
    id: String,
) -> Result<(), AppError> {
    // Read config while holding the lock briefly
    let server_config = {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;

        if server.status == Some(ServerStatus::Connected)
            || server.status == Some(ServerStatus::Connecting)
        {
            return Err(AppError::AlreadyConnected(id.clone()));
        }

        server.status = Some(ServerStatus::Connecting);

        ServerConnectConfig {
            transport: server.transport.clone(),
            command: server.command.clone(),
            args: server.args.clone().unwrap_or_default(),
            env: server.env.clone().unwrap_or_default(),
            url: server.url.clone(),
            headers: server.headers.clone().unwrap_or_default(),
        }
    };

    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": id, "status": "connecting" }),
    );

    // For HTTP transport, check if we have existing OAuth tokens
    let access_token = if matches!(server_config.transport, ServerTransport::Http) {
        resolve_access_token(&oauth_store, &id).await
    } else {
        None
    };

    // Do the async connection work WITHOUT holding either lock
    let client_result = match server_config.transport {
        ServerTransport::Stdio => {
            let command = server_config
                .command
                .ok_or_else(|| AppError::ConnectionFailed("No command specified".into()))?;
            McpClient::connect_stdio(&app, &id, &command, &server_config.args, &server_config.env)
                .await
        }
        ServerTransport::Http => {
            let url = server_config
                .url
                .ok_or_else(|| AppError::ConnectionFailed("No URL specified".into()))?;
            emit_server_log(&app, &id, "info", &format!("Connecting to {url}"));
            match McpClient::connect_http(&url, server_config.headers, access_token).await {
                Ok(client) => {
                    emit_server_log(
                        &app,
                        &id,
                        "info",
                        &format!("Connected — {} tools available", client.tools.len()),
                    );
                    Ok(client)
                }
                Err(e) => {
                    emit_server_log(&app, &id, "error", &format!("Connection failed: {e}"));
                    Err(e)
                }
            }
        }
    };

    match client_result {
        Ok(client) => {
            finalize_connection(&app, &state, &connections, &id, client).await?;
            Ok(())
        }
        Err(AppError::AuthRequired(_)) => {
            info!("Server {id} requires OAuth authentication");
            mark_server_error(
                &app,
                &state,
                &id,
                "Authentication required. Click Authorize to sign in.",
            );
            let _ = app.emit("oauth-required", serde_json::json!({ "serverId": id }));
            Err(AppError::AuthRequired(
                "Authentication required. Click Authorize to sign in.".into(),
            ))
        }
        Err(e) => {
            error!("Failed to connect to server {id}: {e}");
            let error_message = e.to_string();
            mark_server_error(&app, &state, &id, &error_message);
            let _ = app.emit(
                "server-error",
                serde_json::json!({
                    "serverId": id,
                    "error": error_message,
                    "details": format!("Connection to server {id} failed: {error_message}")
                }),
            );
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn disconnect_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    connections: State<'_, SharedConnections>,
    id: String,
) -> Result<(), AppError> {
    // Remove and shut down the live MCP client
    {
        let mut conns = connections.lock().await;
        if let Some(client) = conns.remove(&id) {
            client.shutdown();
        }
    }

    // Update AppState
    {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;
        server.status = Some(ServerStatus::Disconnected);
        s.connections.remove(&id);
    }

    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": id, "status": "disconnected" }),
    );

    crate::tray::rebuild_tray_menu(&app);

    // Update integration configs so AI tools no longer see this server
    let proxy_state = app.state::<ProxyState>();
    let port = proxy_state.port().await;
    if let Err(e) = crate::commands::integrations::update_all_integration_configs(&app, port) {
        tracing::warn!("Failed to update integration configs after disconnect: {e}");
    }

    emit_server_log(&app, &id, "info", "Disconnected");
    info!("Disconnected server {id}");

    Ok(())
}

/// Reconnect servers that were previously connected (called on app startup).
/// Resets all statuses to Disconnected first, then attempts to reconnect each.
pub async fn reconnect_on_startup(app: AppHandle) {
    let servers_to_reconnect: Vec<(String, ServerConnectConfig)> = {
        let state = app.state::<SharedState>();
        let mut s = state.lock().unwrap();

        let mut to_reconnect = Vec::new();
        for server in &mut s.servers {
            if server.status == Some(ServerStatus::Connected)
                || server.status == Some(ServerStatus::Connecting)
            {
                to_reconnect.push((
                    server.id.clone(),
                    ServerConnectConfig {
                        transport: server.transport.clone(),
                        command: server.command.clone(),
                        args: server.args.clone().unwrap_or_default(),
                        env: server.env.clone().unwrap_or_default(),
                        url: server.url.clone(),
                        headers: server.headers.clone().unwrap_or_default(),
                    },
                ));
            }
            // Reset all to disconnected — real status comes from actual connections
            server.status = Some(ServerStatus::Disconnected);
        }
        to_reconnect
    };

    if servers_to_reconnect.is_empty() {
        return;
    }

    info!(
        "Auto-reconnecting {} server(s) from previous session",
        servers_to_reconnect.len()
    );

    // Wait for the proxy to be ready
    let proxy_state = app.state::<ProxyState>();
    for _ in 0..50 {
        if proxy_state.is_running().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let state = app.state::<SharedState>();
    let connections = app.state::<SharedConnections>();
    let oauth_store = app.state::<SharedOAuthStore>();

    for (id, config) in servers_to_reconnect {
        // Skip if already connected/connecting (frontend's autoConnectServers may have raced us)
        {
            let mut s = state.lock().unwrap();
            if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
                if server.status == Some(ServerStatus::Connected)
                    || server.status == Some(ServerStatus::Connecting)
                {
                    info!("Server {id} already connecting/connected, skipping reconnect");
                    continue;
                }
                server.status = Some(ServerStatus::Connecting);
            }
        }
        let _ = app.emit(
            "server-status-changed",
            serde_json::json!({ "serverId": id, "status": "connecting" }),
        );

        let access_token = if matches!(config.transport, ServerTransport::Http) {
            resolve_access_token(&oauth_store, &id).await
        } else {
            None
        };

        let client_result = match config.transport {
            ServerTransport::Stdio => {
                let Some(command) = config.command else {
                    error!("Server {id} has no command, skipping reconnect");
                    continue;
                };
                McpClient::connect_stdio(&app, &id, &command, &config.args, &config.env).await
            }
            ServerTransport::Http => {
                let Some(url) = config.url else {
                    error!("Server {id} has no URL, skipping reconnect");
                    continue;
                };
                emit_server_log(&app, &id, "info", &format!("Connecting to {url}"));
                match McpClient::connect_http(&url, config.headers, access_token).await {
                    Ok(client) => {
                        emit_server_log(
                            &app,
                            &id,
                            "info",
                            &format!("Connected — {} tools available", client.tools.len()),
                        );
                        Ok(client)
                    }
                    Err(e) => {
                        emit_server_log(&app, &id, "error", &format!("Connection failed: {e}"));
                        Err(e)
                    }
                }
            }
        };

        match client_result {
            Ok(client) => {
                if let Err(e) = finalize_connection(&app, &state, &connections, &id, client).await {
                    error!("Failed to finalize reconnection for {id}: {e}");
                }
            }
            Err(e) => {
                error!("Failed to reconnect server {id}: {e}");
                mark_server_error(&app, &state, &id, &e.to_string());
            }
        }
    }
}

// --- Private helpers ---

/// Temporary struct to hold server config data extracted from the lock.
struct ServerConnectConfig {
    transport: ServerTransport,
    command: Option<String>,
    args: Vec<String>,
    env: HashMap<String, String>,
    url: Option<String>,
    headers: HashMap<String, String>,
}

/// Try to get a valid access token from stored OAuth state, refreshing if needed.
async fn resolve_access_token(oauth_store: &SharedOAuthStore, id: &str) -> Option<String> {
    let store = oauth_store.lock().await;
    let oauth_state = store.get(id)?;
    let tokens = oauth_state.tokens.as_ref()?;

    if !oauth::is_token_expired(tokens) {
        return Some(tokens.access_token.clone());
    }

    if tokens.refresh_token.is_some() {
        drop(store);
        match oauth::try_refresh_token(oauth_store, id).await {
            Ok(new_token) => Some(new_token),
            Err(e) => {
                tracing::warn!("Token refresh failed: {e}, will try without token");
                None
            }
        }
    } else {
        None
    }
}

/// Mark a server as errored: update state, emit events, rebuild tray.
fn mark_server_error(app: &AppHandle, state: &SharedState, id: &str, error: &str) {
    {
        let mut s = state.lock().unwrap();
        if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
            server.status = Some(ServerStatus::Error);
        }
    }
    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": id, "status": "error", "error": error }),
    );
    crate::tray::rebuild_tray_menu(app);
}

/// Finalize a successful connection: store tools, update state, emit events, sync integrations.
async fn finalize_connection(
    app: &AppHandle,
    state: &SharedState,
    connections: &SharedConnections,
    id: &str,
    client: McpClient,
) -> Result<(), AppError> {
    let server_name;

    // Convert discovered tools to McpTool for storage in AppState
    let tools: Vec<McpTool> = {
        let s = state.lock().unwrap();
        let srv = s.servers.iter().find(|s| s.id == id);
        server_name = srv.map(|s| s.name.clone()).unwrap_or_default();

        client
            .tools
            .iter()
            .map(|t| McpTool {
                name: t.name.clone(),
                title: t.title.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
                server_id: id.to_string(),
                server_name: server_name.clone(),
            })
            .collect()
    };

    info!("Connected to server {id} with {} tools", tools.len());

    // Store connection state in AppState
    {
        let mut s = state.lock().unwrap();
        if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
            server.status = Some(ServerStatus::Connected);
            server.last_connected = Some(chrono_now());
        }
        s.connections.insert(
            id.to_string(),
            ConnectionState {
                tools: tools.clone(),
            },
        );
    }

    // Store the live client in the connections map
    {
        let mut conns = connections.lock().await;
        conns.insert(id.to_string(), client);
    }

    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": id, "status": "connected" }),
    );
    let _ = app.emit(
        "tools-updated",
        serde_json::json!({ "serverId": id, "tools": tools }),
    );

    crate::tray::rebuild_tray_menu(app);

    // Update integration configs so AI tools see this server
    let proxy_state = app.state::<ProxyState>();
    let port = proxy_state.port().await;
    if let Err(e) = crate::commands::integrations::update_all_integration_configs(app, port) {
        tracing::warn!("Failed to update integration configs after connect: {e}");
    }

    Ok(())
}

/// Emit a `server-log` event and buffer it in AppState for the frontend to drain later.
/// HTTP servers only get logs during connection, so if the frontend isn't mounted yet
/// the events are lost. The buffer ensures they can be retrieved after mount.
fn emit_server_log(app: &AppHandle, server_id: &str, level: &str, message: &str) {
    let _ = app.emit(
        "server-log",
        serde_json::json!({
            "serverId": server_id,
            "level": level,
            "message": message,
        }),
    );

    // Also buffer for frontend that may not be listening yet
    let state = app.state::<SharedState>();
    let mut s = state.lock().unwrap();
    s.log_buffer.push(crate::state::BufferedLog {
        server_id: server_id.to_string(),
        level: level.to_string(),
        message: message.to_string(),
    });
}

/// Drain buffered logs — called by the frontend after it sets up the event listener.
#[tauri::command]
pub async fn drain_log_buffer(
    state: State<'_, SharedState>,
) -> Result<Vec<crate::state::BufferedLog>, AppError> {
    let mut s = state.lock().unwrap();
    Ok(std::mem::take(&mut s.log_buffer))
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();
    format!("{now}")
}
