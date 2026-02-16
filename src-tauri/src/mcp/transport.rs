use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, error, warn};

use crate::error::AppError;
use crate::mcp::types::{JsonRpcRequest, JsonRpcResponse};

/// A pending request awaiting a response from the MCP server.
type PendingRequest = oneshot::Sender<JsonRpcResponse>;

/// Handle for writing to a running MCP server's stdin and tracking pending requests.
pub struct StdioTransport {
    next_id: AtomicU64,
    /// PID of the spawned child process.
    pid: u32,
    /// Channel to send raw JSON lines to the stdin writer task.
    stdin_tx: mpsc::Sender<String>,
    /// Map of request ID -> oneshot sender for response correlation.
    pending: Arc<Mutex<HashMap<u64, PendingRequest>>>,
}

impl StdioTransport {
    /// Spawn an MCP server process and return a transport handle.
    ///
    /// `command` is the program name (e.g. "node", "npx", "python").
    /// `args` are the command-line arguments.
    /// `env` is an optional set of extra environment variables.
    pub fn spawn(
        app: &AppHandle,
        server_id: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, AppError> {
        let mut cmd = app.shell().command(command);

        for arg in args {
            cmd = cmd.arg(arg);
        }
        for (k, v) in env {
            cmd = cmd.env(k, v);
        }

        let (mut rx, mut child) = cmd
            .spawn()
            .map_err(|e| AppError::Transport(format!("Failed to spawn process: {e}")))?;

        let pid = child.pid();

        // Channel for sending lines to stdin
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);

        // Stdin writer task
        tauri::async_runtime::spawn(async move {
            while let Some(line) = stdin_rx.recv().await {
                if let Err(e) = child.write(line.as_bytes()) {
                    error!("Failed to write to stdin: {e}");
                    break;
                }
            }
            // When channel closes, kill the child process
            debug!("Stdin channel closed, killing child process");
            let _ = child.kill();
        });

        let pending: Arc<Mutex<HashMap<u64, PendingRequest>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        // Channel for notifications (server-initiated messages that don't match a pending request)
        let (notification_tx, _notification_rx) = mpsc::channel::<JsonRpcResponse>(64);

        let log_app = app.clone();
        let log_server_id = server_id.to_string();

        // Stdout/stderr reader task
        tauri::async_runtime::spawn(async move {
            let mut stdout_buf = String::new();
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(bytes) => {
                        let chunk = String::from_utf8_lossy(&bytes);
                        stdout_buf.push_str(&chunk);

                        // Process complete lines
                        while let Some(newline_pos) = stdout_buf.find('\n') {
                            let line = stdout_buf[..newline_pos].trim().to_string();
                            stdout_buf = stdout_buf[newline_pos + 1..].to_string();

                            if line.is_empty() {
                                continue;
                            }

                            debug!("MCP stdout: {line}");

                            match serde_json::from_str::<JsonRpcResponse>(&line) {
                                Ok(response) => {
                                    // Check if this is a response to a pending request
                                    if let Some(serde_json::Value::Number(n)) = &response.id {
                                        if let Some(id) = n.as_u64() {
                                            let mut map = pending_clone.lock().await;
                                            if let Some(sender) = map.remove(&id) {
                                                let _ = sender.send(response);
                                                continue;
                                            }
                                        }
                                    }
                                    // Not a response to a pending request — treat as notification
                                    let _ = notification_tx.send(response).await;
                                }
                                Err(e) => {
                                    warn!("Failed to parse JSON-RPC message: {e} — raw: {line}");
                                }
                            }
                        }
                    }
                    CommandEvent::Stderr(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).trim().to_string();
                        if !text.is_empty() {
                            // Python sends all logging to stderr — detect the
                            // actual level from the message content instead of
                            // treating everything as an error.
                            let level = detect_log_level(&text);
                            warn!("MCP stderr: {text}");
                            let _ = log_app.emit(
                                "server-log",
                                serde_json::json!({
                                    "serverId": log_server_id,
                                    "level": level,
                                    "message": text,
                                }),
                            );
                        }
                    }
                    CommandEvent::Terminated(status) => {
                        debug!("MCP process terminated: {status:?}");
                        let _ = log_app.emit(
                            "server-log",
                            serde_json::json!({
                                "serverId": log_server_id,
                                "level": "info",
                                "message": format!("Process exited: {status:?}"),
                            }),
                        );
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            pid,
            stdin_tx,
            pending,
        })
    }

    /// Send a JSON-RPC request and wait for the correlated response.
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

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        let line = serde_json::to_string(&request)
            .map_err(|e| AppError::Transport(format!("Failed to serialize request: {e}")))?;

        self.stdin_tx
            .send(format!("{line}\n"))
            .await
            .map_err(|_| AppError::Transport("Stdin channel closed".to_string()))?;

        debug!("Sent request id={id} method={method}");

        let response = tokio::time::timeout(std::time::Duration::from_secs(60), rx)
            .await
            .map_err(|_| {
                AppError::Transport(format!("Timeout waiting for response to {method} (id={id})"))
            })?
            .map_err(|_| AppError::Transport("Response channel dropped".to_string()))?;

        if let Some(err) = &response.error {
            return Err(AppError::Protocol(format!(
                "{}: {}",
                err.code, err.message
            )));
        }

        Ok(response)
    }

    /// Send a JSON-RPC notification (no id, no response expected).
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

        let line = serde_json::to_string(&request)
            .map_err(|e| AppError::Transport(format!("Failed to serialize notification: {e}")))?;

        self.stdin_tx
            .send(format!("{line}\n"))
            .await
            .map_err(|_| AppError::Transport("Stdin channel closed".to_string()))?;

        debug!("Sent notification method={method}");

        Ok(())
    }

    /// Return the PID of the spawned child process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Shut down the transport — closes stdin which triggers child process kill.
    pub fn shutdown(&self) {
        // Dropping the sender side is enough — the stdin writer task will kill the child
        // We don't explicitly drop here because the transport owns the sender,
        // but callers can drop the whole StdioTransport.
        debug!("StdioTransport::shutdown called");
    }
}

/// Detect the log level from stderr content. Python sends all logging to
/// stderr, so we parse the message to find the actual level keyword.
fn detect_log_level(text: &str) -> &'static str {
    let upper = text.to_uppercase();
    if upper.contains(" ERROR ") || upper.contains("TRACEBACK") {
        "error"
    } else if upper.contains("WARNING") || upper.contains("USERWARNING") {
        "warn"
    } else if upper.contains(" INFO ") || upper.contains(" DEBUG ") {
        "info"
    } else {
        // Default stderr to warn — it's not stdout, but not necessarily an error
        "warn"
    }
}
