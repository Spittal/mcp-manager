use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

pub const MAX_RECENT_CALLS: usize = 200;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStats {
    pub total_calls: u64,
    pub errors: u64,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEntry {
    pub tool: String,
    pub client: String,
    pub duration_ms: u64,
    pub is_error: bool,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStats {
    pub total_calls: u64,
    pub errors: u64,
    pub total_duration_ms: u64,
    pub tools: HashMap<String, ToolStats>,
    pub clients: HashMap<String, u64>,
    #[serde(default)]
    pub recent_calls: Vec<ToolCallEntry>,
}

impl ServerStats {
    pub fn push_call(&mut self, entry: ToolCallEntry) {
        self.recent_calls.push(entry);
        if self.recent_calls.len() > MAX_RECENT_CALLS {
            // Drop the oldest entries
            let excess = self.recent_calls.len() - MAX_RECENT_CALLS;
            self.recent_calls.drain(..excess);
        }
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs()
}

pub type StatsStore = Arc<RwLock<HashMap<String, ServerStats>>>;
