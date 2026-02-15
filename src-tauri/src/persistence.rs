use std::collections::HashMap;

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::{error, info};

use crate::state::{EmbeddingConfig, ServerConfig};
use crate::stats::ServerStats;

const STORE_FILE: &str = "config.json";
const SERVERS_KEY: &str = "servers";
const INTEGRATIONS_KEY: &str = "enabled_integrations";
const STATS_KEY: &str = "stats";
const EMBEDDING_CONFIG_KEY: &str = "embedding_config";
const OPENAI_API_KEY_KEY: &str = "openai_api_key";

/// Load saved server configurations from the persistent store.
/// Returns an empty Vec if no data is stored yet or deserialization fails.
pub fn load_servers(app: &AppHandle) -> Vec<ServerConfig> {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store: {e}");
            return Vec::new();
        }
    };

    match store.get(SERVERS_KEY) {
        Some(value) => match serde_json::from_value::<Vec<ServerConfig>>(value.clone()) {
            Ok(servers) => {
                info!("Loaded {} server configs from store", servers.len());
                servers
            }
            Err(e) => {
                error!("Failed to deserialize servers from store: {e}");
                Vec::new()
            }
        },
        None => {
            info!("No saved servers found in store");
            Vec::new()
        }
    }
}

/// Save server configurations to the persistent store.
pub fn save_servers(app: &AppHandle, servers: &[ServerConfig]) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for saving: {e}");
            return;
        }
    };

    let value = match serde_json::to_value(servers) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to serialize servers: {e}");
            return;
        }
    };

    store.set(SERVERS_KEY, value);

    if let Err(e) = store.save() {
        error!("Failed to save store to disk: {e}");
    } else {
        info!("Saved {} server configs to store", servers.len());
    }
}

/// Load enabled integration IDs from the persistent store.
pub fn load_enabled_integrations(app: &AppHandle) -> Vec<String> {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    match store.get(INTEGRATIONS_KEY) {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Save enabled integration IDs to the persistent store.
pub fn save_enabled_integrations(app: &AppHandle, ids: &[String]) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for saving integrations: {e}");
            return;
        }
    };

    store.set(
        INTEGRATIONS_KEY,
        serde_json::to_value(ids).unwrap_or_default(),
    );

    if let Err(e) = store.save() {
        error!("Failed to save integrations to disk: {e}");
    }
}

/// Load tool usage stats from the persistent store.
pub fn load_stats(app: &AppHandle) -> HashMap<String, ServerStats> {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };

    match store.get(STATS_KEY) {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or_default(),
        None => HashMap::new(),
    }
}

/// Save tool usage stats to the persistent store.
pub fn save_stats(app: &AppHandle, stats: &HashMap<String, ServerStats>) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for saving stats: {e}");
            return;
        }
    };

    store.set(STATS_KEY, serde_json::to_value(stats).unwrap_or_default());

    if let Err(e) = store.save() {
        error!("Failed to save stats to disk: {e}");
    }
}

/// Load embedding config from the persistent store.
pub fn load_embedding_config(app: &AppHandle) -> EmbeddingConfig {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(_) => return EmbeddingConfig::default(),
    };

    match store.get(EMBEDDING_CONFIG_KEY) {
        Some(value) => serde_json::from_value(value.clone()).unwrap_or_default(),
        None => EmbeddingConfig::default(),
    }
}

/// Save embedding config to the persistent store.
pub fn save_embedding_config(app: &AppHandle, config: &EmbeddingConfig) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for saving embedding config: {e}");
            return;
        }
    };

    store.set(
        EMBEDDING_CONFIG_KEY,
        serde_json::to_value(config).unwrap_or_default(),
    );

    if let Err(e) = store.save() {
        error!("Failed to save embedding config to disk: {e}");
    }
}

/// Load OpenAI API key from the persistent store.
pub fn load_openai_api_key(app: &AppHandle) -> Option<String> {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(_) => return None,
    };

    store
        .get(OPENAI_API_KEY_KEY)
        .and_then(|v| v.as_str().map(String::from))
}

/// Save OpenAI API key to the persistent store.
pub fn save_openai_api_key(app: &AppHandle, key: &str) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open store for saving OpenAI API key: {e}");
            return;
        }
    };

    store.set(OPENAI_API_KEY_KEY, serde_json::Value::String(key.into()));

    if let Err(e) = store.save() {
        error!("Failed to save OpenAI API key to disk: {e}");
    }
}

