use crate::error::AppError;
use crate::memory_client::*;

const MEMORY_API_URL: &str = "http://localhost:8000";

fn client() -> MemoryApiClient {
    MemoryApiClient::new(MEMORY_API_URL.to_string())
}

#[tauri::command]
pub async fn search_memories(
    text: String,
    limit: Option<i64>,
    offset: Option<i64>,
    memory_type: Option<String>,
    topics: Option<Vec<String>>,
    entities: Option<Vec<String>>,
    namespace: Option<String>,
    user_id: Option<String>,
    session_id: Option<String>,
) -> Result<MemorySearchResult, AppError> {
    let filters = SearchFilters {
        user_id: user_id.map(|v| FilterEq { eq: v }),
        session_id: session_id.map(|v| FilterEq { eq: v }),
        namespace: namespace.map(|v| FilterEq { eq: v }),
        memory_type: memory_type.map(|v| FilterEq { eq: v }),
        topics: topics.map(|v| FilterAny { any: v }),
        entities: entities.map(|v| FilterAny { any: v }),
    };

    let request = SearchRequest {
        text,
        limit,
        offset,
        filters,
    };

    client()
        .search_memories(request)
        .await
        .map_err(|e| AppError::ConnectionFailed(e))
}

#[tauri::command]
pub async fn get_memory(id: String) -> Result<MemoryItem, AppError> {
    client()
        .get_memory(&id)
        .await
        .map_err(|e| AppError::ConnectionFailed(e))
}

#[tauri::command]
pub async fn check_memory_health() -> Result<bool, AppError> {
    match client().health().await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
