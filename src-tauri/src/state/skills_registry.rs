use serde::Serialize;

use super::providers;

// ---------------------------------------------------------------------------
// Frontend-facing types (returned to Vue via serde)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSkillSummary {
    pub id: String,
    pub name: String,
    pub source: String,
    pub skill_id: String,
    pub installs: u64,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceSkillDetail {
    pub id: String,
    pub name: String,
    pub source: String,
    pub skill_id: String,
    pub installs: u64,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsSearchResult {
    pub skills: Vec<MarketplaceSkillSummary>,
    pub count: u64,
}

// ---------------------------------------------------------------------------
// Cache — on-demand search (no bulk pre-fetch like MCPAnvil)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SkillsMarketplaceCache {
    http: reqwest::Client,
}

impl SkillsMarketplaceCache {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("mcp-manager")
            .build()
            .expect("reqwest client should build");
        Self { http }
    }

    /// Search skills.sh and return results with installed status.
    pub async fn search(
        &self,
        query: &str,
        limit: u32,
        installed_ids: &[String],
    ) -> SkillsSearchResult {
        let resp = providers::skillssh::search_skills(&self.http, query, limit).await;

        match resp {
            Some(data) => SkillsSearchResult {
                skills: data
                    .skills
                    .into_iter()
                    .map(|entry| MarketplaceSkillSummary {
                        installed: installed_ids.contains(&entry.id),
                        id: entry.id,
                        name: entry.name,
                        source: entry.source,
                        skill_id: entry.skill_id,
                        installs: entry.installs,
                    })
                    .collect(),
                count: data.count,
            },
            None => SkillsSearchResult {
                skills: vec![],
                count: 0,
            },
        }
    }

    /// Fetch the SKILL.md content from GitHub raw.
    /// Tries `{source}/HEAD/{skill_id}/SKILL.md` first, then `{source}/HEAD/skills/{skill_id}/SKILL.md`.
    pub async fn fetch_skill_content(
        &self,
        source: &str,
        skill_id: &str,
    ) -> Option<String> {
        // Try root-level path first
        let url1 = format!(
            "https://raw.githubusercontent.com/{source}/HEAD/{skill_id}/SKILL.md"
        );
        if let Some(content) = self.try_fetch(&url1).await {
            return Some(content);
        }

        // Fallback: skills/ prefix
        let url2 = format!(
            "https://raw.githubusercontent.com/{source}/HEAD/skills/{skill_id}/SKILL.md"
        );
        self.try_fetch(&url2).await
    }

    async fn try_fetch(&self, url: &str) -> Option<String> {
        let resp = self.http.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.text().await.ok()
    }
}
