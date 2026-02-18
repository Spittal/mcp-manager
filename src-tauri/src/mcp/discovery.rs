use std::collections::HashMap;

use axum::extract::{Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;
use tauri::Manager;
use tokio::time::Instant;
use tracing::{error, info};

use crate::mcp::client::SharedConnections;
use crate::mcp::proxy::{make_error_response, record_tool_stats, ProxyAppState};
use crate::state::SharedState;

/// Handle POST requests to `/mcp/discovery` — the single discovery endpoint.
pub(crate) async fn handle_discovery_post(
    AxumState(state): AxumState<ProxyAppState>,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Check if discovery mode is enabled
    {
        let app_state = state.app_handle.state::<SharedState>();
        let s = app_state.lock().unwrap();
        if !s.tool_discovery_enabled {
            let resp = make_error_response(
                body.get("id").cloned(),
                -32001,
                "Tool discovery mode is not enabled",
            );
            let body_str = serde_json::to_string(&resp).unwrap_or_default();
            let mut headers = HeaderMap::new();
            headers.insert("content-type", "application/json".parse().unwrap());
            return (StatusCode::OK, headers, body_str);
        }
    }

    let method = body
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default();
    let id = body.get("id").cloned();
    let params = body.get("params").cloned();
    let client_id = query.get("client").cloned().unwrap_or_default();

    // Notifications get 202 with no body
    if id.is_none() {
        return (StatusCode::ACCEPTED, HeaderMap::new(), String::new());
    }

    info!("Discovery endpoint: {method}");

    let response = match method {
        "initialize" => handle_initialize(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(id, params, &client_id, &state).await,
        _ => make_error_response(id, -32601, &format!("Method not found: {method}")),
    };

    let body_str = serde_json::to_string(&response).unwrap_or_default();
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    (StatusCode::OK, headers, body_str)
}

fn handle_initialize(id: Option<Value>) -> Value {
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
                "name": "MCP Manager — Tool Discovery",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list(id: Option<Value>) -> Value {
    let tools = vec![
        serde_json::json!({
            "name": "discover_tools",
            "description": "Search for available tools across all connected MCP servers. Returns matching tools with their full input schemas so you can call them immediately via call_tool. Use this before calling a tool you haven't used yet.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query — matches against tool names and descriptions. All terms must match (case-insensitive). Example: 'slack message' finds tools with both 'slack' and 'message' in their name or description."
                    }
                },
                "required": ["query"]
            }
        }),
        serde_json::json!({
            "name": "call_tool",
            "description": "Call a tool on a specific MCP server. Use discover_tools first to find the server_id and tool_name, then call this with the appropriate arguments.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "The server ID that hosts the tool (from discover_tools or list_servers results)."
                    },
                    "tool_name": {
                        "type": "string",
                        "description": "The name of the tool to call."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments to pass to the tool, matching its inputSchema."
                    }
                },
                "required": ["server_id", "tool_name"]
            }
        }),
        serde_json::json!({
            "name": "list_servers",
            "description": "List all connected MCP servers and their available tool names. Use this to get an overview of what's available, then use discover_tools for details on specific tools.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
    ];

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    })
}

async fn handle_tools_call(
    id: Option<Value>,
    params: Option<Value>,
    client_id: &str,
    state: &ProxyAppState,
) -> Value {
    let params = match params {
        Some(p) => p,
        None => return make_error_response(id, -32602, "Missing params for tools/call"),
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return make_error_response(id, -32602, "Missing tool name in params"),
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    match tool_name {
        "discover_tools" => handle_discover_tools(id, &arguments, state),
        "list_servers" => handle_list_servers(id, state),
        "call_tool" => handle_call_tool(id, &arguments, client_id, state).await,
        _ => make_error_response(
            id,
            -32602,
            &format!("Unknown discovery tool: {tool_name}. Available: discover_tools, call_tool, list_servers"),
        ),
    }
}

/// Build a human-readable parameter summary from an inputSchema.
/// e.g. "logql (string, required), limit (integer), start (string), end (string)"
fn summarize_params(schema: &Option<Value>) -> String {
    let schema = match schema {
        Some(s) => s,
        None => return String::new(),
    };

    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return String::new(),
    };

    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut parts = Vec::new();
    for (name, prop) in props {
        let typ = prop.get("type").and_then(|t| t.as_str()).unwrap_or("any");
        if required.contains(&name.as_str()) {
            parts.push(format!("{name} ({typ}, required)"));
        } else {
            parts.push(format!("{name} ({typ})"));
        }
    }
    parts.join(", ")
}

