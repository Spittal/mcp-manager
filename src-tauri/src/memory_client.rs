use reqwest::Client;
use serde::{Deserialize, Serialize};

// --- Responses from agent-memory-server ---

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub now: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub memory_type: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub namespace: Option<String>,
    pub topics: Option<Vec<String>>,
    pub entities: Option<Vec<String>>,
    pub event_date: Option<String>,
    pub created_at: String,
    pub last_accessed: String,
    pub updated_at: String,
    pub persisted_at: Option<String>,
    pub access_count: Option<i64>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecordResult {
    #[serde(flatten)]
    pub memory: MemoryRecord,
    pub dist: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchResponse {
    pub memories: Vec<MemoryRecordResult>,
    pub total: i64,
    pub next_offset: Option<i64>,
}

// --- Request types ---

#[derive(Debug, Serialize)]
pub struct SearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<FilterEq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<FilterEq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<FilterEq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<FilterEq>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<FilterAny>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<FilterAny>,
}

#[derive(Debug, Serialize)]
pub struct FilterEq {
    pub eq: String,
}

#[derive(Debug, Serialize)]
pub struct FilterAny {
    pub any: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

// --- Frontend-facing types ---

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub text: String,
    pub memory_type: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub namespace: Option<String>,
    pub topics: Vec<String>,
    pub entities: Vec<String>,
    pub event_date: Option<String>,
    pub created_at: String,
    pub last_accessed: String,
    pub updated_at: String,
    pub pinned: bool,
    pub distance: Option<f64>,
}

impl From<MemoryRecordResult> for MemoryItem {
    fn from(r: MemoryRecordResult) -> Self {
        Self {
            id: r.memory.id,
            text: r.memory.text,
            memory_type: r.memory.memory_type,
            user_id: r.memory.user_id,
            session_id: r.memory.session_id,
            namespace: r.memory.namespace,
            topics: r.memory.topics.unwrap_or_default(),
            entities: r.memory.entities.unwrap_or_default(),
            event_date: r.memory.event_date,
            created_at: r.memory.created_at,
            last_accessed: r.memory.last_accessed,
            updated_at: r.memory.updated_at,
            pinned: r.memory.pinned.unwrap_or(false),
            distance: r.dist,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchResult {
    pub memories: Vec<MemoryItem>,
    pub total: i64,
    pub next_offset: Option<i64>,
}

// --- Import/Create types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryRecord {
    pub id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discrete_memory_extracted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_strategy_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persisted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_from: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryRequest {
    pub memories: Vec<CreateMemoryRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicate: Option<bool>,
}

// --- Client ---

#[derive(Clone)]
pub struct MemoryApiClient {
    client: Client,
    base_url: String,
}

impl MemoryApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn health(&self) -> Result<HealthResponse, String> {
        self.client
            .get(format!("{}/v1/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Connection failed: {e}"))?
            .json::<HealthResponse>()
            .await
            .map_err(|e| format!("Invalid response: {e}"))
    }

    pub async fn search_memories(
        &self,
        request: SearchRequest,
    ) -> Result<MemorySearchResult, String> {
        let resp = self
            .client
            .post(format!("{}/v1/long-term-memory/search", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Search failed: {e}"))?
            .json::<MemorySearchResponse>()
            .await
            .map_err(|e| format!("Invalid search response: {e}"))?;

        Ok(MemorySearchResult {
            memories: resp.memories.into_iter().map(MemoryItem::from).collect(),
            total: resp.total,
            next_offset: resp.next_offset,
        })
    }

    pub async fn get_memory(&self, id: &str) -> Result<MemoryItem, String> {
        let resp = self
            .client
            .get(format!("{}/v1/long-term-memory/{id}", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Fetch failed: {e}"))?;

        if resp.status() == 404 {
            return Err("Memory not found".to_string());
        }

        let record = resp
            .json::<MemoryRecord>()
            .await
            .map_err(|e| format!("Invalid response: {e}"))?;

        Ok(MemoryItem {
            id: record.id,
            text: record.text,
            memory_type: record.memory_type,
            user_id: record.user_id,
            session_id: record.session_id,
            namespace: record.namespace,
            topics: record.topics.unwrap_or_default(),
            entities: record.entities.unwrap_or_default(),
            event_date: record.event_date,
            created_at: record.created_at,
            last_accessed: record.last_accessed,
            updated_at: record.updated_at,
            pinned: record.pinned.unwrap_or(false),
            distance: None,
        })
    }

    /// Search returning raw API records (snake_case) for export.
    pub async fn search_memories_raw(
        &self,
        request: SearchRequest,
    ) -> Result<Vec<MemoryRecordResult>, String> {
        let resp = self
            .client
            .post(format!("{}/v1/long-term-memory/search", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Search failed: {e}"))?
            .json::<MemorySearchResponse>()
            .await
            .map_err(|e| format!("Invalid search response: {e}"))?;

        Ok(resp.memories)
    }

    pub async fn create_memories(&self, request: CreateMemoryRequest) -> Result<(), String> {
        let resp = self
            .client
            .post(format!("{}/v1/long-term-memory/", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Create failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Create failed ({status}): {body}"));
        }
        Ok(())
    }

}
