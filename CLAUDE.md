# MCP Manager - Project Guidelines

## Project Overview

A native macOS desktop application (Tauri v2 + Vue 3) that provides a GUI for managing MCP (Model Context Protocol) servers. The target audience is non-technical co-workers who use AI agents for project management, sales, etc. and need to connect to multiple MCP servers without dealing with npm, CLI configuration, or OAuth complexity.

## Tech Stack

- **Desktop Framework**: Tauri v2 (latest stable)
- **Frontend**: Vue 3 + Vite + TypeScript (SPA, not Nuxt — desktop apps don't need SSR/SSG complexity)
- **State Management**: Pinia (with `tauri-plugin-pinia` for persistence across restarts)
- **UI**: TBD (consider Radix Vue / shadcn-vue for accessible, unstyled components)
- **Backend**: Rust (Tauri core process)
- **Package Manager**: pnpm

## Tauri v2 Architecture Reference

### Process Model
Tauri uses a multi-process architecture:
- **Core Process** (Rust): Full OS access, manages windows/trays/IPC, holds all state and secrets. Business logic lives here.
- **WebView Process** (Vue app): Renders UI via system webview (WKWebView on macOS). Has NO direct OS access — communicates only via IPC.

Security principle: **Principle of Least Privilege**. The frontend should never handle secrets. All sensitive operations go through the Rust backend.

### IPC: Commands & Events

**Commands** (frontend → backend, request/response):
```rust
// src-tauri/src/commands.rs
#[tauri::command]
async fn list_servers(state: State<'_, Mutex<AppState>>) -> Result<Vec<ServerConfig>, String> {
    let state = state.lock().unwrap();
    Ok(state.servers.clone())
}
```
```typescript
// frontend
import { invoke } from '@tauri-apps/api/core';
const servers = await invoke<ServerConfig[]>('list_servers');
```

- Arguments auto-deserialize from JSON (must impl `serde::Deserialize`)
- Return types auto-serialize (must impl `serde::Serialize`)
- Frontend sends camelCase, Rust receives snake_case by default
- Use `#[tauri::command(rename_all = "snake_case")]` to change
- Async commands run on separate task (won't block UI)
- Async commands CANNOT use borrowed types (`&str`, `State<'_, T>`) — use owned types or wrap return in `Result`

**Events** (bidirectional, fire-and-forget, no return value):
```rust
use tauri::Emitter;
app.emit("server-status-changed", payload)?;
```
```typescript
import { listen } from '@tauri-apps/api/event';
const unlisten = await listen('server-status-changed', (event) => { ... });
// Always unlisten on component unmount!
```

**Channels** (backend → frontend streaming, for large/continuous data):
```rust
#[tauri::command]
async fn stream_logs(server_id: String, channel: tauri::ipc::Channel<String>) {
    // Send log lines as they arrive
    channel.send("log line here".into()).unwrap();
}
```

### State Management (Rust side)
```rust
use std::sync::Mutex;

struct AppState {
    servers: Vec<ServerConfig>,
}

// In setup:
app.manage(Mutex::new(AppState { servers: vec![] }));

// In commands:
fn my_cmd(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    // mutate s
    Ok(())
}
```
- Tauri wraps managed state in `Arc` automatically — no need to add your own
- Use `std::sync::Mutex` for most cases; `tokio::Mutex` only if holding lock across `.await` points

### Permissions & Capabilities
Tauri v2 uses a capabilities-based security model:
- Capabilities defined in `src-tauri/capabilities/*.json`
- Each capability grants specific permissions to specific windows
- Plugin commands require explicit permission grants
- Auto-generated `allow-*` / `deny-*` permissions for each command

Example (`src-tauri/capabilities/default.json`):
```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-spawn",
    "shell:allow-kill",
    "shell:allow-stdin-write",
    "store:default",
    "http:default"
  ]
}
```

### Key Official Plugins We'll Use
| Plugin | Purpose | Crate |
|--------|---------|-------|
| **shell** | Spawn/manage MCP server processes (stdio transport) | `tauri-plugin-shell` |
| **store** | Persistent key-value config storage | `tauri-plugin-store` |
| **http** | HTTP requests for streamable HTTP transport & OAuth | `tauri-plugin-http` |
| **fs** | Read/write config files | `tauri-plugin-fs` |
| **process** | App lifecycle management | `tauri-plugin-process` |

For secrets (OAuth tokens), use `tauri-plugin-keyring` (community, wraps OS keychain) since Stronghold is deprecated.

### Sidecar & Process Management
For stdio-based MCP servers, use the shell plugin:
```rust
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;

let (mut rx, child) = app.shell()
    .command("node")
    .args(["path/to/mcp-server.js"])
    .spawn()
    .expect("failed to spawn");

// Handle stdout/stderr in async task
tauri::async_runtime::spawn(async move {
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => { /* parse JSON-RPC */ }
            CommandEvent::Stderr(bytes) => { /* log */ }
            CommandEvent::Terminated(status) => { /* handle exit */ }
            _ => {}
        }
    }
});
```

Binary naming for sidecars requires platform-specific suffixes (e.g., `my-binary-aarch64-apple-darwin`).

### Project Structure Convention
```
mcp-manager/
├── src/                    # Vue 3 frontend
│   ├── assets/
│   ├── components/
│   ├── composables/        # Vue composables (useServers, useMcpClient, etc.)
│   ├── stores/             # Pinia stores
│   ├── views/
│   ├── types/              # TypeScript types/interfaces
│   ├── App.vue
│   └── main.ts
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── commands/       # Tauri command handlers (organized by domain)
│   │   ├── mcp/            # MCP protocol implementation
│   │   │   ├── client.rs   # MCP client (JSON-RPC over stdio/HTTP)
│   │   │   ├── transport.rs # Transport abstractions
│   │   │   └── types.rs    # MCP protocol types
│   │   ├── state.rs        # Application state definitions
│   │   ├── error.rs        # Error types (use thiserror)
│   │   └── lib.rs          # Tauri app entry point
│   ├── capabilities/       # Security permissions
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
├── tsconfig.json
├── vite.config.ts
└── CLAUDE.md
```

## MCP Protocol Reference

### Transports
1. **stdio** (primary for local servers): Client spawns server as subprocess, communicates via stdin/stdout with newline-delimited JSON-RPC 2.0 messages.
2. **Streamable HTTP** (for remote servers, replaces deprecated SSE): Server exposes single HTTP endpoint, client POSTs JSON-RPC messages, server responds with JSON or SSE stream. Supports session management via `Mcp-Session-Id` header.
3. **Legacy HTTP+SSE** (deprecated but still in use): Some servers still use this. Client should implement backwards-compatible detection.

### OAuth 2.1 in MCP
The MCP spec (2025-06-18 revision) includes OAuth 2.1 authorization. For desktop apps:
- Use `tauri-plugin-oauth` to spawn temporary localhost server for redirect capture
- Implement PKCE flow (required for public clients)
- Store tokens securely in OS keychain via `tauri-plugin-keyring`
- Handle token refresh transparently

### JSON-RPC 2.0 Message Format
All MCP communication uses JSON-RPC 2.0:
```json
// Request
{"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
// Response
{"jsonrpc": "2.0", "id": 1, "result": {"tools": [...]}}
// Notification (no id, no response expected)
{"jsonrpc": "2.0", "method": "notifications/initialized"}
```

### MCP Lifecycle
1. Client sends `initialize` request with protocol version and capabilities
2. Server responds with its capabilities
3. Client sends `notifications/initialized`
4. Normal operation (list tools, call tools, list resources, etc.)
5. Client sends disconnect or closes transport

## Existing Prior Art
- **sublayerapp/tauri-plugin-mcp-client**: Tauri plugin for MCP client connections. Stdio-only, no OAuth, no persistence. Good reference for JSON-RPC/process management patterns.
- **mcp-router/mcp-router**: Electron-based MCP manager with projects/workspaces. TypeScript. Good UX reference for server toggling and organization.
- **vlazic/mcp-server-manager**: Go + HTMX single-binary. Cross-platform config file management.

## Development Commands
```bash
# Create project
npm create tauri-app@latest mcp-manager -- --template vue-ts

# Development
pnpm tauri dev          # Start dev server with hot reload

# Build
pnpm tauri build        # Production build (creates .dmg/.app on macOS)

# Add Tauri plugins
pnpm tauri add shell
pnpm tauri add store
pnpm tauri add http
pnpm tauri add fs
```

## Key Design Decisions

1. **Vue + Vite over Nuxt**: Desktop app is a SPA. Nuxt adds SSR/SSG complexity that Tauri explicitly doesn't support. Plain Vite is the officially recommended approach for SPA frameworks.

2. **Rust for MCP protocol handling**: All MCP client logic (JSON-RPC parsing, transport management, process lifecycle) lives in Rust. The Vue frontend only renders state and dispatches actions via `invoke()`.

3. **Process-per-server model**: Each MCP server connection runs as a managed child process (stdio) or HTTP client (streamable HTTP). The Rust backend tracks all processes and handles cleanup on app exit.

4. **Config format**: Store server configurations in a JSON file via `tauri-plugin-store`. Format should be compatible with Claude Desktop's `claude_desktop_config.json` for easy import/export.

5. **Security-first OAuth**: OAuth tokens stored in OS keychain, never in plaintext config. OAuth redirect handled via localhost callback server.

## Coding Conventions

### Rust
- Use `thiserror` for error types, implement `Serialize` for IPC
- Organize commands by domain in `src-tauri/src/commands/`
- All commands are async unless trivially simple
- State accessed via `State<'_, Mutex<AppState>>` pattern
- Use `std::sync::Mutex` for serializable state (`AppState`), `tokio::sync::Mutex` only when holding lock across `.await` points (`McpConnections`, `OAuthStore`)
- Never use bare `.unwrap()` on fallible operations — use `.expect("reason")` or proper error handling
- Keep `AppError` variants minimal — only add variants that are actively used by commands
- Remove dead code promptly; run `cargo check` to verify zero warnings before committing
- **Serde camelCase for IPC structs**: Tauri only auto-converts snake_case → camelCase for command *arguments* (JS → Rust). Return values use serde as-is. Any struct returned to the frontend **must** have `#[serde(rename_all = "camelCase")]` so field names match the TypeScript interfaces. Without this, the JS side sees `undefined` for multi-word fields (e.g., `existing_servers` vs `existingServers`), causing silent runtime errors.

### TypeScript/Vue
- Composition API with `<script setup lang="ts">`
- Pinia stores in `src/stores/`
- Tauri IPC calls wrapped in composables (`src/composables/`)
- Strong typing — define interfaces for all IPC payloads in `src/types/`
- Always `unlisten()` event listeners on component unmount
- Extract shared UI logic into components (`src/components/`) — e.g., `ServerForm.vue` for add/edit views
- Extract shared helper functions into composables — e.g., `useServerStatus.ts` for status display logic
- Keep frontend types in sync with Rust structs — don't define fields the backend never sends

### Naming
- Rust crate: `mcp-manager` (binary) / commands in snake_case
- Vue components: PascalCase filenames
- Pinia stores: `useXxxStore` pattern
- Tauri commands: snake_case in Rust, camelCase when invoked from JS
- Events: kebab-case (`server-status-changed`)
- Types: one file per domain in `src/types/` (server.ts, oauth.ts, proxy.ts, integration.ts, mcp.ts)

## References
- Tauri v2 Docs: https://v2.tauri.app/
- Tauri Architecture: https://v2.tauri.app/concept/architecture/
- Tauri Process Model: https://v2.tauri.app/concept/process-model/
- Tauri Commands: https://v2.tauri.app/develop/calling-rust/
- Tauri State Management: https://v2.tauri.app/develop/state-management/
- Tauri Plugins: https://v2.tauri.app/plugin/
- Tauri Security/Capabilities: https://v2.tauri.app/security/capabilities/
- Tauri Sidecar Guide: https://v2.tauri.app/develop/sidecar/
- Tauri Shell Plugin: https://v2.tauri.app/plugin/shell/
- Tauri Plugin Development: https://v2.tauri.app/develop/plugins/
- MCP Specification (Transports): https://modelcontextprotocol.io/specification/2025-03-26/basic/transports
- MCP OAuth: https://modelcontextprotocol.io/specification/2025-03-26/basic/authorization
- tauri-plugin-mcp-client: https://github.com/sublayerapp/tauri-plugin-mcp-client
- tauri-plugin-oauth: https://github.com/FabianLars/tauri-plugin-oauth
- tauri-plugin-keyring: https://github.com/HuakunShen/tauri-plugin-keyring
- tauri-plugin-pinia: https://crates.io/crates/tauri-plugin-pinia
- awesome-tauri: https://github.com/tauri-apps/awesome-tauri
