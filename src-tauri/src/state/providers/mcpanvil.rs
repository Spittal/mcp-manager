use serde::Deserialize;

use crate::state::registry::{InstallConfig, MarketplaceServer};

const ANVIL_URL: &str = "https://mcpanvil.com/api/v1/all.json";
pub const PROVIDER_ID: &str = "mcpanvil";

/// MCPAnvil wraps entries in `{ version, lastUpdated, count, mcps: [...] }`.
#[derive(Deserialize)]
struct AnvilResponse {
    mcps: Vec<AnvilEntry>,
}

#[derive(Deserialize)]
struct AnvilEntry {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    repository: Option<String>,
    stars: Option<u32>,
    latest_version: Option<String>,
    installation_json: Option<String>,
}

/// Fetch MCPAnvil's full server list and normalize into `MarketplaceServer`s.
///
/// Returns servers pre-sorted by stars (descending).
pub async fn fetch_servers(client: &reqwest::Client) -> Option<Vec<MarketplaceServer>> {
    tracing::info!("Fetching MCP server data from MCPAnvil...");

    let resp = client.get(ANVIL_URL).send().await.ok()?;
    if !resp.status().is_success() {
        tracing::warn!("MCPAnvil returned status {}", resp.status());
        return None;
    }

    let body: AnvilResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to parse MCPAnvil response: {e}");
            return None;
        }
    };

    let mut servers: Vec<MarketplaceServer> = body
        .mcps
        .into_iter()
        .filter_map(|entry| {
            let id = entry.id?;
            let name = entry.name.unwrap_or_else(|| id.clone());
            let install = entry.installation_json.as_deref().and_then(parse_install);

            Some(MarketplaceServer {
                id,
                name,
                description: entry.description,
                repository_url: entry.repository,
                stars: entry.stars,
                version: entry.latest_version,
                install,
                provider: PROVIDER_ID,
            })
        })
        .collect();

    // Pre-sort by stars descending (starred first, then unstarred)
    servers.sort_by(|a, b| match (a.stars, b.stars) {
        (Some(a_stars), Some(b_stars)) => b_stars.cmp(&a_stars),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    tracing::info!("Loaded {} servers from MCPAnvil", servers.len());
    Some(servers)
}

/// Parse MCPAnvil's `installation_json` (Claude Desktop config format) into our
/// common `InstallConfig`.
///
/// Expected shape:
/// ```json
/// { "claudeDesktop": { "mcpServers": { "<name>": { "command": "npx", "args": [...], "env": {...} } } } }
/// ```
fn parse_install(json_str: &str) -> Option<InstallConfig> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let servers = val.get("claudeDesktop")?.get("mcpServers")?.as_object()?;
    let (_, config) = servers.iter().next()?;

    let command = config.get("command")?.as_str()?.to_string();

    // Only accept package-runner commands that support one-click install.
    // `node`/`python` require a local repo clone and can't be auto-installed.
    match command.as_str() {
        "npx" | "uvx" | "docker" => {}
        _ => return None,
    }

    let args = config
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let env = config
        .get("env")
        .and_then(|e| e.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    Some(InstallConfig { command, args, env })
}
