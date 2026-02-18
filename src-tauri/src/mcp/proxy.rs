use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::{error, info};

use crate::mcp::client::SharedConnections;
use crate::persistence::save_stats;
use crate::state::SharedState;
use crate::stats::{unix_now, StatsStore, ToolCallEntry, ToolStats};

/// Shared proxy state tracking whether the server is running and on which port.
#[derive(Clone)]
pub struct ProxyState {
    inner: Arc<RwLock<ProxyStateInner>>,
}

struct ProxyStateInner {
    running: bool,
    port: u16,
}

impl ProxyState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ProxyStateInner {
                running: false,
                port: 0,
            })),
        }
    }

    pub async fn set_running(&self, port: u16) {
        let mut inner = self.inner.write().await;
        inner.running = true;
        inner.port = port;
    }

    pub async fn is_running(&self) -> bool {
        self.inner.read().await.running
    }

    pub async fn port(&self) -> u16 {
        self.inner.read().await.port
    }
}

/// Shared state passed into axum handlers.
#[derive(Clone)]
pub(crate) struct ProxyAppState {
    pub(crate) app_handle: AppHandle,
}

/// Start the MCP proxy HTTP server on a random available port.
pub async fn start_proxy(
    app_handle: AppHandle,
    proxy_state: ProxyState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = ProxyAppState {
        app_handle: app_handle.clone(),
    };

    let app = Router::new()
        .route(
            "/mcp/discovery",
            post(super::discovery::handle_discovery_post).get(handle_mcp_get),
        )
        .route(
            "/mcp/{server_id}",
            post(handle_mcp_post).get(handle_mcp_get),
        )
        .with_state(state);

    // Bind to localhost with port 0 to get a random available port
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let port = addr.port();

    proxy_state.set_running(port).await;

    // Update all enabled AI tool integration configs with the new port
    if let Err(e) =
        crate::commands::integrations::update_all_integration_configs(&app_handle, port)
    {
        tracing::warn!("Failed to update integration configs on startup: {e}");
    }

    info!("MCP proxy server listening on http://127.0.0.1:{port}/mcp/{{server_id}}");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Handle GET requests — spec says server MUST return SSE stream or 405.
/// We don't support server-initiated streaming, so return 405.
async fn handle_mcp_get() -> impl IntoResponse {
    StatusCode::METHOD_NOT_ALLOWED
}

/// Handle POST requests — per-server JSON-RPC handler.
async fn handle_mcp_post(
    AxumState(state): AxumState<ProxyAppState>,
    Path(server_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let method = body
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    let id = body.get("id").cloned();
    let params = body.get("params").cloned();
    let client = query.get("client").cloned().unwrap_or_default();

    // Per spec: if the message has no "id", it's a notification or response.
    // Notifications must get 202 Accepted with no body.
    let is_notification = id.is_none();

    if is_notification {
        return (StatusCode::ACCEPTED, HeaderMap::new(), String::new());
    }

    // Look up the server by ID
    let server_name = {
        let app_state = state.app_handle.state::<SharedState>();
        let s = app_state.lock().unwrap();
        s.servers
            .iter()
            .find(|srv| srv.id == server_id)
            .map(|srv| srv.name.clone())
    };

    let server_name = match server_name {
        Some(name) => name,
        None => {
            let resp = make_error_response(
                id,
                -32602,
                &format!("No server found with ID: {server_id}"),
            );
            let body = serde_json::to_string(&resp).unwrap_or_default();
            let mut headers = HeaderMap::new();
            headers.insert("content-type", "application/json".parse().unwrap());
            return (StatusCode::OK, headers, body);
        }
    };

    info!("Proxy [{server_name}] {method}");

    let response = match method {
        "initialize" => handle_initialize(id, &server_name),
        "tools/list" => handle_tools_list(id, &server_id, &state),
        "tools/call" => {
            handle_tools_call(id, params, &server_id, &server_name, &client, &state).await
        }
        _ => make_error_response(id, -32601, &format!("Method not found: {method}")),
    };

    let body = serde_json::to_string(&response).unwrap_or_default();
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    (StatusCode::OK, headers, body)
}

/// Handle the `initialize` request -- return server info and capabilities.
fn handle_initialize(id: Option<Value>, server_name: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": format!("MCP Manager — {server_name}"),
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

/// Handle `tools/list` -- return tools for this specific server only.
fn handle_tools_list(id: Option<Value>, server_id: &str, state: &ProxyAppState) -> Value {
    let tools = collect_server_tools(server_id, state);

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    })
}

/// Handle `tools/call` -- route directly to this server's backend.
async fn handle_tools_call(
    id: Option<Value>,
    params: Option<Value>,
    server_id: &str,
    server_name: &str,
    client_id: &str,
    state: &ProxyAppState,
) -> Value {
    let params = match params {
        Some(p) => p,
        None => {
            return make_error_response(id, -32602, "Missing params for tools/call");
        }
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return make_error_response(id, -32602, "Missing tool name in params");
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    // Clone an Arc handle and drop the lock before doing async I/O.
    // This avoids blocking all other proxy requests while a tool call is in flight.
    let connections = state.app_handle.state::<SharedConnections>();
    let client = {
        let conns = connections.lock().await;
        match conns.get(server_id).cloned() {
            Some(c) => c,
            None => {
                return make_error_response(
                    id,
                    -32602,
                    &format!("Server '{server_name}' is not connected"),
                );
            }
        }
    };

    info!("Proxy tool call: {server_name}.{tool_name}");

    let start = Instant::now();
    let call_result = client.call_tool(&tool_name, arguments).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let (response, is_error) = match call_result {
        Ok(result) => {
            let is_err = result.is_error.unwrap_or(false);
            if is_err {
                info!("Proxy tool result: {server_name}.{tool_name} -> error");
            } else {
                info!("Proxy tool result: {server_name}.{tool_name} -> ok");
            }
            let result_value = match serde_json::to_value(&result) {
                Ok(v) => v,
                Err(e) => {
                    return make_error_response(
                        id,
                        -32603,
                        &format!("Failed to serialize tool result: {e}"),
                    );
                }
            };
            (
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result_value
                }),
                is_err,
            )
        }
        Err(e) => {
            error!("Proxy tool call failed: {server_name}.{tool_name} -> {e}");
            (
                make_error_response(id, -32603, &format!("Tool call failed: {e}")),
                true,
            )
        }
    };

    // Record stats
    record_tool_stats(
        &state.app_handle,
        server_id,
        &tool_name,
        client_id,
        duration_ms,
        is_error,
    )
    .await;

    response
}

