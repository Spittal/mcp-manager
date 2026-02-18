use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{debug, info};
use url::Url;

use crate::error::AppError;
use crate::state::{AuthServerMetadata, OAuthTokens, SharedOAuthStore};

/// PKCE challenge pair.
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
}

// --- Discovery ---

/// Discover the OAuth authorization server metadata for an MCP server.
///
/// Tries two approaches in order:
/// 1. RFC 9728: GET `{origin}/.well-known/oauth-protected-resource` → extract `authorization_servers[0]`
///    → GET `{auth_server}/.well-known/oauth-authorization-server`
/// 2. Fallback: GET `{origin}/.well-known/oauth-authorization-server` directly (servers like Linear
///    skip the protected-resource step and serve auth metadata on the MCP origin itself).
pub async fn discover_metadata(server_url: &str) -> Result<AuthServerMetadata, AppError> {
    let parsed =
        Url::parse(server_url).map_err(|e| AppError::OAuth(format!("Invalid server URL: {e}")))?;
    let origin = format!("{}://{}", parsed.scheme(), parsed.authority());
    let client = Client::new();

    // Attempt 1: RFC 9728 protected-resource discovery
    let pr_url = format!("{origin}/.well-known/oauth-protected-resource");
    debug!("Trying protected resource discovery at {pr_url}");

    if let Ok(response) = client.get(&pr_url).send().await {
        if response.status().is_success() {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let auth_servers = body
                    .get("authorization_servers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                if let Some(auth_server_url) = auth_servers.first() {
                    info!("Found authorization server via protected-resource: {auth_server_url}");
                    if let Ok(metadata) = fetch_auth_server_metadata(&client, auth_server_url).await
                    {
                        return Ok(metadata);
                    }
                }
            }
        }
    }

    // Attempt 2: Auth server metadata directly on the MCP origin
    info!("Protected-resource discovery failed, trying auth server metadata on origin {origin}");
    fetch_auth_server_metadata(&client, &origin).await
}

/// Fetch OAuth authorization server metadata (RFC 8414) from a given origin.
async fn fetch_auth_server_metadata(
    client: &Client,
    base_url: &str,
) -> Result<AuthServerMetadata, AppError> {
    let parsed = Url::parse(base_url)
        .map_err(|e| AppError::OAuth(format!("Invalid auth server URL: {e}")))?;
    let origin = format!("{}://{}", parsed.scheme(), parsed.authority());

    let well_known_url = format!("{origin}/.well-known/oauth-authorization-server");
    debug!("Fetching auth server metadata at {well_known_url}");

    let response = client
        .get(&well_known_url)
        .send()
        .await
        .map_err(|e| AppError::OAuth(format!("Auth server discovery failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::OAuth(format!(
            "Auth server discovery at {well_known_url} returned status {}",
            response.status()
        )));
    }

    let metadata: AuthServerMetadata = response
        .json()
        .await
        .map_err(|e| AppError::OAuth(format!("Failed to parse auth server metadata: {e}")))?;

    info!(
        "Auth server metadata: issuer={}, authorization_endpoint={}, token_endpoint={}",
        metadata.issuer, metadata.authorization_endpoint, metadata.token_endpoint
    );

    Ok(metadata)
}

// --- PKCE ---

/// Generate a PKCE code_verifier and code_challenge (S256).
pub fn generate_pkce() -> PkceChallenge {
    let mut rng = rand::rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);

    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    PkceChallenge {
        code_verifier,
        code_challenge,
    }
}

