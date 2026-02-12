use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use reqwest::Client;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};

/// HTTP transport for remote MCP servers.
///
/// Supports two modes:
/// - **Streamable HTTP**: POST JSON-RPC to a single endpoint (preferred).
/// - **Legacy SSE**: GET an SSE endpoint that returns an `endpoint` event, then POST to that URL.
pub struct HttpTransport {
    next_id: AtomicU64,
    client: Client,
    /// The URL to POST JSON-RPC requests to.
    post_url: String,
    /// Extra headers to include on every request (e.g. Authorization).
    headers: HashMap<String, String>,
    /// Session ID returned by the server, sent on subsequent requests.
    session_id: Arc<Mutex<Option<String>>>,
    /// OAuth access token, injected as Bearer header when present.
    access_token: Arc<Mutex<Option<String>>>,
}

impl HttpTransport {
    /// Connect to a remote MCP server via HTTP.
    ///
    /// If the URL path ends with `/sse`, connects in legacy SSE mode (GET for endpoint
    /// discovery, then POST to discovered URL). Otherwise, assumes streamable HTTP
    /// and POSTs directly to the given URL. The first real request (initialize) from
    /// the McpClient layer will validate that the server is reachable.
    pub async fn connect(
        url: &str,
        headers: HashMap<String, String>,
        access_token: Option<String>,
    ) -> Result<Self, AppError> {
        let client = Client::new();
        let token = Arc::new(Mutex::new(access_token));

        // Heuristic: if the URL ends with /sse, use legacy SSE mode
        if url.ends_with("/sse") {
            info!("URL ends with /sse, using legacy SSE transport for {url}");
            return Self::connect_legacy_sse(url, headers, client, token).await;
        }

        // Default: streamable HTTP — just store the URL, no probing needed.
        // The McpClient will send `initialize` as the first request and that
        // will validate that the endpoint is reachable.
        info!("Using streamable HTTP transport for {url}");

        Ok(Self {
            next_id: AtomicU64::new(1),
            client,
            post_url: url.to_string(),
            headers,
            session_id: Arc::new(Mutex::new(None)),
            access_token: token,
        })
    }

    /// Update the OAuth access token (used after token refresh or initial auth).
    pub async fn set_access_token(&self, token: Option<String>) {
        let mut t = self.access_token.lock().await;
        *t = token;
    }

    /// Legacy SSE connection: GET the URL, parse the `endpoint` event, then POST to that URL.
    async fn connect_legacy_sse(
        url: &str,
        headers: HashMap<String, String>,
        client: Client,
        access_token: Arc<Mutex<Option<String>>>,
    ) -> Result<Self, AppError> {
        let mut req = client.get(url).header("Accept", "text/event-stream");

        for (k, v) in &headers {
            req = req.header(k.as_str(), v.as_str());
        }

        // Inject Bearer token if available
        {
            let tok = access_token.lock().await;
            if let Some(ref token) = *tok {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
        }

        let response = req
            .send()
            .await
            .map_err(|e| AppError::Transport(format!("SSE GET request failed: {e}")))?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::AuthRequired(url.to_string()));
        }

        if !response.status().is_success() {
            return Err(AppError::Transport(format!(
                "SSE endpoint returned status {}",
                response.status()
            )));
        }

        let session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Read the SSE stream to find the `endpoint` event
        let body = response
            .text()
            .await
            .map_err(|e| AppError::Transport(format!("Failed to read SSE body: {e}")))?;

        let post_url = parse_endpoint_from_sse(&body, url)?;

        info!("Legacy SSE: discovered POST endpoint {post_url}");

