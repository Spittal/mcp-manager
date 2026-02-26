use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use axum::extract::{Path, Query, State as AxumState};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use futures::stream::Stream;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Instant;
use tracing::{error, info};

use crate::mcp::client::SharedConnections;
use crate::mcp::http_common::{
    accepted_response, client_accepts_sse, mcp_response, negotiate_version, new_session_id,
    validate_origin,
};
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

    /// Synchronous port access for use in non-async contexts (e.g. exit handler).
    pub fn port_blocking(&self) -> u16 {
        self.inner.blocking_read().port
    }
}

/// Wrapper for the broadcast sender so it can be managed as Tauri state.
#[derive(Clone)]
pub struct NotifySender(pub broadcast::Sender<String>);

/// Tracks a hash of the tool name list per endpoint.
/// Used to determine whether `notifications/tools/list_changed` should actually fire.
pub struct ToolListHashes(pub RwLock<HashMap<String, u64>>);

impl ToolListHashes {
    pub fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }
}

/// Compute a deterministic hash of sorted tool names for change detection.
pub fn hash_tool_names(tools: &[crate::state::McpTool]) -> u64 {
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    names.hash(&mut hasher);
    hasher.finish()
}

/// Check if the tool list for a server has changed, and notify SSE clients if so.
/// Call this after connect/disconnect updates the connection state.
pub async fn notify_if_tools_changed(
    app: &AppHandle,
    server_id: &str,
    new_tools: &[crate::state::McpTool],
) {
    let new_hash = hash_tool_names(new_tools);

    if let Some(hashes) = app.try_state::<ToolListHashes>() {
        let mut map = hashes.0.write().await;
        let old_hash = map.get(server_id).copied();

        if old_hash == Some(new_hash) {
            // Tool list hasn't actually changed — don't notify
            return;
        }

        map.insert(server_id.to_string(), new_hash);
    }

    // Tool list genuinely changed — notify SSE clients
    if let Some(sender) = app.try_state::<NotifySender>() {
        let _ = sender.0.send(server_id.to_string());
    }
}

/// Shared state passed into axum handlers.
#[derive(Clone)]
pub(crate) struct ProxyAppState {
    pub(crate) app_handle: AppHandle,
    /// Broadcast channel for tool list change notifications.
    pub(crate) notify_tx: broadcast::Sender<String>,
}

/// Start the MCP proxy HTTP server on a random available port.
pub async fn start_proxy(
    app_handle: AppHandle,
    proxy_state: ProxyState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (notify_tx, _) = broadcast::channel::<String>(64);

    // Manage the sender and hash tracker as Tauri state so connections.rs can push notifications
    app_handle.manage(NotifySender(notify_tx.clone()));
    app_handle.manage(ToolListHashes::new());

    let state = ProxyAppState {
        app_handle: app_handle.clone(),
        notify_tx: notify_tx.clone(),
    };

    let app = Router::new()
        .route(
            "/mcp/discovery",
            post(super::discovery::handle_discovery_post),
        )
        .route(
            "/mcp/{server_id}",
            post(handle_mcp_post).get(handle_mcp_get),
        )
        .with_state(state);

    // Bind to a stable preferred port, falling back to OS-assigned if busy
    let listener = bind_preferred_port().await?;
    let addr = listener.local_addr()?;
    let port = addr.port();

    proxy_state.set_running(port).await;

    // Update all enabled AI tool integration configs with the new port
    if let Err(e) = crate::commands::integrations::update_all_integration_configs(&app_handle, port)
    {
        tracing::warn!("Failed to update integration configs on startup: {e}");
    }

    info!("MCP proxy server listening on http://127.0.0.1:{port}/mcp/{{server_id}}");

    axum::serve(listener, app).await?;

    Ok(())
}

const PORT_RANGE_START: u16 = 55_000;
const PORT_RANGE_SIZE: u16 = 10_000;
const PORT_ATTEMPTS: u16 = 20;

/// Derive a deterministic preferred port from the current username.
/// The same user always gets the same base port across restarts.
fn preferred_port() -> u16 {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    username.hash(&mut hasher);
    let hash = hasher.finish();
    PORT_RANGE_START + (hash % PORT_RANGE_SIZE as u64) as u16
}

/// Try to bind to the preferred port, then nearby ports, then fall back to OS-assigned.
async fn bind_preferred_port() -> Result<TcpListener, Box<dyn std::error::Error + Send + Sync>> {
    let base = preferred_port();
    for offset in 0..PORT_ATTEMPTS {
        let port = PORT_RANGE_START + ((base - PORT_RANGE_START + offset) % PORT_RANGE_SIZE);
        match TcpListener::bind(format!("127.0.0.1:{port}")).await {
            Ok(listener) => {
                if offset > 0 {
                    info!(
                        "Preferred proxy port {} was busy, using {}",
                        base, port
                    );
                }
                return Ok(listener);
            }
            Err(_) => continue,
        }
    }
    info!("All preferred ports busy, falling back to OS-assigned port");
    Ok(TcpListener::bind("127.0.0.1:0").await?)
}

/// Handle GET requests — open SSE stream for server-initiated notifications.
/// Per MCP spec, clients can open a GET to receive `notifications/tools/list_changed`.
async fn handle_mcp_get(
    AxumState(state): AxumState<ProxyAppState>,
    Path(server_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.notify_tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(changed_id) if changed_id == server_id => {
                    let notification = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/tools/list_changed"
                    });
                    yield Ok(Event::default().data(notification.to_string()));
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Ok(_) => continue, // different server_id, ignore
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Handle POST requests — per-server JSON-RPC handler.
async fn handle_mcp_post(
    AxumState(state): AxumState<ProxyAppState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Origin validation (MCP Streamable HTTP spec)
    if let Err((status, msg)) = validate_origin(&headers) {
        return (status, HeaderMap::new(), msg);
    }

    let method = body
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    let id = body.get("id").cloned();
    let params = body.get("params").cloned();
    let client = query.get("client").cloned().unwrap_or_default();

    let use_sse = client_accepts_sse(&headers);
    let req_session: Option<String> = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Per spec: if the message has no "id", it's a notification or response.
    // Notifications must get 202 Accepted with no body.
    if id.is_none() {
        return accepted_response(req_session.as_deref());
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
            let resp =
                make_error_response(id, -32602, &format!("No server found with ID: {server_id}"));
            return mcp_response(&resp, req_session.as_deref(), use_sse);
        }
    };

    info!("Proxy [{server_name}] {method}");

    match method {
        "initialize" => {
            // Negotiate protocol version from client's requested version
            let client_version = params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let negotiated = negotiate_version(client_version);

            // Generate a session ID for this connection
            let session_id = new_session_id();

            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": negotiated,
                    "capabilities": {
                        "tools": {
                            "listChanged": true
                        }
                    },
                    "serverInfo": {
                        "name": format!("MCP Manager — {server_name}"),
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            });
            mcp_response(&response, Some(&session_id), use_sse)
        }
        "tools/list" => {
            let response = handle_tools_list(id, &server_id, &state);
            mcp_response(&response, req_session.as_deref(), use_sse)
        }
        "tools/call" => {
            let response =
                handle_tools_call(id, params, &server_id, &server_name, &client, &state).await;
            mcp_response(&response, req_session.as_deref(), use_sse)
        }
        _ => {
            let response =
                make_error_response(id, -32601, &format!("Method not found: {method}"));
            mcp_response(&response, req_session.as_deref(), use_sse)
        }
    }
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
