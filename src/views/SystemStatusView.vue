<script setup lang="ts">
import { computed } from 'vue';
import { useSystemStatus } from '@/composables/useSystemStatus';

const { status, loading, history, refresh } = useSystemStatus();

const lastChecked = computed(() => {
  if (!status.value) return 'Never';
  return new Date(status.value.checkedAt * 1000).toLocaleTimeString();
});

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="flex items-center justify-between border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">System Status</h1>
      <div class="flex items-center gap-3">
        <span class="text-[10px] text-text-muted">Last: {{ lastChecked }}</span>
        <button
          class="text-[10px] text-text-muted underline hover:text-text-secondary"
          :disabled="loading"
          @click="refresh"
        >
          {{ loading ? 'Checking...' : 'Refresh' }}
        </button>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto p-4">
      <div class="mx-auto max-w-lg space-y-5">
        <!-- Uptime strip -->
        <div v-if="history.length" class="space-y-1.5">
          <span class="text-[10px] text-text-muted uppercase tracking-wide">Health history</span>
          <div class="flex gap-0.5">
            <div
              v-for="(entry, i) in history"
              :key="i"
              class="h-5 min-w-1 flex-1 rounded-sm"
              :class="
                entry.proxyOk && entry.redisOk
                  ? 'bg-status-connected'
                  : !entry.proxyOk && !entry.redisOk
                    ? 'bg-status-error'
                    : 'bg-status-connecting'
              "
              :title="`${new Date(entry.timestamp).toLocaleTimeString()} — Proxy: ${entry.proxyOk ? 'OK' : 'Down'}, Redis: ${entry.redisOk ? 'OK' : 'Down'}`"
            />
          </div>
        </div>

        <div v-if="status" class="space-y-4">
          <!-- Servers summary -->
          <section class="rounded border border-border bg-surface-1">
            <div class="flex items-center justify-between px-3 py-2.5">
              <span class="text-xs font-medium text-text-primary">Servers</span>
              <span class="text-[10px] text-text-muted">
                {{ status.connectedCount }}/{{ status.serverCount }} connected
              </span>
            </div>
          </section>

          <!-- Proxy -->
          <section class="rounded border border-border bg-surface-1">
            <div class="flex items-center justify-between px-3 py-2.5">
              <div class="flex items-center gap-2">
                <span
                  class="h-1.5 w-1.5 rounded-full"
                  :class="status.proxy.running ? 'bg-status-connected' : 'bg-status-error'"
                />
                <span class="text-xs font-medium text-text-primary">Proxy</span>
              </div>
              <span class="text-[10px] font-mono text-text-muted">
                {{ status.proxy.running ? `port ${status.proxy.port}` : 'stopped' }}
              </span>
            </div>
          </section>

          <!-- Redis -->
          <section v-if="status.redis" class="rounded border border-border bg-surface-1">
            <div class="flex items-center justify-between border-b border-border/50 px-3 py-2.5">
              <div class="flex items-center gap-2">
                <span
                  class="h-1.5 w-1.5 rounded-full"
                  :class="status.redis.ok ? 'bg-status-connected' : 'bg-status-error'"
                />
                <span class="text-xs font-medium text-text-primary">Redis</span>
              </div>
              <span
                class="rounded-full px-2 py-0.5 text-[10px] font-medium"
                :class="status.redis.ok
                  ? 'bg-status-connected/10 text-status-connected'
                  : 'bg-status-error/10 text-status-error'"
              >
                {{ status.redis.ok ? 'Healthy' : 'Down' }}
              </span>
            </div>
            <div class="px-3 py-2 space-y-1.5 text-[11px]">
              <div class="flex justify-between">
                <span class="text-text-muted">Latency</span>
                <span class="text-text-secondary">{{ status.redis.latencyMs }}ms</span>
              </div>
              <div v-if="status.redis.usedMemoryHuman" class="flex justify-between">
                <span class="text-text-muted">Memory</span>
                <span class="text-text-secondary">{{ status.redis.usedMemoryHuman }}</span>
              </div>
              <div v-if="status.redis.connectedClients != null" class="flex justify-between">
                <span class="text-text-muted">Clients</span>
                <span class="text-text-secondary">{{ status.redis.connectedClients }}</span>
              </div>
              <div v-if="status.redis.uptimeInSeconds != null" class="flex justify-between">
                <span class="text-text-muted">Uptime</span>
                <span class="text-text-secondary">{{ formatUptime(status.redis.uptimeInSeconds!) }}</span>
              </div>
              <div v-if="status.redis.dbKeys != null" class="flex justify-between">
                <span class="text-text-muted">Keys</span>
                <span class="text-text-secondary">{{ status.redis.dbKeys!.toLocaleString() }}</span>
              </div>
              <div v-if="status.redis.error" class="text-[10px] text-status-error">
                {{ status.redis.error }}
              </div>
            </div>
          </section>

          <!-- Processes -->
          <section v-if="status.processes.length">
            <h2 class="mb-2 font-mono text-[10px] font-medium tracking-wide text-text-muted uppercase">
              Processes
            </h2>
            <div class="space-y-1.5">
              <div
                v-for="proc in status.processes"
                :key="proc.pid"
                class="rounded border border-border bg-surface-1 px-3 py-2 text-[11px]"
              >
                <div class="flex items-center justify-between">
                  <span class="font-medium text-text-primary">{{ proc.name }}</span>
                  <span class="text-[10px] text-text-muted">PID {{ proc.pid }}</span>
                </div>
                <div class="mt-0.5 truncate text-[10px] text-text-muted" :title="proc.command">
                  {{ proc.command }}
                </div>
                <div class="mt-1.5 flex gap-4">
                  <div class="flex gap-1.5">
                    <span class="text-text-muted">CPU</span>
                    <span class="text-text-secondary">{{ proc.cpuPercent.toFixed(1) }}%</span>
                  </div>
                  <div class="flex gap-1.5">
                    <span class="text-text-muted">RAM</span>
                    <span class="text-text-secondary">{{ formatBytes(proc.memoryBytes) }}</span>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>

        <!-- Loading state -->
        <div v-if="!status && loading" class="py-12 text-center text-xs text-text-muted">
          Checking system status...
        </div>
      </div>
    </div>
  </div>
</template>
