use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tracing::{error, info};

use crate::error::AppError;
use crate::mcp::client::SharedConnections;
use crate::mcp::{oauth, oauth_callback};
use crate::state::{OAuthState, ServerTransport, SharedOAuthStore, SharedState};

#[tauri::command]
pub async fn start_oauth_flow(
    app: AppHandle,
    state: State<'_, SharedState>,
    oauth_store: State<'_, SharedOAuthStore>,
    connections: State<'_, SharedConnections>,
    id: String,
) -> Result<(), AppError> {
    // 1. Read server URL from AppState
    let server_url = {
        let s = state.lock().unwrap();
        let server = s
            .servers
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;
        if !matches!(server.transport, ServerTransport::Http) {
            return Err(AppError::OAuth("OAuth is only supported for HTTP servers".into()));
        }
        server
            .url
            .clone()
            .ok_or_else(|| AppError::OAuth("No URL configured for server".into()))?
    };

    let _ = app.emit(
        "oauth-status-changed",
        serde_json::json!({ "serverId": id, "status": "discovering" }),
    );

    // 2-3. Discover auth server metadata (tries RFC 9728 first, falls back to direct)
    let metadata = oauth::discover_metadata(&server_url).await?;

    // 4. Start callback server to get the redirect URI
    let (port, callback_rx) = oauth_callback::start_callback_server().await?;
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    // 5. Dynamic client registration if available and no stored client_id
    let (client_id, client_secret) = {
        let store = oauth_store.lock().await;
        let existing = store.get(&id);
        match existing {
            Some(os) if os.client_id.is_some() => (
                os.client_id.clone().unwrap(),
                os.client_secret.clone(),
            ),
            _ => {
                drop(store);
                if let Some(ref reg_endpoint) = metadata.registration_endpoint {
                    let (cid, csec) =
                        oauth::dynamic_register(reg_endpoint, &redirect_uri).await?;
                    (cid, csec)
                } else {
                    return Err(AppError::OAuth(
                        "Server has no registration_endpoint and no client_id is stored. \
                         Cannot authenticate without a client_id."
                            .into(),
                    ));
                }
            }
        }
    };

    // 6. Generate PKCE + state nonce
    let pkce = oauth::generate_pkce();
    let state_nonce = oauth::generate_state_nonce();

    // 7. Build authorization URL
    let auth_url = oauth::build_authorization_url(
        &metadata,
        &client_id,
        &redirect_uri,
        &pkce,
        &state_nonce,
    )?;

    // 8. Open browser
    info!("Opening browser for OAuth authorization");
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| AppError::OAuth(format!("Failed to open browser: {e}")))?;

    let _ = app.emit(
        "oauth-status-changed",
        serde_json::json!({ "serverId": id, "status": "awaiting_browser" }),
    );

    // 9. Await callback (2-min timeout is built into the callback server)
    let callback_result = callback_rx
        .await
        .map_err(|_| AppError::OAuth("OAuth callback channel closed unexpectedly".into()))?
        ?;

    // Verify state nonce
    if callback_result.state != state_nonce {
        return Err(AppError::OAuth("OAuth state mismatch — possible CSRF attack".into()));
    }

    let _ = app.emit(
        "oauth-status-changed",
        serde_json::json!({ "serverId": id, "status": "exchanging_code" }),
    );

    // 10. Exchange code for tokens
    let tokens = oauth::exchange_code(
        &metadata,
        &client_id,
        client_secret.as_deref(),
        &callback_result.code,
        &redirect_uri,
        &pkce.code_verifier,
    )
    .await?;

    // 11. Store in OAuthStore
    {
        let mut store = oauth_store.lock().await;
        store.set(
            id.clone(),
            OAuthState {
                auth_server_metadata: metadata,
                client_id: Some(client_id),
                client_secret,
                tokens: Some(tokens.clone()),
            },
        );
    }

    let _ = app.emit(
        "oauth-status-changed",
        serde_json::json!({ "serverId": id, "status": "authorized" }),
    );

    info!("OAuth flow complete for server {id}, auto-reconnecting");

    // 12. Auto-retry connection with token
    //     Re-read config and connect with the new access token.
    let server_config = {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;
        server.status = Some(crate::state::ServerStatus::Connecting);
        (
            server.url.clone().unwrap_or_default(),
            server.headers.clone().unwrap_or_default(),
        )
    };

    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": id, "status": "connecting" }),
    );

    let client = crate::mcp::client::McpClient::connect_http(
        &server_config.0,
        server_config.1,
        Some(tokens.access_token),
    )
    .await;

    match client {
        Ok(mcp_client) => {
            let tools: Vec<crate::state::McpTool> = {
                let s = state.lock().unwrap();
                let server_name = s
                    .servers
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                mcp_client
                    .tools
                    .iter()
                    .map(|t| crate::state::McpTool {
                        name: t.name.clone(),
                        title: t.title.clone(),
                        description: t.description.clone(),
                        input_schema: t.input_schema.clone(),
                        server_id: id.clone(),
                        server_name: server_name.clone(),
                    })
                    .collect()
            };

            {
                let mut s = state.lock().unwrap();
                if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
                    server.status = Some(crate::state::ServerStatus::Connected);
                }
                s.connections.insert(
                    id.clone(),
                    crate::state::ConnectionState {
                        tools: tools.clone(),
                    },
                );
            }

            {
                let mut conns = connections.lock().await;
                conns.insert(id.clone(), mcp_client);
            }

            let _ = app.emit(
                "server-status-changed",
                serde_json::json!({ "serverId": id, "status": "connected" }),
            );
            let _ = app.emit(
                "tools-updated",
                serde_json::json!({ "serverId": id, "tools": tools }),
            );

            Ok(())
        }
        Err(e) => {
            error!("Auto-reconnect after OAuth failed: {e}");
            {
                let mut s = state.lock().unwrap();
                if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
                    server.status = Some(crate::state::ServerStatus::Error);
                }
            }
            let _ = app.emit(
                "server-status-changed",
                serde_json::json!({ "serverId": id, "status": "error", "error": e.to_string() }),
            );
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn clear_oauth_tokens(
    oauth_store: State<'_, SharedOAuthStore>,
    id: String,
) -> Result<(), AppError> {
    let mut store = oauth_store.lock().await;
    store.remove(&id);
    info!("Cleared OAuth tokens for server {id}");
    Ok(())
}
