use std::collections::HashMap;

use serde::Deserialize;

use crate::state::registry::InstallConfig;

const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
/// Max pages to fetch (safety limit). At 100/page this covers 3000 entries.
const MAX_PAGES: usize = 30;

// ---------------------------------------------------------------------------
// API response types (private — only used for deserialization)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ListResponse {
    servers: Vec<Entry>,
    metadata: Metadata,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    server: Server,
}

#[derive(Deserialize)]
struct Server {
    repository: Option<Repository>,
    #[serde(default)]
    packages: Vec<Package>,
    // TODO: parse `remotes` to support streamable-http installs (phase 2)
}

#[derive(Deserialize)]
struct Repository {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    registry_type: Option<String>,
    identifier: Option<String>,
    #[serde(default)]
    environment_variables: Vec<EnvVar>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvVar {
    name: String,
    is_required: Option<bool>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch all entries from the official MCP registry and return an index of
/// install configs keyed by normalized repository URL.
///
/// This is used to enrich MCPAnvil entries that have broken or missing install
/// configs (e.g. `node path/to/server.js` placeholders).
pub async fn fetch_install_index(client: &reqwest::Client) -> HashMap<String, InstallConfig> {
    tracing::info!("Fetching install configs from official MCP registry...");
    let mut index = HashMap::new();
    let mut cursor: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let mut req = client
            .get(REGISTRY_URL)
            .query(&[("version", "latest"), ("limit", "100")]);
        if let Some(ref c) = cursor {
            req = req.query(&[("cursor", c.as_str())]);
        }

        let resp = match req.send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!("Official registry returned status {}", r.status());
                break;
            }
            Err(e) => {
                tracing::warn!("Failed to fetch official registry: {e}");
                break;
            }
        };

        let body: ListResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to parse official registry response: {e}");
                break;
            }
        };

        for entry in body.servers {
            let Some(repo_url) = entry
                .server
                .repository
                .and_then(|r| r.url)
                .map(|u| normalize_repo_url(&u))
            else {
                continue;
            };

            if let Some(config) = best_package_config(&entry.server.packages) {
                index.insert(repo_url, config);
            }
        }

        cursor = body.metadata.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    tracing::info!(
        "Loaded {} install configs from official MCP registry",
        index.len()
    );
    index
}

/// Normalize a GitHub repository URL for cross-provider matching.
///
/// Handles trailing slashes, `.git` suffixes, and `/tree/...` subfolder paths
/// that MCPAnvil sometimes includes.
pub fn normalize_repo_url(url: &str) -> String {
    let mut url = url.trim().to_lowercase();
    while url.ends_with('/') {
        url.pop();
    }
    if url.ends_with(".git") {
        url.truncate(url.len() - 4);
    }
    // MCPAnvil repos for monorepos include the subfolder path
    if let Some(idx) = url.find("/tree/") {
        url.truncate(idx);
    }
    url
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Pick the best installable package from a list, preferring npm > pypi > oci.
fn best_package_config(packages: &[Package]) -> Option<InstallConfig> {
    const PREFERENCE: &[&str] = &["npm", "pypi", "oci"];

    for preferred in PREFERENCE {
        if let Some(pkg) = packages
            .iter()
            .find(|p| p.registry_type.as_deref() == Some(preferred))
        {
            return package_to_config(pkg);
        }
    }

    // Fall back to first package with a known registry type
    packages.iter().find_map(package_to_config)
}

/// Convert an official registry package entry into our common `InstallConfig`.
fn package_to_config(pkg: &Package) -> Option<InstallConfig> {
    let identifier = pkg.identifier.as_deref()?;
    let (command, args) = match pkg.registry_type.as_deref()? {
        "npm" => (
            "npx".to_string(),
            vec!["-y".to_string(), identifier.to_string()],
        ),
        "pypi" => ("uvx".to_string(), vec![identifier.to_string()]),
        "oci" => (
            "docker".to_string(),
            vec![
                "run".to_string(),
                "-i".to_string(),
                "--rm".to_string(),
                identifier.to_string(),
            ],
        ),
        _ => return None,
    };

    // Required env vars become empty-string placeholders that `is_placeholder()`
    // will detect — the install modal prompts the user to fill them in.
    let env: HashMap<String, String> = pkg
        .environment_variables
        .iter()
        .filter(|v| v.is_required.unwrap_or(false))
        .map(|v| (v.name.clone(), String::new()))
        .collect();

    Some(InstallConfig { command, args, env })
}