/// Generate a random state nonce for CSRF protection.
pub fn generate_state_nonce() -> String {
    let mut rng = rand::rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

// --- Dynamic Client Registration ---

/// Dynamically register a client per RFC 7591.
/// POST to registration_endpoint with client metadata.
pub async fn dynamic_register(
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<(String, Option<String>), AppError> {
    let client = Client::new();
    let body = serde_json::json!({
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "client_name": "MCP Manager",
        "token_endpoint_auth_method": "none",
    });

    debug!("Dynamic client registration at {registration_endpoint}");

    let response = client
        .post(registration_endpoint)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::OAuth(format!("Dynamic registration failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::OAuth(format!(
            "Dynamic registration returned status {status}: {body_text}"
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::OAuth(format!("Failed to parse registration response: {e}")))?;

    let client_id = result
        .get("client_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AppError::OAuth("No client_id in registration response".into()))?;

    let client_secret = result
        .get("client_secret")
        .and_then(|v| v.as_str())
        .map(String::from);

    info!("Registered client: {client_id}");
    Ok((client_id, client_secret))
}

// --- Authorization URL ---

/// Build the full authorization URL with PKCE and state.
pub fn build_authorization_url(
    metadata: &AuthServerMetadata,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceChallenge,
    state: &str,
) -> Result<String, AppError> {
    let mut url = Url::parse(&metadata.authorization_endpoint)
        .map_err(|e| AppError::OAuth(format!("Invalid authorization_endpoint URL: {e}")))?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);

    // Add scopes if the server supports any
    if !metadata.scopes_supported.is_empty() {
        url.query_pairs_mut()
            .append_pair("scope", &metadata.scopes_supported.join(" "));
    }

    Ok(url.to_string())
}

// --- Token Exchange ---

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
    metadata: &AuthServerMetadata,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<OAuthTokens, AppError> {
    let client = Client::new();

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];

    // client_secret is optional (public clients use PKCE only)
    let secret_string;
    if let Some(secret) = client_secret {
        secret_string = secret.to_string();
        params.push(("client_secret", &secret_string));
    }

    debug!("Exchanging code at {}", metadata.token_endpoint);

    let response = client
        .post(&metadata.token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::OAuth(format!("Token exchange failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::OAuth(format!(
            "Token exchange returned status {status}: {body_text}"
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::OAuth(format!("Failed to parse token response: {e}")))?;

    let access_token = result
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AppError::OAuth("No access_token in token response".into()))?;

    let refresh_token = result
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(String::from);

    let expires_in = result.get("expires_in").and_then(|v| v.as_u64());

    let obtained_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();

    info!("Token exchange successful, expires_in={expires_in:?}");

    Ok(OAuthTokens {
        access_token,
        refresh_token,
        expires_in,
        obtained_at,
    })
}

/// Refresh an access token using a refresh_token grant.
pub async fn refresh_token(
    metadata: &AuthServerMetadata,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_tok: &str,
) -> Result<OAuthTokens, AppError> {
    let client = Client::new();

    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_tok),
        ("client_id", client_id),
    ];

    let secret_string;
    if let Some(secret) = client_secret {
        secret_string = secret.to_string();
        params.push(("client_secret", &secret_string));
    }

    debug!("Refreshing token at {}", metadata.token_endpoint);

    let response = client
        .post(&metadata.token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::OAuth(format!("Token refresh failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        return Err(AppError::OAuth(format!(
            "Token refresh returned status {status}: {body_text}"
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::OAuth(format!("Failed to parse refresh response: {e}")))?;

    let access_token = result
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| AppError::OAuth("No access_token in refresh response".into()))?;

    let new_refresh = result
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| Some(refresh_tok.to_string()));

    let expires_in = result.get("expires_in").and_then(|v| v.as_u64());

    let obtained_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();

    Ok(OAuthTokens {
        access_token,
        refresh_token: new_refresh,
        expires_in,
        obtained_at,
    })
}

// --- Token expiry check ---

/// Check whether an access token has expired (with 60s buffer).
pub fn is_token_expired(tokens: &OAuthTokens) -> bool {
    let Some(expires_in) = tokens.expires_in else {
        // No expiry information — assume valid
        return false;
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();

    let expiry = tokens.obtained_at + expires_in;
    now + 60 >= expiry
}

/// Attempt to refresh the stored token for a server. Returns the new access_token on success.
pub async fn try_refresh_token(
    oauth_store: &SharedOAuthStore,
    server_id: &str,
) -> Result<String, AppError> {
    let (metadata, client_id, client_secret, refresh_tok) = {
        let store = oauth_store.lock().await;
        let oauth_state = store
            .get(server_id)
            .ok_or_else(|| AppError::OAuth("No OAuth state for server".into()))?;
        let tokens = oauth_state
            .tokens
            .as_ref()
            .ok_or_else(|| AppError::OAuth("No tokens stored".into()))?;
        let refresh = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| AppError::OAuth("No refresh token available".into()))?;
        (
            oauth_state.auth_server_metadata.clone(),
            oauth_state.client_id.clone().unwrap_or_default(),
            oauth_state.client_secret.clone(),
            refresh,
        )
    };

    let new_tokens = refresh_token(
        &metadata,
        &client_id,
        client_secret.as_deref(),
        &refresh_tok,
    )
    .await?;

    let new_access = new_tokens.access_token.clone();

    // Update the store
    {
        let mut store = oauth_store.lock().await;
        if let Some(oauth_state) = store.entries_mut().get_mut(server_id) {
            oauth_state.tokens = Some(new_tokens);
        }
    }

    Ok(new_access)
}
