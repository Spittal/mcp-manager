<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useServersStore } from '@/stores/servers';
import type { AiToolInfo } from '@/types/integration';
import type { ProxyStatus } from '@/types/proxy';

interface MemoryStatus {
  enabled: boolean;
  serverStatus: string | null;
  uvxAvailable: boolean;
  dockerAvailable: boolean;
  redisRunning: boolean;
  ollamaRunning: boolean;
  error: string | null;
}

const store = useServersStore();

const proxyStatus = ref<ProxyStatus | null>(null);
const proxyError = ref<string | null>(null);
const copied = ref(false);

const integrations = ref<AiToolInfo[] | null>(null);
const installedIntegrations = computed(() =>
  integrations.value?.filter(t => t.installed) ?? []
);
const integrationsError = ref<string | null>(null);
const togglingId = ref<string | null>(null);
const importingId = ref<string | null>(null);

const memoryStatus = ref<MemoryStatus | null>(null);
const memoryToggling = ref(false);
const memoryError = ref<string | null>(null);
const memoryProgress = ref<string | null>(null);

let unlistenMemoryProgress: UnlistenFn | null = null;

async function fetchProxyStatus() {
  try {
    proxyStatus.value = await invoke<ProxyStatus>('get_proxy_status');
    proxyError.value = null;
  } catch (e) {
    proxyError.value = String(e);
  }
}

async function fetchIntegrations() {
  try {
    integrations.value = await invoke<AiToolInfo[]>('detect_integrations');
    integrationsError.value = null;
  } catch (e) {
    integrationsError.value = String(e);
  }
}

async function enableTool(id: string) {
  togglingId.value = id;
  try {
    await invoke('enable_integration', { id });
    // Reload servers list and auto-connect imported servers
    await store.loadServers();
    store.autoConnectServers();
    await fetchIntegrations();
  } catch (e) {
    integrationsError.value = String(e);
  } finally {
    togglingId.value = null;
  }
}

async function disableTool(id: string) {
  togglingId.value = id;
  try {
    await invoke('disable_integration', { id });
    await fetchIntegrations();
  } catch (e) {
    integrationsError.value = String(e);
  } finally {
    togglingId.value = null;
  }
}

async function importFromTool(id: string) {
  importingId.value = id;
  integrationsError.value = null;
  try {
    const count = await invoke<number>('import_from_tool', { id });
    if (count > 0) {
      await store.loadServers();
      store.autoConnectServers();
    }
    await fetchIntegrations();
  } catch (e) {
    integrationsError.value = String(e);
  } finally {
    importingId.value = null;
  }
}

function serverSummary(server: AiToolInfo['existingServers'][number]): string {
  if (server.transport === 'http' && server.url) return server.url;
  if (server.command) {
    const parts = [server.command, ...(server.args ?? [])];
    const full = parts.join(' ');
    return full.length > 50 ? full.slice(0, 50) + '...' : full;
  }
  return server.transport;
}

function claudeDesktopSnippet(): string {
  const port = proxyStatus.value?.port ?? 0;
  return JSON.stringify(
    {
      mcpServers: {
        'mcp-manager': {
          url: `http://localhost:${port}/mcp`,
        },
      },
    },
    null,
    2,
  );
}

async function copySnippet() {
  try {
    await navigator.clipboard.writeText(claudeDesktopSnippet());
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  } catch {
    // Clipboard may not be available in some webview contexts
  }
}

async function fetchMemoryStatus() {
  try {
    memoryStatus.value = await invoke<MemoryStatus>('get_memory_status');
    memoryError.value = null;
  } catch (e) {
    memoryError.value = String(e);
  }
}

async function toggleMemory() {
  if (!memoryStatus.value) return;
  memoryToggling.value = true;
  memoryError.value = null;
  memoryProgress.value = null;
  try {
    if (memoryStatus.value.enabled) {
      await invoke('disable_memory');
      await store.loadServers();
    } else {
      await invoke('enable_memory');
      await store.loadServers();
      store.autoConnectServers();
    }
    await fetchMemoryStatus();
  } catch (e) {
    memoryError.value = String(e);
  } finally {
    memoryToggling.value = false;
    memoryProgress.value = null;
  }
}

const canEnableMemory = computed(() => {
  if (!memoryStatus.value) return false;
  return memoryStatus.value.uvxAvailable && memoryStatus.value.dockerAvailable;
});