/// Search across all connected servers' tools by keyword.
fn handle_discover_tools(id: Option<Value>, arguments: &Value, state: &ProxyAppState) -> Value {
    let query = arguments
        .get("query")
        .and_then(|q| q.as_str())
        .unwrap_or("");

    if query.is_empty() {
        return make_error_response(id, -32602, "Missing required argument: query");
    }

    let terms: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();

    let app_state = state.app_handle.state::<SharedState>();
    let s = app_state.lock().unwrap();

    let mut matches = Vec::new();

    for srv in &s.servers {
        if srv.status != Some(crate::state::ServerStatus::Connected) {
            continue;
        }

        let conn = match s.connections.get(&srv.id) {
            Some(c) => c,
            None => continue,
        };

        for tool in &conn.tools {
            let name_lower = tool.name.to_lowercase();
            let desc_lower = tool
                .description
                .as_deref()
                .unwrap_or("")
                .to_lowercase();
            let haystack = format!("{name_lower} {desc_lower}");

            let all_match = terms.iter().all(|term| haystack.contains(term.as_str()));
            if !all_match {
                continue;
            }

            let param_summary = summarize_params(&tool.input_schema);
            let mut entry = serde_json::json!({
                "server_id": srv.id,
                "server_name": srv.name,
                "name": tool.name,
                "parameters": param_summary,
                "inputSchema": tool.input_schema,
            });
            if let Some(ref desc) = tool.description {
                entry["description"] = Value::String(desc.clone());
            }
            if let Some(ref title) = tool.title {
                entry["title"] = Value::String(title.clone());
            }
            matches.push(entry);

            if matches.len() >= 20 {
                break;
            }
        }

        if matches.len() >= 20 {
            break;
        }
    }

    let result_text = if matches.is_empty() {
        format!("No tools found matching '{query}'. Try broader terms or use list_servers to see available servers.")
    } else {
        serde_json::to_string_pretty(&matches).unwrap_or_default()
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": result_text
            }]
        }
    })
}

/// List all connected servers and their tool names.
fn handle_list_servers(id: Option<Value>, state: &ProxyAppState) -> Value {
    let app_state = state.app_handle.state::<SharedState>();
    let s = app_state.lock().unwrap();

    let mut servers = Vec::new();

    for srv in &s.servers {
        if srv.status != Some(crate::state::ServerStatus::Connected) {
            continue;
        }

        let tool_names: Vec<String> = s
            .connections
            .get(&srv.id)
            .map(|c| c.tools.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default();

        servers.push(serde_json::json!({
            "server_id": srv.id,
            "server_name": srv.name,
            "tool_count": tool_names.len(),
            "tools": tool_names,
        }));
    }

    let result_text = if servers.is_empty() {
        "No servers are currently connected.".to_string()
    } else {
        serde_json::to_string_pretty(&servers).unwrap_or_default()
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": result_text
            }]
        }
    })
}

/// Look up a tool's inputSchema from connection state.
fn lookup_tool_schema(state: &ProxyAppState, server_id: &str, tool_name: &str) -> Option<Value> {
    let app_state = state.app_handle.state::<SharedState>();
    let s = app_state.lock().unwrap();
    let conn = s.connections.get(server_id)?;
    let tool = conn.tools.iter().find(|t| t.name == tool_name)?;
    tool.input_schema.clone()
}

/// Build an error result that includes the tool's schema so the LLM can self-correct.
fn tool_error_with_schema(
    id: Option<Value>,
    error_text: &str,
    state: &ProxyAppState,
    server_id: &str,
    tool_name: &str,
) -> Value {
    let schema = lookup_tool_schema(state, server_id, tool_name);
    let hint = match &schema {
        Some(s) => format!(
            "\n\nExpected inputSchema for '{tool_name}':\n{}",
            serde_json::to_string_pretty(s).unwrap_or_default()
        ),
        None => String::new(),
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{
                "type": "text",
                "text": format!("{error_text}{hint}")
            }],
            "isError": true
        }
    })
}

/// Route a tool call to the actual MCP server.
async fn handle_call_tool(
    id: Option<Value>,
    arguments: &Value,
    client_id: &str,
    state: &ProxyAppState,
) -> Value {
    let server_id = match arguments.get("server_id").and_then(|s| s.as_str()) {
        Some(s) => s.to_string(),
        None => return make_error_response(id, -32602, "Missing required argument: server_id"),
    };

    let tool_name = match arguments.get("tool_name").and_then(|s| s.as_str()) {
        Some(s) => s.to_string(),
        None => return make_error_response(id, -32602, "Missing required argument: tool_name"),
    };

    let tool_arguments = arguments
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    // Look up server name
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
            return make_error_response(
                id,
                -32602,
                &format!("No server found with ID: {server_id}"),
            );
        }
    };

    // Get the MCP client
    let connections = state.app_handle.state::<SharedConnections>();
    let client = {
        let conns = connections.lock().await;
        match conns.get(&server_id).cloned() {
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

    info!("Discovery tool call: {server_name}.{tool_name}");

    let start = Instant::now();
    let call_result = client.call_tool(&tool_name, tool_arguments).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let (response, is_error) = match call_result {
        Ok(result) => {
            let is_err = result.is_error.unwrap_or(false);
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

            // If the tool returned an error, attach the schema to help the LLM retry
            if is_err {
                let error_text = result_value
                    .get("content")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("Tool returned an error");
                (
                    tool_error_with_schema(id, error_text, state, &server_id, &tool_name),
                    true,
                )
            } else {
                (
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result_value
                    }),
                    false,
                )
            }
        }
        Err(e) => {
            error!("Discovery tool call failed: {server_name}.{tool_name} -> {e}");
            (
                tool_error_with_schema(
                    id,
                    &format!("Tool call failed: {e}"),
                    state,
                    &server_id,
                    &tool_name,
                ),
                true,
            )
        }
    };

    record_tool_stats(
        &state.app_handle,
        &server_id,
        &tool_name,
        client_id,
        duration_ms,
        is_error,
    )
    .await;

    response
}
