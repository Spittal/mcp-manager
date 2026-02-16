<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';
import ToolBrowser from '@/components/ToolBrowser.vue';
import LogViewer from '@/components/LogViewer.vue';
import { statusColor, statusLabel } from '@/composables/useServerStatus';
import { useServerStats, formatClientName } from '@/composables/useServerStats';

const store = useServersStore();
const { servers, selectedServerId, lastError, oauthStatus } = storeToRefs(store);

const selectedServer = computed(() =>
  servers.value.find((s) => s.id === selectedServerId.value)
);

// --- Error handling ---

const serverError = computed(() => {
  if (!selectedServer.value) return null;
  const raw = lastError.value[selectedServer.value.id] ?? null;
  if (!raw) return null;
  if (raw.includes('<!DOCTYPE') || raw.includes('<html')) {
    return 'Server returned an HTML page instead of a JSON-RPC response. Check that the URL is an MCP endpoint (e.g. https://mcp.example.com/sse), not a documentation page.';
  }
  return raw.length > 300 ? raw.slice(0, 300) + '...' : raw;
});

// --- OAuth ---

const serverOAuthStatus = computed(() => {
  if (!selectedServer.value) return null;
  return oauthStatus.value[selectedServer.value.id] ?? null;
});

const isAuthRequired = computed(() =>
  serverOAuthStatus.value === 'idle' || serverError.value?.includes('Authentication required')
);

const isOAuthInProgress = computed(() => {
  const s = serverOAuthStatus.value;
  return s === 'discovering' || s === 'awaiting_browser' || s === 'exchanging_code';
});

const isOAuthAuthorized = computed(() => serverOAuthStatus.value === 'authorized');

const OAUTH_LABELS: Record<string, string> = {
  discovering: 'Discovering OAuth server...',
  awaiting_browser: 'Complete sign-in in your browser...',
  exchanging_code: 'Exchanging authorization code...',
};

const oauthProgressLabel = computed(() =>
  OAUTH_LABELS[serverOAuthStatus.value ?? ''] ?? ''
);

// --- Stats (composable) ---

const { stats, resetStats, successRate, avgLatency, topClient, sortedTools, recentCalls } =
  useServerStats(selectedServerId);

// --- Tabs ---

type Tab = 'overview' | 'tools' | 'logs';

const availableTabs = computed<Tab[]>(() =>
  selectedServer.value?.status === 'connected'
    ? ['overview', 'tools', 'logs']
    : ['overview', 'logs']
);
const activeTab = ref<Tab>('overview');

// Reset to overview if current tab becomes unavailable (e.g. disconnect)
watch(availableTabs, (tabs) => {
  if (!tabs.includes(activeTab.value)) {
    activeTab.value = 'overview';
  }
});

// --- Local state ---

const confirmingDelete = ref(false);

watch(selectedServerId, () => {
  confirmingDelete.value = false;
});

// --- Actions ---

async function deleteServer() {
  if (!selectedServer.value) return;
  const id = selectedServer.value.id;
  confirmingDelete.value = false;
  if (selectedServer.value.status === 'connected') {
    await store.disconnectServer(id);
  }
  await store.removeServer(id);
}

async function toggleEnabled() {
  if (!selectedServer.value) return;
  const server = selectedServer.value;
  const newEnabled = !server.enabled;
  if (!newEnabled && server.status === 'connected') {
    await store.disconnectServer(server.id);
  }
  await store.updateServer(server.id, {
    name: server.name,
    transport: server.transport,
    enabled: newEnabled,
    command: server.command,
    args: server.args,
    env: server.env,
    url: server.url,
    headers: server.headers,
    tags: server.tags,
  });
  if (newEnabled) {
    store.connectServer(server.id);
  }
}

// --- Helpers ---

function formatDate(raw: string | undefined): string {
  if (!raw) return '';
  const ts = Number(raw);
  const date = Number.isFinite(ts) && ts > 1e9 ? new Date(ts * 1000) : new Date(raw);
  if (isNaN(date.getTime())) return '';
  return date.toLocaleString();
}

function formatTime(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleTimeString();
}
</script>

