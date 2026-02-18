use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::error::AppError;
use crate::mcp::client::SharedConnections;
use crate::mcp::proxy::ProxyState;
use crate::state::SharedState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisHealth {
    pub ok: bool,
    pub latency_ms: u64,
    pub used_memory_human: Option<String>,
    pub connected_clients: Option<u64>,
    pub uptime_in_seconds: Option<u64>,
    pub db_keys: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStats {
    pub name: String,
    pub command: String,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyHealth {
    pub running: bool,
    pub port: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatusResponse {
    pub proxy: ProxyHealth,
    pub redis: Option<RedisHealth>,
    pub processes: Vec<ProcessStats>,
    pub server_count: usize,
    pub connected_count: usize,
    pub checked_at: u64,
}

async fn check_redis_health() -> RedisHealth {
    let start = Instant::now();

    let client = match redis::Client::open("redis://localhost:6379") {
        Ok(c) => c,
        Err(e) => {
            return RedisHealth {
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                used_memory_human: None,
                connected_clients: None,
                uptime_in_seconds: None,
                db_keys: None,
                error: Some(e.to_string()),
            };
        }
    };

    let mut con = match client.get_multiplexed_async_connection().await {
        Ok(c) => c,
        Err(e) => {
            return RedisHealth {
                ok: false,
                latency_ms: start.elapsed().as_millis() as u64,
                used_memory_human: None,
                connected_clients: None,
                uptime_in_seconds: None,
                db_keys: None,
                error: Some(e.to_string()),
            };
        }
    };

    let info_result: Result<String, _> = redis::cmd("INFO").query_async(&mut con).await;
    let dbsize_result: Result<u64, _> = redis::cmd("DBSIZE").query_async(&mut con).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match info_result {
        Ok(info) => {
            let parse = |key: &str| -> Option<String> {
                info.lines()
                    .find(|l| l.starts_with(&format!("{key}:")))
                    .map(|l| l.split_once(':').unwrap().1.trim().to_string())
            };

            RedisHealth {
                ok: true,
                latency_ms,
                used_memory_human: parse("used_memory_human"),
                connected_clients: parse("connected_clients").and_then(|v| v.parse().ok()),
                uptime_in_seconds: parse("uptime_in_seconds").and_then(|v| v.parse().ok()),
                db_keys: dbsize_result.ok(),
                error: None,
            }
        }
        Err(e) => RedisHealth {
            ok: false,
            latency_ms,
            used_memory_human: None,
            connected_clients: None,
            uptime_in_seconds: None,
            db_keys: None,
            error: Some(e.to_string()),
        },
    }
}

pub type SharedSystem = Mutex<sysinfo::System>;

#[tauri::command]
pub async fn get_system_status(
    app_state: State<'_, SharedState>,
    proxy_state: State<'_, ProxyState>,
    connections: State<'_, SharedConnections>,
    system: State<'_, SharedSystem>,
) -> Result<SystemStatusResponse, AppError> {
    // Check if memory (Redis) is enabled, and build a server_id -> name map
    let (server_count, connected_count, memory_enabled, server_names) = {
        let s = app_state.lock().unwrap();
        let total = s.servers.len();
        let connected = s
            .servers
            .iter()
            .filter(|srv| {
                srv.status
                    .as_ref()
                    .map(|st| format!("{st:?}").to_lowercase() == "connected")
                    .unwrap_or(false)
            })
            .count();
        let has_memory = s
            .servers
            .iter()
            .any(|srv| srv.managed.unwrap_or(false) && srv.name == "Memory");
        let names: HashMap<String, String> = s
            .servers
            .iter()
            .map(|srv| (srv.id.clone(), srv.name.clone()))
            .collect();
        (total, connected, has_memory, names)
    };

    // Get PIDs of our managed server processes
    let managed_pids: Vec<(String, u32)> = {
        let conns = connections.lock().await;
        conns.pids()
    };

    // Redis check (only if memory is enabled)
    let redis = if memory_enabled {
        Some(check_redis_health().await)
    } else {
        None
    };

    // Proxy status
    let proxy = ProxyHealth {
        running: proxy_state.is_running().await,
        port: proxy_state.port().await,
    };

    // Process stats — only look up PIDs we actually manage
    let processes = {
        let mut sys = system
            .lock()
            .map_err(|e| AppError::ConnectionFailed(format!("Failed to lock sysinfo: {e}")))?;

        let pids: Vec<sysinfo::Pid> = managed_pids
            .iter()
            .map(|(_, pid)| sysinfo::Pid::from_u32(*pid))
            .collect();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);

        managed_pids
            .iter()
            .filter_map(|(server_id, pid)| {
                let sysinfo_pid = sysinfo::Pid::from_u32(*pid);
                let p = sys.process(sysinfo_pid)?;
                let server_name = server_names
                    .get(server_id)
                    .cloned()
                    .unwrap_or_else(|| server_id.clone());
                let cmd_args: Vec<String> = p
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect();
                let command = if cmd_args.len() > 1 {
                    cmd_args[1..].join(" ")
                } else {
                    p.name().to_string_lossy().to_string()
                };
                Some(ProcessStats {
                    name: server_name,
                    command,
                    pid: *pid,
                    cpu_percent: p.cpu_usage(),
                    memory_bytes: p.memory(),
                })
            })
            .collect()
    };

    let checked_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(SystemStatusResponse {
        proxy,
        redis,
        processes,
        server_count,
        connected_count,
        checked_at,
    })
}
