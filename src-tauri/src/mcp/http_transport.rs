use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::error::AppError;
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};

/// Pending request senders, keyed by stringified JSON-RPC id.
type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>;

/// HTTP transport for remote MCP servers.
///
/// Supports two modes:
/// - **Streamable HTTP**: POST JSON-RPC to a single endpoint (preferred).
///   The server returns the response directly in the HTTP response body.
/// - **Legacy SSE**: GET an SSE endpoint that returns an `endpoint` event,
///   then POST to that URL. Responses arrive on the SSE stream, not in
///   the POST response body.
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
    /// Whether this transport uses legacy SSE mode.
    legacy_sse: bool,
    /// For legacy SSE: pending request senders keyed by JSON-RPC id.
    pending: PendingMap,
    /// Background SSE reader task handle (legacy SSE only).
    _sse_reader: Option<JoinHandle<()>>,
}

impl HttpTransport {
    /// Connect to a remote MCP server via HTTP.
    ///
    /// If the URL path ends with `/sse`, connects in legacy SSE mode (GET for endpoint
    /// discovery, then POST to discovered URL). Otherwise, assumes streamable HTTP
    /// and POSTs directly to the given URL.
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
        info!("Using streamable HTTP transport for {url}");

        Ok(Self {
            next_id: AtomicU64::new(1),
            client,
            post_url: url.to_string(),
            headers,
            session_id: Arc::new(Mutex::new(None)),
            access_token: token,
            legacy_sse: false,
            pending: Arc::new(Mutex::new(HashMap::new())),
            _sse_reader: None,
        })
    }

    /// Legacy SSE connection: GET the URL to establish the SSE stream,
    /// find the `endpoint` event, then spawn a background task to read
    /// responses from the stream.
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

        // Stream the SSE response incrementally to find the `endpoint` event.
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut post_url: Option<String> = None;

        let timeout = tokio::time::Duration::from_secs(15);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    let text = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
                    buffer.push_str(&text);
                    if let Ok(found) = parse_endpoint_from_sse(&buffer, url) {
                        post_url = Some(found);
                        break;
                    }
                }
                Ok(Some(Err(e))) => {
                    return Err(AppError::Transport(format!("SSE stream error: {e}")));
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        let post_url = post_url.ok_or_else(|| {
            AppError::Transport(
                "Timed out waiting for 'endpoint' event from SSE stream".to_string(),
            )
        })?;

        info!("Legacy SSE: discovered POST endpoint {post_url}");

        // Clear any already-consumed events from the buffer so the background
        // reader only processes new data.
        let remaining = drain_consumed_events(&buffer);

        // Spawn a background task that continues reading the SSE stream
        // and dispatches JSON-RPC responses to pending request waiters.
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        let sse_reader = tokio::spawn(async move {
            let mut buf = remaining;
            loop {
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        let text = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
                        buf.push_str(&text);
                        dispatch_sse_responses(&mut buf, &pending_clone).await;
                    }
                    Some(Err(e)) => {
                        error!("Legacy SSE stream error: {e}");
                        break;
                    }
                    None => {
                        info!("Legacy SSE stream closed by server");
                        break;
                    }
                }
            }
            // Clean up any remaining pending requests
            let mut map = pending_clone.lock().await;
            for (id, tx) in map.drain() {
                warn!("SSE stream closed with pending request id={id}");
                let _ = tx.send(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Some(serde_json::Value::String(id)),
                    result: None,
                    error: Some(crate::mcp::types::JsonRpcError {
                        code: -1,
                        message: "SSE stream closed".to_string(),
                        data: None,
                    }),
                });
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            client,
            post_url,
            headers,
            session_id: Arc::new(Mutex::new(session_id)),
            access_token,
            legacy_sse: true,
            pending,
            _sse_reader: Some(sse_reader),
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

        debug!(
            "HTTP send_request id={id} method={method} -> {}",
            self.post_url
        );

        if self.legacy_sse {
            return self.send_request_legacy_sse(id, &body, method).await;
        }

        // Streamable HTTP: POST and read response from body
        let mut req = self
            .client
            .post(&self.post_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        {
            let tok = self.access_token.lock().await;
            if let Some(ref token) = *tok {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
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
            .map_err(|e| AppError::Transport(format!("HTTP request failed: {e}")))?;

        if let Some(new_sid) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            let mut sid = self.session_id.lock().await;
            *sid = Some(new_sid.to_string());
        }

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

        let json_text = if content_type.contains("text/event-stream") {
            extract_json_from_sse(&response_text)?
        } else {
            response_text
        };

        let rpc_response: JsonRpcResponse = serde_json::from_str(&json_text).map_err(|e| {
            AppError::Protocol(format!(
                "Failed to parse JSON-RPC response: {e} — raw: {json_text}"
            ))
        })?;

        if let Some(err) = &rpc_response.error {
            return Err(AppError::Protocol(format!("{}: {}", err.code, err.message)));
        }

        Ok(rpc_response)
    }

    /// Legacy SSE: POST the request and wait for the response on the SSE stream.
    async fn send_request_legacy_sse(
        &self,
        id: u64,
        body: &serde_json::Value,
        method: &str,
    ) -> Result<JsonRpcResponse, AppError> {
        let id_str = id.to_string();

        // Register a oneshot channel for this request's response
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id_str.clone(), tx);
        }

        // POST the request — legacy SSE servers return 200/202 with no useful body
        let mut req = self
            .client
            .post(&self.post_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        {
            let tok = self.access_token.lock().await;
            if let Some(ref token) = *tok {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
        }

        {
            let sid = self.session_id.lock().await;
            if let Some(ref s) = *sid {
                req = req.header("Mcp-Session-Id", s.as_str());
            }
        }

        let response = req.json(body).send().await.map_err(|e| {
            // Clean up pending entry on send failure
            let pending = self.pending.clone();
            let id_str = id_str.clone();
            tokio::spawn(async move {
                pending.lock().await.remove(&id_str);
            });
            AppError::Transport(format!("HTTP request failed: {e}"))
        })?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.pending.lock().await.remove(&id_str);
            return Err(AppError::AuthRequired(self.post_url.clone()));
        }

        // Accept 200 and 202 as success for legacy SSE
        if !response.status().is_success() {
            self.pending.lock().await.remove(&id_str);
            return Err(AppError::Transport(format!(
                "HTTP request for {method} returned status {}",
                response.status()
            )));
        }

        // Wait for the response to arrive on the SSE stream
        let timeout = tokio::time::Duration::from_secs(60);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(rpc_response)) => {
                if let Some(err) = &rpc_response.error {
                    return Err(AppError::Protocol(format!("{}: {}", err.code, err.message)));
                }
                Ok(rpc_response)
            }
            Ok(Err(_)) => Err(AppError::Transport(
                "SSE stream closed while waiting for response".to_string(),
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id_str);
                Err(AppError::Transport(format!(
                    "Timeout waiting for SSE response to {method} (id={id})"
                )))
            }
        }
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
            let tok = self.access_token.lock().await;
            if let Some(ref token) = *tok {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
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

impl Drop for HttpTransport {
    fn drop(&mut self) {
        if let Some(ref handle) = self._sse_reader {
            handle.abort();
        }
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

/// After finding the endpoint event, return any remaining unparsed data
/// from the buffer (data after the endpoint event's double-newline).
fn drain_consumed_events(buffer: &str) -> String {
    // Find the endpoint event and skip past it
    if let Some(idx) = buffer.find("event: endpoint") {
        // Find the next double-newline after the endpoint event (event separator)
        if let Some(end) = buffer[idx..].find("\n\n") {
            let after = idx + end + 2;
            if after < buffer.len() {
                return buffer[after..].to_string();
            }
        }
    }
    // Also try without space after "event:"
    if let Some(idx) = buffer.find("event:endpoint") {
        if let Some(end) = buffer[idx..].find("\n\n") {
            let after = idx + end + 2;
            if after < buffer.len() {
                return buffer[after..].to_string();
            }
        }
    }
    String::new()
}

/// Parse complete SSE events from the buffer and dispatch JSON-RPC responses
/// to pending request waiters. Removes consumed events from the buffer,
/// leaving any incomplete trailing data.
async fn dispatch_sse_responses(buffer: &mut String, pending: &PendingMap) {
    loop {
        // Find a complete event (terminated by double newline)
        let Some(event_end) = buffer.find("\n\n") else {
            break;
        };

        let event_block = buffer[..event_end].to_string();
        // Remove the consumed event + the \n\n separator
        *buffer = buffer[event_end + 2..].to_string();

        let mut event_type = String::new();
        let mut data_parts = Vec::new();

        for line in event_block.lines() {
            if let Some(et) = line.strip_prefix("event:") {
                event_type = et.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data:") {
                data_parts.push(d.trim().to_string());
            }
        }

        // Only process "message" events (or events with no explicit type, which default to "message")
        if !event_type.is_empty() && event_type != "message" {
            debug!("Legacy SSE: ignoring event type={event_type}");
            continue;
        }

        if data_parts.is_empty() {
            continue;
        }

        let json_text = data_parts.join("");
        let rpc_response: JsonRpcResponse = match serde_json::from_str(&json_text) {
            Ok(r) => r,
            Err(e) => {
                warn!("Legacy SSE: failed to parse JSON-RPC from SSE data: {e} — raw: {json_text}");
                continue;
            }
        };

        // Extract the id to find the matching pending request
        let id_str = match &rpc_response.id {
            Some(serde_json::Value::Number(n)) => n.to_string(),
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => {
                debug!("Legacy SSE: received response with no/unexpected id, ignoring");
                continue;
            }
        };

        let mut map = pending.lock().await;
        if let Some(tx) = map.remove(&id_str) {
            debug!("Legacy SSE: dispatching response for id={id_str}");
            let _ = tx.send(rpc_response);
        } else {
            debug!("Legacy SSE: received response for unknown id={id_str}, ignoring");
        }
    }
}

/// Extract JSON-RPC response data from an SSE response body (streamable HTTP mode).
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

    Ok(json_parts.last().expect("non-empty after guard").clone())
}
