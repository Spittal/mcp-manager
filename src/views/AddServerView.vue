<script setup lang="ts">
import { ref, computed } from 'vue';
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import type { ServerTransport } from '@/types/server';

const router = useRouter();
const store = useServersStore();

const name = ref('');
const transport = ref<ServerTransport>('stdio');
const command = ref('');
const args = ref('');
const url = ref('');
const headers = ref('');

const urlWarning = computed(() => {
  const u = url.value.trim().toLowerCase();
  if (!u) return null;
  // Detect common docs pages that aren't MCP endpoints
  if (u.includes('/docs/') || u.includes('/changelog/') || u.includes('/blog/')) {
    return 'This looks like a documentation page, not an MCP endpoint. The endpoint URL is usually on a different subdomain (e.g. mcp.linear.app) and ends with /sse or /mcp.';
  }
  // Warn if URL doesn't look like a typical MCP endpoint
  if (u.endsWith('.html') || u.endsWith('.htm')) {
    return 'This URL points to an HTML page. MCP endpoints typically end with /sse or /mcp.';
  }
  return null;
});

function parseHeaders(): Record<string, string> {
  const parsed: Record<string, string> = {};
  for (const line of headers.value.split('\n')) {
    const idx = line.indexOf(':');
    if (idx > 0) {
      parsed[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
    }
  }
  return parsed;
}

async function addServer() {
  if (!name.value.trim()) return;

  store.addServer({
    name: name.value.trim(),
    transport: transport.value,
    enabled: true,
    ...(transport.value === 'stdio'
      ? {
          command: command.value.trim(),
          args: args.value.split(/\s+/).filter(Boolean),
        }
      : {
          url: url.value.trim(),
          headers: parseHeaders(),
        }),
  });

  router.push('/');
}
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Add Server</h1>
    </header>

    <form class="flex-1 overflow-y-auto p-4" @submit.prevent="addServer">
      <div class="mx-auto max-w-md space-y-4">
        <!-- Name -->
        <div>
          <label class="mb-1 block font-mono text-xs text-text-muted uppercase">Name</label>
          <input
            v-model="name"
            type="text"
            placeholder="My MCP Server"
            class="w-full rounded border border-border bg-surface-1 px-3 py-2 text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
          />
        </div>

        <!-- Transport -->
        <div>
          <label class="mb-1 block font-mono text-xs text-text-muted uppercase">Transport</label>
          <div class="flex gap-2">
            <button
              type="button"
              class="rounded border px-3 py-1.5 text-xs transition-colors"
              :class="transport === 'stdio'
                ? 'border-accent bg-accent/10 text-accent'
                : 'border-border text-text-secondary hover:border-border-active'"
              @click="transport = 'stdio'"
            >
              stdio
            </button>
            <button
              type="button"
              class="rounded border px-3 py-1.5 text-xs transition-colors"
              :class="transport === 'http'
                ? 'border-accent bg-accent/10 text-accent'
                : 'border-border text-text-secondary hover:border-border-active'"
              @click="transport = 'http'"
            >
              HTTP
            </button>
          </div>
        </div>

        <!-- stdio fields -->
        <template v-if="transport === 'stdio'">
          <div>
            <label class="mb-1 block font-mono text-xs text-text-muted uppercase">Command</label>
            <input
              v-model="command"
              type="text"
              placeholder="npx"
              class="w-full rounded border border-border bg-surface-1 px-3 py-2 font-mono text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
            />
          </div>
          <div>
            <label class="mb-1 block font-mono text-xs text-text-muted uppercase">Arguments</label>
            <input
              v-model="args"
              type="text"
              placeholder="-y @modelcontextprotocol/server-filesystem /tmp"
              class="w-full rounded border border-border bg-surface-1 px-3 py-2 font-mono text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
            />
          </div>
        </template>

        <!-- HTTP fields -->
        <template v-if="transport === 'http'">
          <div>
            <label class="mb-1 block font-mono text-xs text-text-muted uppercase">URL</label>
            <input
              v-model="url"
              type="text"
              placeholder="https://mcp.linear.app/sse"
              class="w-full rounded border border-border bg-surface-1 px-3 py-2 font-mono text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
            />
            <p v-if="urlWarning" class="mt-1.5 rounded bg-status-error/10 px-2 py-1 text-[11px] text-status-error">{{ urlWarning }}</p>
            <p v-else class="mt-1 text-[11px] text-text-muted">The MCP server endpoint URL, not the docs page. Often ends with /sse or /mcp.</p>
          </div>
          <div>
            <label class="mb-1 block font-mono text-xs text-text-muted uppercase">Headers</label>
            <textarea
              v-model="headers"
              placeholder="Authorization: Bearer your-token-here"
              rows="3"
              class="w-full rounded border border-border bg-surface-1 px-3 py-2 font-mono text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
            />
            <p class="mt-1 text-[11px] text-text-muted">One header per line, format: Key: Value</p>
          </div>
        </template>

        <div class="flex gap-2 pt-2">
          <button
            type="submit"
            class="rounded bg-accent px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-accent-hover"
          >
            Add Server
          </button>
          <router-link
            to="/"
            class="rounded bg-surface-3 px-4 py-2 text-xs text-text-secondary transition-colors hover:bg-surface-2"
          >
            Cancel
          </router-link>
        </div>
      </div>
    </form>
  </div>
</template>