/// Record a tool call in the stats store, persist periodically, and emit event.
pub(crate) async fn record_tool_stats(
    app: &AppHandle,
    server_id: &str,
    tool_name: &str,
    client_id: &str,
    duration_ms: u64,
    is_error: bool,
) {
    let stats_store = app.state::<StatsStore>();
    let mut store = stats_store.write().await;

    let server_stats = store.entry(server_id.to_string()).or_default();

    // Server-level aggregates
    server_stats.total_calls += 1;
    if is_error {
        server_stats.errors += 1;
    }
    server_stats.total_duration_ms += duration_ms;

    // Per-tool aggregates
    let tool_stats = server_stats
        .tools
        .entry(tool_name.to_string())
        .or_insert_with(ToolStats::default);
    tool_stats.total_calls += 1;
    if is_error {
        tool_stats.errors += 1;
    }
    tool_stats.total_duration_ms += duration_ms;

    // Per-client aggregates
    if !client_id.is_empty() {
        *server_stats
            .clients
            .entry(client_id.to_string())
            .or_insert(0) += 1;
    }

    // Recent call log (capped at MAX_RECENT_CALLS)
    server_stats.push_call(ToolCallEntry {
        tool: tool_name.to_string(),
        client: client_id.to_string(),
        duration_ms,
        is_error,
        timestamp: unix_now(),
    });

    // Persist every 10 calls
    let should_persist = server_stats.total_calls % 10 == 0;
    if should_persist {
        save_stats(app, &store);
    }

    drop(store);

    // Emit event so frontend can refresh
    let _ = app.emit(
        "tool-call-recorded",
        serde_json::json!({ "serverId": server_id }),
    );
}

/// Collect tools for a specific server (no namespacing — original tool names).
fn collect_server_tools(server_id: &str, state: &ProxyAppState) -> Vec<Value> {
    let app_state = state.app_handle.state::<SharedState>();
    let s = app_state.lock().unwrap();

    let conn_state = match s.connections.get(server_id) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut tools = Vec::new();
    for tool in &conn_state.tools {
        let mut entry = serde_json::json!({
            "name": tool.name,
            "inputSchema": tool.input_schema,
        });
        if let Some(ref desc) = tool.description {
            entry["description"] = serde_json::Value::String(desc.clone());
        }
        if let Some(ref title) = tool.title {
            entry["title"] = serde_json::Value::String(title.clone());
        }
        tools.push(entry);
    }
    tools
}

/// Build a JSON-RPC error response.
pub(crate) fn make_error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
