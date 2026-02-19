use serde::{Deserialize, Serialize};

/// An installed skill, persisted in the store and synced to AI tool directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    /// Full skill path, e.g. "vercel-labs/agent-skills/vercel-react-best-practices"
    pub id: String,
    /// Display name from SKILL.md frontmatter
    pub name: String,
    /// Directory name, e.g. "vercel-react-best-practices"
    pub skill_id: String,
    /// GitHub repo, e.g. "vercel-labs/agent-skills"
    pub source: String,
    /// Description from SKILL.md frontmatter
    pub description: String,
    /// The full SKILL.md content (stored for offline use)
    pub content: String,
    /// Whether this skill is currently active
    pub enabled: bool,
    /// Install count from marketplace at install time
    pub installs: Option<u64>,
}
