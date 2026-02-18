use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tokio::sync::RwLock;

use super::providers;

const CACHE_TTL_SECS: u64 = 3600; // 1 hour

// ---------------------------------------------------------------------------
// Common types — provider-agnostic
// ---------------------------------------------------------------------------

/// A server entry normalized from any marketplace provider.
#[derive(Debug, Clone)]
pub struct MarketplaceServer {
    /// Provider-specific unique identifier (e.g. MCPAnvil `id`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub stars: Option<u32>,
    pub version: Option<String>,
    /// Parsed install configuration, if available.
    pub install: Option<InstallConfig>,
    /// Which provider this came from (e.g. "mcpanvil").
    /// Used for multi-provider deduplication (not yet implemented).
    #[allow(dead_code)]
    pub provider: &'static str,
}

/// Everything needed to install a server via stdio.
#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

impl InstallConfig {
    /// Derive the package registry type from the command.
    pub fn runtime(&self) -> Option<&'static str> {
        match self.command.as_str() {
            "npx" | "node" => Some("npm"),
            "uvx" | "uv" => Some("pypi"),
            "docker" => Some("oci"),
            _ => None,
        }
    }

    /// Returns env vars that look like placeholders (need user input).
    pub fn placeholder_env_vars(&self) -> Vec<MarketplaceEnvVar> {
        self.env
            .iter()
            .filter(|(_, v)| is_placeholder(v))
            .map(|(k, v)| MarketplaceEnvVar {
                name: k.clone(),
                default_value: v.clone(),
                is_required: true,
                is_secret: true,
            })
            .collect()
    }

    /// Returns env vars that are real defaults (not placeholders).
    pub fn default_env(&self) -> HashMap<String, String> {
        self.env
            .iter()
            .filter(|(_, v)| !is_placeholder(v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// Heuristic: does this env var value look like a placeholder the user must fill?
fn is_placeholder(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return true;
    }
    if v.starts_with("YOUR_") || v.starts_with("your_") {
        return true;
    }
    if v.starts_with('<') && v.ends_with('>') {
        return true;
    }
    // All uppercase + underscores, length > 3 (e.g. "API_KEY_HERE")
    if v.len() > 3
        && v.chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
    {
        const REAL_VALUES: &[&str] = &[
            "TRUE", "FALSE", "INFO", "DEBUG", "WARN", "ERROR", "NONE", "AUTO",
        ];
        if !REAL_VALUES.contains(&v) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Frontend-facing types (returned to Vue via serde)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryServerSummary {
    pub id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub icon_url: Option<String>,
    pub transport_types: Vec<String>,
    pub registry_type: Option<String>,
    pub requires_config: bool,
    pub has_remote: bool,
    pub repository_url: Option<String>,
    pub installed: bool,
    pub stars: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchResult {
    pub servers: Vec<RegistryServerSummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceServerDetail {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub stars: Option<u32>,
    pub version: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env_vars: Vec<MarketplaceEnvVar>,
    pub runtime: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceEnvVar {
    pub name: String,
    pub default_value: String,
    pub is_required: bool,
    pub is_secret: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDeps {
    pub npx: bool,
    pub uvx: bool,
    pub docker: bool,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

impl MarketplaceServer {
    pub fn to_summary(&self, installed_ids: &[String]) -> RegistryServerSummary {
        let (transport_types, registry_type, requires_config) = match &self.install {
            Some(config) => (
                vec!["stdio".to_string()],
                config.runtime().map(String::from),
                !config.placeholder_env_vars().is_empty(),
            ),
            None => (vec![], None, false),
        };

        RegistryServerSummary {
            id: self.id.clone(),
            display_name: self.name.clone(),
            description: self.description.clone(),
            version: self.version.clone(),
            icon_url: None,
            transport_types,
            registry_type,
            requires_config,
            has_remote: false,
            repository_url: self.repository_url.clone(),
            installed: installed_ids.contains(&self.id),
            stars: self.stars,
        }
    }

    pub fn to_detail(&self) -> MarketplaceServerDetail {
        let (command, args, env_vars, runtime) = match &self.install {
            Some(config) => (
                Some(config.command.clone()),
                config.args.clone(),
                config.placeholder_env_vars(),
                config.runtime().map(String::from),
            ),
            None => (None, vec![], vec![], None),
        };

        MarketplaceServerDetail {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            repository_url: self.repository_url.clone(),
            stars: self.stars,
            version: self.version.clone(),
            command,
            args,
            env_vars,
            runtime,
        }
    }
}

// ---------------------------------------------------------------------------
// Cache — holds the full dataset from all configured providers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CacheData {
    servers: Vec<MarketplaceServer>,
    fetched_at: Instant,
}

#[derive(Debug, Clone)]
pub struct MarketplaceCache {
    inner: Arc<RwLock<Option<CacheData>>>,
    http: reqwest::Client,
}

impl MarketplaceCache {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("mcp-manager")
            .build()
            .expect("reqwest client should build");
        Self {
            inner: Arc::default(),
            http,
        }
    }

    /// Ensure the cache is populated. Fetches from provider(s) if empty or expired.
    /// Returns `true` if data is available, `false` if the fetch failed and no
    /// stale data is cached.
    pub async fn ensure_loaded(&self) -> bool {
        {
            let data = self.inner.read().await;
            if let Some(ref d) = *data {
                if d.fetched_at.elapsed().as_secs() < CACHE_TTL_SECS {
                    return true;
                }
            }
        }

        // Fetch from all providers concurrently.
        // MCPAnvil: primary source for discovery, popularity (stars), descriptions.
        // Official registry: install config fallback for entries MCPAnvil can't install.
        let (anvil_result, official_index) = tokio::join!(
            providers::mcpanvil::fetch_servers(&self.http),
            providers::official_registry::fetch_install_index(&self.http),
        );

        if let Some(mut servers) = anvil_result {
            // Enrich MCPAnvil entries that have no install config (e.g. `node`
            // placeholder commands) with proper configs from the official registry,
            // matched by normalized repository URL.
            for server in &mut servers {
                if server.install.is_some() {
                    continue;
                }
                if let Some(repo_url) = &server.repository_url {
                    let normalized = providers::official_registry::normalize_repo_url(repo_url);
                    if let Some(config) = official_index.get(&normalized) {
                        server.install = Some(config.clone());
                    }
                }
            }

            let mut data = self.inner.write().await;
            *data = Some(CacheData {
                servers,
                fetched_at: Instant::now(),
            });
            return true;
        }

        // Fetch failed — return whether stale data is still available.
        self.inner.read().await.is_some()
    }

    /// Search servers by query, return a paginated slice sorted by stars.
    pub async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
        installed_ids: &[String],
    ) -> RegistrySearchResult {
        let data = self.inner.read().await;
        let Some(ref cache) = *data else {
            return RegistrySearchResult {
                servers: vec![],
                has_more: false,
            };
        };

        let query_lower = query.to_lowercase();
        let filtered: Vec<&MarketplaceServer> = if query_lower.is_empty() {
            cache.servers.iter().collect()
        } else {
            cache
                .servers
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&query_lower)
                        || s.description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                })
                .collect()
        };

        let total = filtered.len();
        let page: Vec<RegistryServerSummary> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|s| s.to_summary(installed_ids))
            .collect();
        let has_more = offset + page.len() < total;

        RegistrySearchResult {
            servers: page,
            has_more,
        }
    }

    /// Look up a single server by id and return its install detail.
    pub async fn get_detail(&self, id: &str) -> Option<MarketplaceServerDetail> {
        let data = self.inner.read().await;
        let cache = data.as_ref()?;
        cache
            .servers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.to_detail())
    }

    /// Look up a server's install config by id.
    pub async fn get_install_config(&self, id: &str) -> Option<(String, InstallConfig)> {
        let data = self.inner.read().await;
        let cache = data.as_ref()?;
        cache
            .servers
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.install.clone().map(|config| (s.name.clone(), config)))
    }
}
