<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useServersStore } from '@/stores/servers';
import type { AiToolInfo } from '@/types/integration';
import type { ProxyStatus } from '@/types/proxy';
import type { EmbeddingConfigStatus, EmbeddingProvider, EmbeddingModelInfo } from '@/types/embedding';
import { OLLAMA_MODELS, OPENAI_MODELS } from '@/types/embedding';

interface MemoryStatus {
  enabled: boolean;
  serverStatus: string | null;
  uvxAvailable: boolean;
  dockerAvailable: boolean;
  redisRunning: boolean;
  ollamaRunning: boolean;
  embeddingProvider: string;
  embeddingModel: string;
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

const memoryStatus = ref<MemoryStatus | null>(null);
const memoryToggling = ref(false);
const memoryError = ref<string | null>(null);
const memoryProgress = ref<string | null>(null);

// Embedding config state
const embeddingStatus = ref<EmbeddingConfigStatus | null>(null);
const embeddingProvider = ref<EmbeddingProvider>('ollama');
const embeddingModel = ref('mxbai-embed-large');
const embeddingDimensions = ref(1024);
const embeddingCustomModel = ref(false);
const embeddingCustomName = ref('');
const embeddingCustomDims = ref(512);
const openaiApiKey = ref('');
const showApiKey = ref(false);
const embeddingSaving = ref(false);
const embeddingSaved = ref(false);

const currentModels = computed<EmbeddingModelInfo[]>(() =>
  embeddingProvider.value === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS
);

const embeddingDirty = computed(() => {
  if (!embeddingStatus.value) return false;
  const saved = embeddingStatus.value.config;
  const currentModel = embeddingCustomModel.value ? embeddingCustomName.value : embeddingModel.value;
  const currentDims = embeddingCustomModel.value ? embeddingCustomDims.value : embeddingDimensions.value;
  return (
    embeddingProvider.value !== saved.provider ||
    currentModel !== saved.model ||
    currentDims !== saved.dimensions ||
    (embeddingProvider.value === 'openai' && openaiApiKey.value.length > 0)
  );
});

const embeddingModelChanged = computed(() => {
  if (!memoryStatus.value?.enabled || !embeddingStatus.value) return false;
  const saved = embeddingStatus.value.config;
  const currentModel = embeddingCustomModel.value ? embeddingCustomName.value : embeddingModel.value;
  const currentDims = embeddingCustomModel.value ? embeddingCustomDims.value : embeddingDimensions.value;
  return (
    embeddingProvider.value !== saved.provider ||
    currentModel !== saved.model ||
    currentDims !== saved.dimensions
  );
});

const memoryDescription = computed(() => {
  if (embeddingProvider.value === 'openai') {
    return 'Shared long-term memory across all connected AI tools. Uses OpenAI API for embeddings. Requires Docker (for Redis) and an API key.';
  }
  return 'Shared long-term memory across all connected AI tools. Uses local Ollama models for embeddings — no API keys needed. Requires Docker.';
});

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

function serverSummary(server: AiToolInfo['existingServers'][number]): string {
  if (server.transport === 'http' && server.url) return server.url;
  if (server.command) {
    const parts = [server.command, ...(server.args ?? [])];
    const full = parts.join(' ');
    return full.length > 50 ? full.slice(0, 50) + '...' : full;
  }
  return server.transport;
}

function manualSnippet(): string {
  const port = proxyStatus.value?.port ?? 0;
  const servers = store.servers.filter(s => s.status === 'connected');
  const mcpServers: Record<string, { url: string }> = {};
  for (const s of servers) {
    mcpServers[s.name] = { url: `http://localhost:${port}/mcp/${s.id}` };
  }
  if (Object.keys(mcpServers).length === 0) {
    mcpServers['your-server'] = { url: `http://localhost:${port}/mcp/<server-id>` };
  }
  return JSON.stringify({ mcpServers }, null, 2);
}

async function copySnippet() {
  try {
    await navigator.clipboard.writeText(manualSnippet());
    copied.value = true;
    setTimeout(() => (copied.value = false), 2000);
  } catch {
    // Clipboard may not be available in some webview contexts
  }
}

async function fetchEmbeddingConfig() {
  try {
    embeddingStatus.value = await invoke<EmbeddingConfigStatus>('get_embedding_config');
    const cfg = embeddingStatus.value.config;
    embeddingProvider.value = cfg.provider;

    // Check if model is in the curated list
    const models = cfg.provider === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS;
    const known = models.find(m => m.model === cfg.model);
    if (known) {
      embeddingModel.value = known.model;
      embeddingDimensions.value = known.dimensions;
      embeddingCustomModel.value = false;
    } else {
      embeddingCustomModel.value = true;
      embeddingCustomName.value = cfg.model;
      embeddingCustomDims.value = cfg.dimensions;
    }
  } catch {
    // Non-critical, defaults will be used
  }
}

function selectModel(model: EmbeddingModelInfo) {
  embeddingModel.value = model.model;
  embeddingDimensions.value = model.dimensions;
  embeddingCustomModel.value = false;
  embeddingSaved.value = false;
}

function selectCustomModel() {
  embeddingCustomModel.value = true;
  embeddingSaved.value = false;
}

function switchProvider(provider: EmbeddingProvider) {
  embeddingProvider.value = provider;
  embeddingCustomModel.value = false;
  embeddingSaved.value = false;
  const models = provider === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS;
  const recommended = models.find(m => m.recommended) ?? models[0];
  embeddingModel.value = recommended.model;
  embeddingDimensions.value = recommended.dimensions;
}

async function saveEmbeddingSettings() {
  embeddingSaving.value = true;
  embeddingSaved.value = false;
  try {
    const model = embeddingCustomModel.value ? embeddingCustomName.value : embeddingModel.value;
    const dimensions = embeddingCustomModel.value ? embeddingCustomDims.value : embeddingDimensions.value;
    await invoke('save_embedding_config_cmd', {
      input: {
        config: { provider: embeddingProvider.value, model, dimensions },
        openaiApiKey: embeddingProvider.value === 'openai' && openaiApiKey.value ? openaiApiKey.value : null,
      },
    });
    openaiApiKey.value = '';
    embeddingSaved.value = true;
    await fetchEmbeddingConfig();
    await fetchMemoryStatus();
  } catch (e) {
    memoryError.value = String(e);
  } finally {
    embeddingSaving.value = false;
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
  fetchEmbeddingConfig();
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
            Automatically configure AI tools to use MCP Manager as their MCP server.
            Supports Claude Code, Cursor, Claude Desktop, and Windsurf.
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
                </div>
              </div>

              <!-- Existing servers to migrate -->
              <div v-if="!tool.enabled && tool.existingServers.length" class="border-t border-border/50 px-3 py-2">
                <p class="mb-1.5 text-[10px] font-medium text-text-muted uppercase tracking-wide">
                  Existing MCP servers to import
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
                <p class="mt-1.5 text-[10px] text-text-muted">
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
            {{ memoryDescription }}
          </p>

          <div v-if="memoryError" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
            {{ memoryError }}
          </div>

          <div v-if="memoryStatus" class="space-y-3">
            <!-- Enable/Disable toggle card -->
            <div class="rounded border border-border bg-surface-1">
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
              <div v-if="!memoryStatus.enabled && (!memoryStatus.uvxAvailable || !memoryStatus.dockerAvailable || (embeddingProvider === 'ollama' && !memoryStatus.ollamaRunning))" class="border-t border-border/50 px-3 py-2 space-y-1">
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

            <!-- Embedding config panel -->
            <div class="rounded border border-border bg-surface-1">
              <div class="px-3 py-2.5">
                <span class="text-[11px] font-medium text-text-secondary">Embedding Model</span>
              </div>

              <!-- Provider tabs -->
              <div class="flex border-t border-border/50">
                <button
                  class="flex-1 py-1.5 text-[11px] font-medium transition-colors"
                  :class="embeddingProvider === 'ollama'
                    ? 'bg-surface-2 text-text-primary'
                    : 'text-text-muted hover:text-text-secondary'"
                  @click="switchProvider('ollama')"
                >
                  Ollama
                </button>
                <button
                  class="flex-1 py-1.5 text-[11px] font-medium transition-colors border-l border-border/50"
                  :class="embeddingProvider === 'openai'
                    ? 'bg-surface-2 text-text-primary'
                    : 'text-text-muted hover:text-text-secondary'"
                  @click="switchProvider('openai')"
                >
                  OpenAI
                </button>
              </div>

              <div class="border-t border-border/50 px-3 py-2.5 space-y-2">
                <!-- Model list -->
                <div class="space-y-1">
                  <div
                    v-for="m in currentModels"
                    :key="m.model"
                    class="flex items-center gap-2 rounded px-2 py-1.5 cursor-pointer transition-colors"
                    :class="!embeddingCustomModel && embeddingModel === m.model
                      ? 'bg-accent/10 ring-1 ring-accent/30'
                      : 'hover:bg-surface-2'"
                    @click="selectModel(m)"
                  >
                    <span class="flex items-center gap-1.5 min-w-0 flex-1">
                      <span
                        class="h-3 w-3 shrink-0 rounded-full border-2 flex items-center justify-center"
                        :class="!embeddingCustomModel && embeddingModel === m.model
                          ? 'border-accent'
                          : 'border-surface-3'"
                      >
                        <span
                          v-if="!embeddingCustomModel && embeddingModel === m.model"
                          class="h-1.5 w-1.5 rounded-full bg-accent"
                        />
                      </span>
                      <span class="font-mono text-[11px] text-text-primary">{{ m.displayName }}</span>
                      <span v-if="m.recommended" class="text-[9px] font-medium text-accent uppercase tracking-wide">Recommended</span>
                      <span
                        v-if="embeddingProvider === 'ollama' && embeddingStatus?.pulledOllamaModels.includes(m.model)"
                        class="text-[9px] text-status-connected"
                        title="Already downloaded"
                      >&#10003;</span>
                    </span>
                    <span class="shrink-0 text-[10px] text-text-muted">{{ m.dimensions }}d &middot; {{ m.sizeLabel }}</span>
                  </div>

                  <!-- Custom model option (Ollama only) -->
                  <div
                    v-if="embeddingProvider === 'ollama'"
                    class="flex items-center gap-2 rounded px-2 py-1.5 cursor-pointer transition-colors"
                    :class="embeddingCustomModel
                      ? 'bg-accent/10 ring-1 ring-accent/30'
                      : 'hover:bg-surface-2'"
                    @click="selectCustomModel"
                  >
                    <span
                      class="h-3 w-3 shrink-0 rounded-full border-2 flex items-center justify-center"
                      :class="embeddingCustomModel ? 'border-accent' : 'border-surface-3'"
                    >
                      <span v-if="embeddingCustomModel" class="h-1.5 w-1.5 rounded-full bg-accent" />
                    </span>
                    <span class="text-[11px] text-text-secondary">Custom model...</span>
                  </div>
                </div>

                <!-- Custom model inputs -->
                <div v-if="embeddingCustomModel && embeddingProvider === 'ollama'" class="flex gap-2 pl-5">
                  <input
                    v-model="embeddingCustomName"
                    type="text"
                    placeholder="Model name (e.g. bge-m3)"
                    class="flex-1 rounded border border-border bg-surface-0 px-2 py-1.5 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
                  />
                  <input
                    v-model.number="embeddingCustomDims"
                    type="number"
                    min="1"
                    placeholder="Dims"
                    class="w-20 rounded border border-border bg-surface-0 px-2 py-1.5 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
                  />
                </div>

                <!-- OpenAI API key input -->
                <div v-if="embeddingProvider === 'openai'" class="space-y-1.5">
                  <div class="flex items-center gap-2">
                    <div class="relative flex-1">
                      <input
                        v-model="openaiApiKey"
                        :type="showApiKey ? 'text' : 'password'"
                        placeholder="sk-..."
                        class="w-full rounded border border-border bg-surface-0 px-2 py-1.5 pr-14 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
                      />
                      <button
                        class="absolute top-1/2 right-2 -translate-y-1/2 text-[10px] text-text-muted hover:text-text-secondary"
                        @click="showApiKey = !showApiKey"
                      >
                        {{ showApiKey ? 'Hide' : 'Show' }}
                      </button>
                    </div>
                  </div>
                  <div class="flex items-center gap-2 text-[10px]">
                    <span v-if="embeddingStatus?.hasOpenaiKey" class="text-status-connected">&#10003; Key saved</span>
                    <span v-else class="text-text-muted">No API key saved</span>
                    <span class="text-text-muted">&middot;</span>
                    <a href="https://platform.openai.com/api-keys" target="_blank" class="text-accent hover:underline">
                      Get an API key
                    </a>
                  </div>
                </div>

                <!-- Model change warning -->
                <div v-if="embeddingModelChanged" class="rounded bg-status-connecting/10 px-2.5 py-2 text-[11px] text-status-connecting">
                  Changing the embedding model requires disabling and re-enabling Memory.
                  Existing memories indexed with a different model won't be searchable.
                </div>

                <!-- Save button -->
                <div class="flex items-center gap-2 pt-1">
                  <button
                    class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                    :disabled="!embeddingDirty || embeddingSaving"
                    @click="saveEmbeddingSettings"
                  >
                    {{ embeddingSaving ? 'Saving...' : 'Save' }}
                  </button>
                  <span v-if="embeddingSaved && !embeddingDirty" class="text-[10px] text-status-connected">&#10003; Saved</span>
                </div>
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
            Each connected server gets its own proxy endpoint, so AI tools see them individually.
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
                <pre class="overflow-x-auto rounded bg-surface-2 p-3 font-mono text-xs text-text-primary">{{ manualSnippet() }}</pre>
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