        Ok(Self {
            next_id: AtomicU64::new(1),
            client,
            post_url,
            headers,
            session_id: Arc::new(Mutex::new(session_id)),
            access_token,
        })
    }

    /// Send a JSON-RPC request and return the response.
    pub async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, AppError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(id.into())),
            method: method.to_string(),
            params,
        };

        let body = serde_json::to_value(&request)
            .map_err(|e| AppError::Transport(format!("Failed to serialize request: {e}")))?;

        debug!("HTTP send_request id={id} method={method} -> {}", self.post_url);

        let mut req = self
            .client
            .post(&self.post_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        // Inject Bearer token if available (after custom headers so OAuth takes precedence)
        {
            let tok = self.access_token.lock().await;
            if let Some(ref token) = *tok {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
        }

        // Include session ID if we have one
        {
            let sid = self.session_id.lock().await;
            if let Some(ref s) = *sid {
                req = req.header("Mcp-Session-Id", s.as_str());
            }
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Transport(format!("HTTP request failed: {e}")))?;

        // Capture/update session ID from response
        if let Some(new_sid) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            let mut sid = self.session_id.lock().await;
            *sid = Some(new_sid.to_string());
        }

        // Detect 401 before the generic non-2xx check
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::AuthRequired(self.post_url.clone()));
        }

        if !response.status().is_success() {
            return Err(AppError::Transport(format!(
                "HTTP request for {method} returned status {}",
                response.status()
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let response_text = response
            .text()
            .await
            .map_err(|e| AppError::Transport(format!("Failed to read HTTP response: {e}")))?;

        // If the response is SSE, extract the JSON-RPC response from data lines
        let json_text = if content_type.contains("text/event-stream") {
            extract_json_from_sse(&response_text)?
        } else {
            response_text
        };

        let rpc_response: JsonRpcResponse = serde_json::from_str(&json_text)
            .map_err(|e| {
                AppError::Protocol(format!(
                    "Failed to parse JSON-RPC response: {e} — raw: {json_text}"
                ))
            })?;

        if let Some(err) = &rpc_response.error {
            return Err(AppError::Protocol(format!(
                "{}: {}",
                err.code, err.message
            )));
        }

        Ok(rpc_response)
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), AppError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };

        let body = serde_json::to_value(&request)
            .map_err(|e| AppError::Transport(format!("Failed to serialize notification: {e}")))?;

        debug!("HTTP send_notification method={method}");

        let mut req = self
            .client
            .post(&self.post_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        {
            let sid = self.session_id.lock().await;
            if let Some(ref s) = *sid {
                req = req.header("Mcp-Session-Id", s.as_str());
            }
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Transport(format!("HTTP notification failed: {e}")))?;

        // Update session ID if returned
        if let Some(new_sid) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            let mut sid = self.session_id.lock().await;
            *sid = Some(new_sid.to_string());
        }

        // Notifications may return 200 or 202; we don't need the body.
        if !response.status().is_success() {
            warn!(
                "HTTP notification {method} returned status {}",
                response.status()
            );
        }

        Ok(())
    }
}

/// Parse the `endpoint` event from an SSE body to get the POST URL.
fn parse_endpoint_from_sse(body: &str, base_url: &str) -> Result<String, AppError> {
    let mut current_event = String::new();

    for line in body.lines() {
        if let Some(event_type) = line.strip_prefix("event:") {
            current_event = event_type.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data:") {
            if current_event == "endpoint" {
                let endpoint = data.trim();
                // The endpoint may be a relative path or absolute URL
                if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    return Ok(endpoint.to_string());
                }
                // Relative path — resolve against base URL origin
                if let Some(slash_pos) = base_url[..base_url.len().saturating_sub(1)].rfind('/') {
                    // Find the scheme+authority portion (e.g., https://mcp.linear.app)
                    // by looking for the third slash
                    let origin_end = base_url
                        .find("://")
                        .map(|i| {
                            base_url[i + 3..]
                                .find('/')
                                .map(|j| i + 3 + j)
                                .unwrap_or(base_url.len())
                        })
                        .unwrap_or(slash_pos);
                    let origin = &base_url[..origin_end];
                    let path = if endpoint.starts_with('/') {
                        endpoint.to_string()
                    } else {
                        format!("/{endpoint}")
                    };
                    return Ok(format!("{origin}{path}"));
                }
                return Ok(format!("{}/{}", base_url, endpoint));
            }
        }
    }

    Err(AppError::Transport(
        "No 'endpoint' event found in SSE stream".to_string(),
    ))
}

/// Extract JSON-RPC response data from an SSE response body.
/// SSE responses contain `data:` lines with JSON fragments.
fn extract_json_from_sse(body: &str) -> Result<String, AppError> {
    let mut json_parts = Vec::new();
    let mut current_event = String::new();

    for line in body.lines() {
        if let Some(event_type) = line.strip_prefix("event:") {
            current_event = event_type.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data:") {
            // Accept "message" events or events with no type (default is "message")
            if current_event.is_empty() || current_event == "message" {
                json_parts.push(data.trim().to_string());
            }
        }
    }

    if json_parts.is_empty() {
        return Err(AppError::Transport(
            "No JSON data found in SSE response".to_string(),
        ));
    }

    // Typically each data line is a complete JSON object
    // Return the last one (which should be the response)
    Ok(json_parts.last().unwrap().clone())
}
