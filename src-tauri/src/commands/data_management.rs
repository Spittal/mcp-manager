use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use tauri::Emitter;
use tracing::info;

use crate::error::AppError;
use crate::memory_client::*;

const MEMORY_API_URL: &str = "http://localhost:8000";
const PAGE_SIZE: i64 = 100;
const IMPORT_BATCH_SIZE: usize = 50;

fn client() -> MemoryApiClient {
    MemoryApiClient::new(MEMORY_API_URL.to_string())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
    exported: i64,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    imported: usize,
    total_lines: usize,
}

/// Export all memories to a JSONL file. Returns the number of memories exported.
///
/// The memory API uses vector search which returns non-deterministic ordering.
/// Paginating with offset can miss records or return duplicates.
/// We track seen IDs to deduplicate and retry with higher offsets to catch
/// records that fell between pages.
#[tauri::command]
pub async fn export_memories(app: tauri::AppHandle, path: String) -> Result<i64, AppError> {
    info!("Exporting memories to {path}");
    let c = client();

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut all_records: Vec<MemoryRecord> = Vec::new();
    let mut offset: i64 = 0;
    let mut empty_pages = 0;

    // Keep paginating until we get 3 consecutive pages with no new records.
    // This handles the non-deterministic ordering where records can slip between pages.
    loop {
        let results = c
            .search_memories_raw(SearchRequest {
                text: String::new(),
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
                filters: SearchFilters {
                    user_id: None,
                    session_id: None,
                    namespace: None,
                    memory_type: None,
                    topics: None,
                    entities: None,
                },
            })
            .await
            .map_err(|e| AppError::ConnectionFailed(e))?;

        if results.is_empty() {
            break;
        }

        let mut new_in_page = 0;
        for r in results {
            if seen_ids.insert(r.memory.id.clone()) {
                all_records.push(r.memory);
                new_in_page += 1;
            }
        }

        if new_in_page == 0 {
            empty_pages += 1;
            if empty_pages >= 3 {
                break;
            }
        } else {
            empty_pages = 0;
        }

        let _ = app.emit(
            "export-progress",
            ExportProgress {
                exported: all_records.len() as i64,
            },
        );

        offset += PAGE_SIZE;
    }

    // Write deduplicated records to file
    let mut file = std::fs::File::create(&path)
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to create file: {e}")))?;

    for record in &all_records {
        let line = serde_json::to_string(record)
            .map_err(|e| AppError::ConnectionFailed(format!("Serialize error: {e}")))?;
        writeln!(file, "{line}")
            .map_err(|e| AppError::ConnectionFailed(format!("Write error: {e}")))?;
    }

    let total = all_records.len() as i64;
    info!("Exported {total} unique memories to {path}");
    Ok(total)
}

/// Import memories from a JSONL file. Returns the number of memories imported.
#[tauri::command]
pub async fn import_memories(app: tauri::AppHandle, path: String) -> Result<usize, AppError> {
    info!("Importing memories from {path}");
    let file = std::fs::File::open(&path)
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to open file: {e}")))?;

    let reader = BufReader::new(file);
    let mut records: Vec<CreateMemoryRecord> = Vec::new();

    for line in reader.lines() {
        let line =
            line.map_err(|e| AppError::ConnectionFailed(format!("Failed to read line: {e}")))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: CreateMemoryRecord = serde_json::from_str(trimmed)
            .map_err(|e| AppError::ConnectionFailed(format!("Invalid JSON line: {e}")))?;
        records.push(record);
    }

    let total_lines = records.len();
    let c = client();
    let mut imported: usize = 0;

    for batch in records.chunks(IMPORT_BATCH_SIZE) {
        c.create_memories(CreateMemoryRequest {
            memories: batch.to_vec(),
            deduplicate: Some(true),
        })
        .await
        .map_err(|e| AppError::ConnectionFailed(e))?;

        imported += batch.len();
        let _ = app.emit(
            "import-progress",
            ImportProgress {
                imported,
                total_lines,
            },
        );
    }

    info!("Imported {imported} memories from {path}");
    Ok(imported)
}

/// Delete ALL memories via redis-cli FLUSHDB. Requires confirmation string "format my data".
#[tauri::command]
pub async fn format_memory_data(confirmation: String) -> Result<(), AppError> {
    if confirmation != "format my data" {
        return Err(AppError::Validation(
            "Invalid confirmation string".into(),
        ));
    }

    info!("Formatting all memory data via FLUSHDB");
    let output = tokio::process::Command::new("docker")
        .args(["exec", "mcp-manager-redis", "redis-cli", "FLUSHDB"])
        .output()
        .await
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to flush Redis: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::ConnectionFailed(format!(
            "FLUSHDB failed: {stderr}"
        )));
    }

    // Restart API and MCP containers so they recreate their search indexes
    info!("Restarting memory containers to rebuild indexes");
    let _ = tokio::process::Command::new("docker")
        .args(["restart", "mcp-manager-api", "mcp-manager-mcp"])
        .output()
        .await;

    // Wait for the API to be ready again
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .expect("failed to build client");
    while std::time::Instant::now() < deadline {
        if http
            .get(format!("{MEMORY_API_URL}/v1/health"))
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    info!("All memory data deleted");
    Ok(())
}
