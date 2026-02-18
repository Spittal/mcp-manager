use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::{error, info};

use crate::state::{EmbeddingConfig, OAuthState, ServerConfig};
use crate::stats::ServerStats;

const STORE_FILE: &str = "config.json";
const SERVERS_KEY: &str = "servers";
const INTEGRATIONS_KEY: &str = "enabled_integrations";
const STATS_KEY: &str = "stats";
const EMBEDDING_CONFIG_KEY: &str = "embedding_config";
const OPENAI_API_KEY_KEY: &str = "openai_api_key";
const OAUTH_STORE_KEY: &str = "oauth_store";
const TOOL_DISCOVERY_KEY: &str = "tool_discovery_enabled";

// --- Generic helpers ---

fn store_get<T: DeserializeOwned>(app: &AppHandle, key: &str) -> Option<T> {
    let store = app.store(STORE_FILE).ok()?;
    let value = store.get(key)?;
    serde_json::from_value(value.clone()).ok()
}

fn store_set<T: Serialize>(app: &AppHandle, key: &str, value: &T) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for {key}: {e}");
            return;
        }
    };
    store.set(key, serde_json::to_value(value).unwrap_or_default());
    if let Err(e) = store.save() {
        error!("Failed to persist {key}: {e}");
    }
}

// --- Public API ---

pub fn load_servers(app: &AppHandle) -> Vec<ServerConfig> {
    let servers: Vec<ServerConfig> = store_get(app, SERVERS_KEY).unwrap_or_default();
    info!("Loaded {} server configs from store", servers.len());
    servers
}

pub fn save_servers(app: &AppHandle, servers: &[ServerConfig]) {
    store_set(app, SERVERS_KEY, &servers);
    info!("Saved {} server configs to store", servers.len());
}

pub fn load_enabled_integrations(app: &AppHandle) -> Vec<String> {
    store_get(app, INTEGRATIONS_KEY).unwrap_or_default()
}

pub fn save_enabled_integrations(app: &AppHandle, ids: &[String]) {
    store_set(app, INTEGRATIONS_KEY, &ids);
}

pub fn load_stats(app: &AppHandle) -> HashMap<String, ServerStats> {
    store_get(app, STATS_KEY).unwrap_or_default()
}

pub fn save_stats(app: &AppHandle, stats: &HashMap<String, ServerStats>) {
    store_set(app, STATS_KEY, stats);
}

pub fn load_embedding_config(app: &AppHandle) -> EmbeddingConfig {
    store_get(app, EMBEDDING_CONFIG_KEY).unwrap_or_default()
}

pub fn save_embedding_config(app: &AppHandle, config: &EmbeddingConfig) {
    store_set(app, EMBEDDING_CONFIG_KEY, config);
}

pub fn load_openai_api_key(app: &AppHandle) -> Option<String> {
    store_get(app, OPENAI_API_KEY_KEY)
}

pub fn save_openai_api_key(app: &AppHandle, key: &str) {
    store_set(app, OPENAI_API_KEY_KEY, &key.to_string());
}

pub fn load_oauth_store(app: &AppHandle) -> HashMap<String, OAuthState> {
    store_get(app, OAUTH_STORE_KEY).unwrap_or_default()
}

pub fn save_oauth_store(app: &AppHandle, entries: &HashMap<String, OAuthState>) {
    store_set(app, OAUTH_STORE_KEY, entries);
}

pub fn load_tool_discovery(app: &AppHandle) -> bool {
    store_get(app, TOOL_DISCOVERY_KEY).unwrap_or(false)
}

pub fn save_tool_discovery(app: &AppHandle, enabled: bool) {
    store_set(app, TOOL_DISCOVERY_KEY, &enabled);
}