<template>
  <div v-if="selectedServer" class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <span class="h-2 w-2 rounded-full" :class="statusColor(selectedServer.status)" />
      <h1 class="text-sm font-medium">{{ selectedServer.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedServer.transport }}</span>
      <div class="ml-auto flex items-center gap-2">
        <button
          class="relative h-5 w-9 rounded-full transition-colors"
          :class="selectedServer.enabled ? 'bg-accent' : 'bg-surface-3'"
          :title="selectedServer.enabled ? 'Disable server' : 'Enable server'"
          @click="toggleEnabled"
        >
          <span
            class="absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform"
            :class="{ 'translate-x-4': selectedServer.enabled }"
          />
        </button>

        <router-link
          :to="`/edit/${selectedServer.id}`"
          class="rounded bg-surface-3 px-3 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-2"
        >
          Edit
        </router-link>

        <template v-if="!selectedServer.managed">
          <button
            v-if="!confirmingDelete"
            class="rounded bg-surface-3 px-3 py-1 text-xs text-text-muted transition-colors hover:bg-status-error/20 hover:text-status-error"
            @click="confirmingDelete = true"
          >
            Delete
          </button>
          <template v-else>
            <button
              class="rounded bg-status-error px-3 py-1 text-xs text-white transition-colors hover:bg-status-error/80"
              @click="deleteServer"
            >
              Confirm
            </button>
            <button
              class="rounded bg-surface-3 px-3 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-2"
              @click="confirmingDelete = false"
            >
              Cancel
            </button>
          </template>
        </template>
      </div>
    </header>

    <!-- Tabs -->
    <div class="flex border-b border-border">
      <button
        v-for="tab in availableTabs"
        :key="tab"
        class="border-b-2 px-4 py-2 text-xs transition-colors"
        :class="activeTab === tab
          ? 'border-accent text-text-primary'
          : 'border-transparent text-text-muted hover:text-text-secondary'"
        @click="activeTab = tab"
      >
        {{ tab.charAt(0).toUpperCase() + tab.slice(1) }}
      </button>
    </div>

    <!-- Tab content -->
    <div class="min-h-0 flex-1">
      <!-- Overview -->
      <div v-if="activeTab === 'overview'" class="h-full overflow-y-auto p-4">
        <!-- OAuth: Authentication Required -->
        <div v-if="isAuthRequired && !isOAuthInProgress" class="mb-4 rounded border border-accent/30 bg-accent/10 p-4">
          <p class="mb-1 font-mono text-xs font-medium text-accent">Authentication Required</p>
          <p class="mb-3 text-xs text-text-secondary">This server requires OAuth authorization to connect.</p>
          <button
            class="rounded bg-accent px-4 py-1.5 text-xs font-medium text-white transition-colors hover:bg-accent-hover"
            @click="store.startOAuth(selectedServer!.id)"
          >
            Authorize
          </button>
        </div>

        <!-- OAuth: In-progress -->
        <div v-else-if="isOAuthInProgress" class="mb-4 rounded border border-accent/30 bg-accent/10 p-4">
          <p class="mb-1 font-mono text-xs font-medium text-accent">Authorizing</p>
          <div class="flex items-center gap-2">
            <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />
            <p class="text-xs text-text-secondary">{{ oauthProgressLabel }}</p>
          </div>
        </div>

        <!-- Generic error (non-auth) -->
        <div v-else-if="serverError && !isAuthRequired" class="mb-4 rounded border border-status-error/30 bg-status-error/10 p-3">
          <p class="mb-1 font-mono text-xs font-medium text-status-error">Connection Error</p>
          <p class="whitespace-pre-line font-mono text-xs text-text-secondary break-all">{{ serverError }}</p>
          <p class="mt-2 text-xs text-text-muted">
            This usually means the server needs additional configuration, such as an API key.
            Check the server's documentation for setup instructions, then use
            <router-link :to="`/edit/${selectedServer!.id}`" class="text-accent hover:text-accent-hover">Edit</router-link>
            to update the server settings.
          </p>
        </div>

        <section class="mb-6">
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Status</h2>
          <div class="rounded border border-border bg-surface-1 p-3">
            <div class="flex items-center gap-2">
              <span class="h-1.5 w-1.5 rounded-full" :class="statusColor(selectedServer.status)" />
              <span class="font-mono text-xs text-text-secondary capitalize">{{ statusLabel(selectedServer.status) }}</span>
            </div>
            <div v-if="isOAuthAuthorized || (selectedServer.status === 'connected' && serverOAuthStatus)" class="mt-2 flex items-center justify-between">
              <span class="text-[11px] text-accent">OAuth authorized</span>
              <button
                class="text-[11px] text-text-muted transition-colors hover:text-status-error"
                @click="store.clearOAuthTokens(selectedServer.id)"
              >
                Revoke
              </button>
            </div>
            <div v-if="formatDate(selectedServer.lastConnected)" class="mt-2 text-[11px] text-text-muted">
              Last connected: {{ formatDate(selectedServer.lastConnected) }}
            </div>
          </div>
        </section>

        <section v-if="stats && stats.totalCalls > 0" class="mb-6">
          <div class="mb-2 flex items-center justify-between">
            <h2 class="font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Usage</h2>
            <button
              class="text-[11px] text-text-muted transition-colors hover:text-status-error"
              @click="resetStats"
            >
              Reset Stats
            </button>
          </div>
          <div class="grid grid-cols-2 gap-2">
            <div class="rounded border border-border bg-surface-1 p-3">
              <div class="text-lg font-medium text-text-primary">{{ stats.totalCalls }}</div>
              <div class="text-[11px] text-text-muted">Total Calls</div>
            </div>
            <div class="rounded border border-border bg-surface-1 p-3">
              <div class="text-lg font-medium text-text-primary">{{ successRate !== null ? successRate.toFixed(1) + '%' : '—' }}</div>
              <div class="text-[11px] text-text-muted">Success Rate</div>
            </div>
            <div class="rounded border border-border bg-surface-1 p-3">
              <div class="text-lg font-medium text-text-primary">{{ avgLatency !== null ? avgLatency + 'ms' : '—' }}</div>
              <div class="text-[11px] text-text-muted">Avg Latency</div>
            </div>
            <div class="rounded border border-border bg-surface-1 p-3">
              <div class="text-lg font-medium text-text-primary">{{ topClient ? formatClientName(topClient) : '—' }}</div>
              <div class="text-[11px] text-text-muted">Top Client</div>
            </div>
          </div>

          <div v-if="sortedTools.length > 0" class="mt-3">
            <table class="w-full text-xs font-mono">
              <thead>
                <tr class="border-b border-border text-left text-text-muted">
                  <th class="pb-1.5 font-normal">Tool</th>
                  <th class="pb-1.5 font-normal text-right">Calls</th>
                  <th class="pb-1.5 font-normal text-right">Errors</th>
                  <th class="pb-1.5 font-normal text-right">Avg Latency</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="tool in sortedTools" :key="tool.name" class="border-b border-border/50">
                  <td class="py-1.5 text-text-secondary">{{ tool.name }}</td>
                  <td class="py-1.5 text-right text-text-secondary">{{ tool.calls }}</td>
                  <td class="py-1.5 text-right" :class="tool.errors > 0 ? 'text-status-error' : 'text-text-secondary'">{{ tool.errors }}</td>
                  <td class="py-1.5 text-right text-text-secondary">{{ tool.avgLatency }}ms</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="recentCalls.length > 0" class="mt-4">
            <h3 class="mb-1.5 text-[11px] font-medium text-text-muted">Recent Calls</h3>
            <div class="max-h-52 overflow-y-auto rounded border border-border">
              <table class="w-full text-xs font-mono">
                <thead class="sticky top-0 bg-surface-1">
                  <tr class="border-b border-border text-left text-text-muted">
                    <th class="px-2 py-1.5 font-normal">Time</th>
                    <th class="px-2 py-1.5 font-normal">Tool</th>
                    <th class="px-2 py-1.5 font-normal">Client</th>
                    <th class="px-2 py-1.5 font-normal text-right">Latency</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="(call, i) in recentCalls"
                    :key="i"
                    class="border-b border-border/30"
                    :class="call.isError ? 'text-status-error' : 'text-text-secondary'"
                  >
                    <td class="px-2 py-1">{{ formatTime(call.timestamp) }}</td>
                    <td class="px-2 py-1">{{ call.tool }}</td>
                    <td class="px-2 py-1">{{ call.client ? formatClientName(call.client) : '—' }}</td>
                    <td class="px-2 py-1 text-right">{{ call.durationMs }}ms</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </section>

        <section class="mb-6">
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Configuration</h2>
          <div class="rounded border border-border bg-surface-1 p-3 font-mono text-xs text-text-secondary">
            <div v-if="selectedServer.command">
              <span class="text-text-muted">command:</span> {{ selectedServer.command }}
            </div>
            <div v-if="selectedServer.args?.length">
              <span class="text-text-muted">args:</span> {{ selectedServer.args.join(' ') }}
            </div>
            <div v-if="selectedServer.url">
              <span class="text-text-muted">url:</span> {{ selectedServer.url }}
            </div>
            <div v-if="selectedServer.env && Object.keys(selectedServer.env).length">
              <span class="text-text-muted">env:</span>
              <div v-for="(_val, key) in selectedServer.env" :key="key" class="ml-3">
                {{ key }}=••••••••
              </div>
            </div>
            <div v-if="selectedServer.tags?.length" class="mt-2">
              <span class="text-text-muted">tags:</span>
              <span
                v-for="tag in selectedServer.tags"
                :key="tag"
                class="ml-1 rounded bg-surface-3 px-1.5 py-0.5 text-[11px] text-text-secondary"
              >
                {{ tag }}
              </span>
            </div>
          </div>
        </section>
      </div>

      <!-- Tools -->
      <ToolBrowser v-if="activeTab === 'tools'" :server-id="selectedServer.id" />

      <!-- Logs -->
      <LogViewer v-if="activeTab === 'logs'" :server-id="selectedServer.id" />
    </div>
  </div>

  <div v-else class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No server selected</p>
      <p class="text-xs">Select a server from the sidebar or <router-link to="/add" class="text-accent hover:text-accent-hover">add one</router-link>.</p>
    </div>
  </div>
</template>