onMounted(async () => {
  fetchProxyStatus();
  fetchIntegrations();
  fetchMemoryStatus();
  unlistenMemoryProgress = await listen<{ message: string }>('memory-progress', (event) => {
    memoryProgress.value = event.payload.message;
  });
});

onUnmounted(() => {
  unlistenMemoryProgress?.();
});
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Settings</h1>
    </header>
    <div class="flex-1 overflow-y-auto p-4">
      <div class="mx-auto max-w-md space-y-6">
        <!-- Connected Apps -->
        <section>
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">
            Connected Apps
          </h2>
          <p class="mb-3 text-xs text-text-secondary">
            Automatically configure AI tools to use MCP Manager as their MCP server,
            and discover pre-configured MCP servers from installed tools.
          </p>

          <div v-if="integrationsError" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
            {{ integrationsError }}
          </div>

          <div v-if="integrations && installedIntegrations.length" class="space-y-2">
            <div
              v-for="tool in installedIntegrations"
              :key="tool.id"
              class="rounded border border-border bg-surface-1"
            >
              <!-- Tool header row -->
              <div class="flex items-center justify-between px-3 py-2.5">
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    <span
                      class="h-1.5 w-1.5 shrink-0 rounded-full"
                      :class="tool.enabled ? 'bg-status-connected' : 'bg-surface-3'"
                    />
                    <span class="text-xs font-medium text-text-primary">{{ tool.name }}</span>
                  </div>
                  <div class="mt-0.5 pl-3.5">
                    <span v-if="tool.enabled && proxyStatus && tool.configuredPort !== proxyStatus.port"
                          class="text-[10px] text-status-connecting">Port outdated — restart app to fix</span>
                    <span v-else-if="tool.enabled" class="text-[10px] text-text-muted">Port {{ tool.configuredPort }}</span>
                  </div>
                </div>
                <div class="shrink-0 ml-3">
                  <template v-if="tool.supportsProxy">
                    <button
                      v-if="!tool.enabled"
                      class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                      :disabled="togglingId === tool.id || !proxyStatus?.running"
                      @click="enableTool(tool.id)"
                    >
                      {{ tool.existingServers.length ? 'Migrate & Enable' : 'Enable' }}
                    </button>
                    <button
                      v-else
                      class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2 disabled:opacity-50"
                      :disabled="togglingId === tool.id"
                      @click="disableTool(tool.id)"
                    >
                      Disable
                    </button>
                  </template>
                  <button
                    v-else-if="tool.existingServers.length"
                    class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                    :disabled="importingId === tool.id"
                    @click="importFromTool(tool.id)"
                  >
                    {{ importingId === tool.id ? 'Importing...' : 'Import' }}
                  </button>
                  <span v-else class="text-[10px] text-text-muted">No servers</span>
                </div>
              </div>

              <!-- Existing servers -->
              <div v-if="tool.existingServers.length && (!tool.enabled || !tool.supportsProxy)" class="border-t border-border/50 px-3 py-2">
                <p class="mb-1.5 text-[10px] font-medium text-text-muted uppercase tracking-wide">
                  {{ tool.supportsProxy ? 'Existing MCP servers to import' : 'Configured MCP servers' }}
                </p>
                <div class="space-y-1">
                  <div
                    v-for="server in tool.existingServers"
                    :key="server.name"
                    class="flex items-center gap-2 rounded bg-surface-0 px-2 py-1.5"
                  >
                    <span class="font-mono text-[11px] font-medium text-text-secondary">{{ server.name }}</span>
                    <span class="truncate text-[10px] text-text-muted">{{ serverSummary(server) }}</span>
                  </div>
                </div>
                <p v-if="tool.supportsProxy" class="mt-1.5 text-[10px] text-text-muted">
                  These will be imported into MCP Manager and managed through the proxy.
                </p>
              </div>
            </div>
          </div>

          <div v-else-if="integrations && !installedIntegrations.length" class="text-xs text-text-muted">
            No supported AI tools detected. Install one of the supported tools to get started.
          </div>

          <div v-else class="text-xs text-text-muted">Detecting installed tools...</div>
        </section>

        <!-- Memory -->
        <section>
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">
            Memory
          </h2>
          <p class="mb-3 text-xs text-text-secondary">
            Shared long-term memory across all connected AI tools.
            Uses local Ollama models for embeddings — no API keys needed. Requires Docker.
          </p>

          <div v-if="memoryError" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
            {{ memoryError }}
          </div>

          <div v-if="memoryStatus" class="rounded border border-border bg-surface-1">
            <div class="flex items-center justify-between px-3 py-2.5">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span
                    class="h-1.5 w-1.5 shrink-0 rounded-full"
                    :class="memoryStatus.enabled ? 'bg-status-connected' : 'bg-surface-3'"
                  />
                  <span class="text-xs font-medium text-text-primary">Agent Memory</span>
                </div>
                <div class="mt-0.5 pl-3.5">
                  <span v-if="memoryToggling && memoryProgress" class="flex items-center gap-1.5 text-[10px] text-text-secondary">
                    <span class="h-1 w-1 animate-pulse rounded-full bg-accent" />
                    {{ memoryProgress }}
                  </span>
                  <span v-else-if="memoryStatus.enabled && memoryStatus.serverStatus === 'connected'" class="text-[10px] text-text-muted">Connected</span>
                  <span v-else-if="memoryStatus.enabled && memoryStatus.serverStatus === 'connecting'" class="text-[10px] text-status-connecting">Connecting...</span>
                  <span v-else-if="memoryStatus.enabled" class="text-[10px] text-text-muted">Enabled</span>
                </div>
              </div>
              <div class="shrink-0 ml-3">
                <button
                  v-if="!memoryStatus.enabled"
                  class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                  :disabled="memoryToggling || !canEnableMemory"
                  @click="toggleMemory"
                >
                  Enable
                </button>
                <button
                  v-else
                  class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2 disabled:opacity-50"
                  :disabled="memoryToggling"
                  @click="toggleMemory"
                >
                  Disable
                </button>
              </div>
            </div>

            <!-- Prerequisite warnings -->
            <div v-if="!memoryStatus.enabled && (!memoryStatus.uvxAvailable || !memoryStatus.dockerAvailable)" class="border-t border-border/50 px-3 py-2 space-y-1">
              <div v-if="!memoryStatus.uvxAvailable" class="flex items-center gap-1.5 text-[11px] text-status-connecting">
                <span>&#9888;</span>
                <span>uvx not found — install with: <code class="font-mono bg-surface-2 px-1 rounded">brew install uv</code></span>
              </div>
              <div v-if="!memoryStatus.dockerAvailable" class="flex items-center gap-1.5 text-[11px] text-status-connecting">
                <span>&#9888;</span>
                <span>Docker not found</span>
              </div>
            </div>
          </div>

          <div v-else class="text-xs text-text-muted">Loading memory status...</div>
        </section>

        <!-- Proxy -->
        <section>
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">
            Proxy
          </h2>
          <p class="mb-3 text-xs text-text-secondary">
            MCP Manager exposes all connected servers as a single MCP proxy endpoint.
            Use the Connected Apps toggles above, or manually add the config snippet below.
          </p>

          <div v-if="proxyError" class="rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
            Failed to get proxy status: {{ proxyError }}
          </div>

          <div v-else-if="proxyStatus" class="space-y-3">
            <div class="flex items-center gap-2">
              <span
                class="inline-block h-2 w-2 rounded-full"
                :class="proxyStatus.running ? 'bg-status-connected' : 'bg-status-error'"
              />
              <span class="text-xs text-text-secondary">
                <template v-if="proxyStatus.running">
                  Running on port <span class="font-mono font-medium text-text-primary">{{ proxyStatus.port }}</span>
                </template>
                <template v-else>
                  Not running
                </template>
              </span>
            </div>

            <div v-if="proxyStatus.running">
              <p class="mb-1.5 text-xs text-text-muted">
                Manual config snippet:
              </p>
              <div class="relative">
                <pre class="overflow-x-auto rounded bg-surface-2 p-3 font-mono text-xs text-text-primary">{{ claudeDesktopSnippet() }}</pre>
                <button
                  class="absolute top-1.5 right-1.5 rounded bg-surface-3 px-2 py-0.5 text-[10px] text-text-muted transition hover:text-text-primary"
                  @click="copySnippet"
                >
                  {{ copied ? 'Copied' : 'Copy' }}
                </button>
              </div>
            </div>
          </div>

          <div v-else class="text-xs text-text-muted">Loading proxy status...</div>
        </section>
      </div>
    </div>
  </div>
</template>
