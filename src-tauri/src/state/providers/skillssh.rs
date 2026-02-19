use serde::Deserialize;

const SEARCH_API: &str = "https://skills.sh/api/search";

/// Raw API response from skills.sh search endpoint.
#[derive(Deserialize)]
pub struct SearchResponse {
    pub skills: Vec<SkillsshEntry>,
    pub count: u64,
}

/// A single skill entry from the skills.sh API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsshEntry {
    pub id: String,
    pub skill_id: String,
    pub name: String,
    pub installs: u64,
    pub source: String,
}

/// Search the skills.sh API for skills matching a query.
/// The API requires a query of at least 2 characters, so empty/short queries
/// use "skills" as a default browse term.
pub async fn search_skills(
    client: &reqwest::Client,
    query: &str,
    limit: u32,
) -> Option<SearchResponse> {
    // skills.sh requires q >= 2 chars; fall back to a broad browse term
    let effective_query = if query.trim().len() < 2 {
        "skills"
    } else {
        query
    };

    let resp = client
        .get(SEARCH_API)
        .query(&[("q", effective_query), ("limit", &limit.to_string())])
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!("skills.sh returned status {}", resp.status());
        return None;
    }

    match resp.json().await {
        Ok(body) => Some(body),
        Err(e) => {
            tracing::warn!("Failed to parse skills.sh response: {e}");
            None
        }
    }
}
