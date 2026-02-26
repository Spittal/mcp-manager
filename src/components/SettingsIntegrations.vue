<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useServersStore } from '@/stores/servers';
import type { AiToolInfo } from '@/types/integration';
import type { ProxyStatus } from '@/types/proxy';

const store = useServersStore();

const integrations = ref<AiToolInfo[] | null>(null);
const proxyStatus = ref<ProxyStatus | null>(null);
const error = ref<string | null>(null);
const togglingId = ref<string | null>(null);
const confirmingId = ref<string | null>(null);

const installedTools = computed(() =>
  integrations.value?.filter(t => t.installed) ?? []
);

async function fetchIntegrations() {
  try {
    integrations.value = await invoke<AiToolInfo[]>('detect_integrations');
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}

async function fetchProxyStatus() {
  try {
    proxyStatus.value = await invoke<ProxyStatus>('get_proxy_status');
  } catch {
    // Non-critical
  }
}

async function migrateAndEnable(tool: AiToolInfo) {
  togglingId.value = tool.id;
  confirmingId.value = null;
  try {
    await invoke('enable_integration', { id: tool.id });
    await store.loadServers();
    store.autoConnectServers();
    await fetchIntegrations();
  } catch (e) {
    error.value = String(e);
  } finally {
    togglingId.value = null;
  }
}

async function disable(tool: AiToolInfo) {
  togglingId.value = tool.id;
  try {
    await invoke('disable_integration', { id: tool.id });
    await fetchIntegrations();
  } catch (e) {
    error.value = String(e);
  } finally {
    togglingId.value = null;
  }
}

function serverSummary(server: AiToolInfo['existingServers'][number]): string {
  if (server.transport === 'http' && server.url) return server.url;
  if (server.command) {
    const parts = [server.command, ...(server.args ?? [])];
    const full = parts.join(' ');
    return full.length > 60 ? full.slice(0, 60) + '...' : full;
  }
  return server.transport;
}

function isBusy(tool: AiToolInfo): boolean {
  return togglingId.value === tool.id;
}

function willOverwrite(serverName: string): string | null {
  const existing = store.servers.find(s => s.name === serverName);
  return existing ? existing.name : null;
}

function hasOverwrites(tool: AiToolInfo): boolean {
  return tool.existingServers.some(s => willOverwrite(s.name));
}

onMounted(() => {
  fetchIntegrations();
  fetchProxyStatus();
});
</script>

<template>
  <div>
    <h2 class="mb-1 text-xs font-medium text-text-primary">Managed MCP Configs</h2>
    <p class="mb-4 text-xs text-text-secondary">
      Discover MCP servers configured in your AI tools and import them into Agent Hub.
    </p>

    <div v-if="error" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
      {{ error }}
    </div>

    <div v-if="!integrations" class="text-xs text-text-muted">Scanning config files...</div>

    <template v-if="integrations">
      <div v-if="installedTools.length" class="space-y-5">
        <div v-for="tool in installedTools" :key="tool.id">
          <h3 class="mb-2 font-mono text-[10px] font-medium tracking-wide text-text-muted uppercase">
            {{ tool.name }}
          </h3>
          <div class="rounded border border-border bg-surface-1">
            <div class="flex items-center justify-between px-3 py-2.5">
              <div class="min-w-0">
                <span class="truncate text-[10px] text-text-muted">{{ tool.configPath }}</span>
                <!-- Port status for enabled tools -->
                <div v-if="tool.enabled && tool.configuredPort" class="mt-0.5">
                  <span
                    v-if="proxyStatus && tool.configuredPort !== proxyStatus.port"
                    class="text-[10px] text-status-connecting"
                  >Port outdated — restart app to fix</span>
                  <span v-else class="text-[10px] text-text-muted">Proxy port {{ tool.configuredPort }}</span>
                </div>
              </div>
              <div class="shrink-0 ml-3 flex items-center gap-2">
                <!-- Managed badge -->
                <span
                  v-if="tool.enabled"
                  class="inline-flex items-center gap-1 rounded bg-status-connected/10 px-2 py-1 text-[11px] font-medium text-status-connected"
                >
                  <span class="h-1.5 w-1.5 rounded-full bg-status-connected" />
                  Managed
                </span>
                <!-- Migrate & Enable: shown when servers exist and tool is not yet enabled -->
                <template v-else-if="tool.existingServers.length">
                  <template v-if="confirmingId === tool.id">
                    <button
                      class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                      :disabled="isBusy(tool) || !(proxyStatus?.running ?? false)"
                      @click="migrateAndEnable(tool)"
                    >
                      {{ isBusy(tool) ? 'Migrating...' : 'Confirm' }}
                    </button>
                    <button
                      v-if="!isBusy(tool)"
                      class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2"
                      @click="confirmingId = null"
                    >
                      Cancel
                    </button>
                  </template>
                  <button
                    v-else
                    class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                    :disabled="isBusy(tool) || !(proxyStatus?.running ?? false)"
                    @click="hasOverwrites(tool) ? confirmingId = tool.id : migrateAndEnable(tool)"
                  >
                    Migrate & Enable
                  </button>
                </template>
                <!-- Enable (no servers to migrate) -->
                <button
                  v-else-if="!tool.enabled"
                  class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
                  :disabled="isBusy(tool) || !(proxyStatus?.running ?? false)"
                  @click="migrateAndEnable(tool)"
                >
                  {{ isBusy(tool) ? 'Enabling...' : 'Enable' }}
                </button>
                <!-- Disable -->
                <button
                  v-if="tool.enabled"
                  class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2 disabled:opacity-50"
                  :disabled="isBusy(tool)"
                  @click="disable(tool)"
                >
                  Disable
                </button>
              </div>
            </div>

            <!-- Cursor quirk notice -->
            <div v-if="tool.id === 'cursor' && tool.enabled" class="border-t border-border/50 px-3 py-2">
              <p class="text-[11px] text-text-muted leading-relaxed">
                <span class="font-medium text-text-secondary">Note:</span>
                After restarting Cursor you may need to toggle each MCP server off and on in Cursor's settings for them to reconnect.
              </p>
            </div>

            <!-- Overwrite warning -->
            <div v-if="confirmingId === tool.id && hasOverwrites(tool)" class="border-t border-border/50 px-3 py-2">
              <p class="text-[11px] text-status-connecting">
                Some servers below share a name with servers you already have. Migrating will replace your existing configurations with the ones from {{ tool.name }}.
              </p>
            </div>

            <!-- Server list -->
            <div v-if="tool.existingServers.length" class="border-t border-border/50 px-3 py-2">
              <div class="space-y-1">
                <div
                  v-for="server in tool.existingServers"
                  :key="server.name"
                  class="rounded bg-surface-0 px-2 py-1.5"
                >
                  <div class="flex items-center gap-2">
                    <span class="font-mono text-[11px] font-medium text-text-secondary">{{ server.name }}</span>
                    <span class="truncate text-[10px] text-text-muted">{{ serverSummary(server) }}</span>
                  </div>
                  <div v-if="willOverwrite(server.name)" class="mt-0.5 text-[10px] text-status-connecting">
                    will replace existing on migrate
                  </div>
                </div>
              </div>
            </div>
            <div v-else-if="!tool.enabled" class="border-t border-border/50 px-3 py-2">
              <span class="text-[10px] text-text-muted">No existing MCP servers configured</span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="!installedTools.length" class="text-xs text-text-muted">
        No supported AI tools detected.
      </div>
    </template>
  </div>
</template>
