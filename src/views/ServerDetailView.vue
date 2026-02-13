<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';
import ToolBrowser from '@/components/ToolBrowser.vue';
import LogViewer from '@/components/LogViewer.vue';
import { statusColor, statusLabel } from '@/composables/useServerStatus';

const store = useServersStore();
const { servers, selectedServerId, lastError, oauthStatus } = storeToRefs(store);

const selectedServer = computed(() =>
  servers.value.find((s) => s.id === selectedServerId.value)
);

const serverErrorRaw = computed(() => {
  if (!selectedServer.value) return null;
  return lastError.value[selectedServer.value.id] ?? null;
});

const serverError = computed(() => {
  const raw = serverErrorRaw.value;
  if (!raw) return null;
  // Detect HTML responses (server returned a webpage, not JSON-RPC)
  if (raw.includes('<!DOCTYPE') || raw.includes('<html')) {
    return 'Server returned an HTML page instead of a JSON-RPC response. Check that the URL is an MCP endpoint (e.g. https://mcp.example.com/sse), not a documentation page.';
  }
  // Truncate very long errors
  if (raw.length > 300) {
    return raw.slice(0, 300) + '...';
  }
  return raw;
});

const serverOAuthStatus = computed(() => {
  if (!selectedServer.value) return null;
  return oauthStatus.value[selectedServer.value.id] ?? null;
});

const isAuthRequired = computed(() => {
  return serverOAuthStatus.value === 'idle'
    || (serverError.value?.includes('Authentication required'));
});

const isOAuthInProgress = computed(() => {
  const s = serverOAuthStatus.value;
  return s === 'discovering' || s === 'awaiting_browser' || s === 'exchanging_code';
});

const isOAuthAuthorized = computed(() => {
  return serverOAuthStatus.value === 'authorized';
});

const oauthProgressLabel = computed(() => {
  switch (serverOAuthStatus.value) {
    case 'discovering': return 'Discovering OAuth server...';
    case 'awaiting_browser': return 'Complete sign-in in your browser...';
    case 'exchanging_code': return 'Exchanging authorization code...';
    default: return '';
  }
});

const confirmingDelete = ref(false);

// Reset confirm state whenever the selected server changes
watch(selectedServerId, () => {
  confirmingDelete.value = false;
});

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

type Tab = 'overview' | 'tools' | 'logs';
const activeTab = ref<Tab>('overview');

function formatDate(iso: string | undefined): string {
  if (!iso) return 'Never';
  return new Date(iso).toLocaleString();
}
</script>

<template>
  <div v-if="selectedServer" class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <span
        class="h-2 w-2 rounded-full"
        :class="statusColor(selectedServer.status)"
      />
      <h1 class="text-sm font-medium">{{ selectedServer.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedServer.transport }}</span>
      <div class="ml-auto flex items-center gap-2">
        <!-- Enable/Disable toggle -->
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
        <button
          v-if="selectedServer.status !== 'connected'"
          class="rounded bg-accent px-3 py-1 text-xs text-white transition-colors hover:bg-accent-hover"
          :disabled="!selectedServer.enabled"
          :class="{ 'opacity-50 cursor-not-allowed': !selectedServer.enabled }"
          @click="store.connectServer(selectedServer.id)"
        >
          Connect
        </button>
        <button
          v-else
          class="rounded bg-surface-3 px-3 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-2"
          @click="store.disconnectServer(selectedServer.id)"
        >
          Disconnect
        </button>

        <!-- Delete (hidden for managed servers) -->
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
        v-for="tab in (['overview', 'tools', 'logs'] as const)"
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
          <p class="font-mono text-xs text-text-secondary break-all">{{ serverError }}</p>
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
            <div class="mt-2 text-[11px] text-text-muted">
              Last connected: {{ formatDate(selectedServer.lastConnected) }}
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
              <div v-for="(val, key) in selectedServer.env" :key="key" class="ml-3">
                {{ key }}={{ val }}
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
