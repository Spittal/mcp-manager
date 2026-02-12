<script setup lang="ts">
import { onMounted } from 'vue';
import ServerList from './components/ServerList.vue';
import StatusBar from './components/StatusBar.vue';
import { useServersStore } from '@/stores/servers';
import { useEvents } from '@/composables/useEvents';
import { useServerLogs } from '@/composables/useServerLogs';

const serversStore = useServersStore();

useEvents();
useServerLogs();

onMounted(() => {
  serversStore.loadServers();
});
</script>

<template>
  <div class="flex h-full flex-col">
    <div class="flex min-h-0 flex-1">
      <!-- Sidebar -->
      <aside class="flex w-60 flex-col border-r border-border bg-surface-1">
        <div class="flex items-center justify-between border-b border-border px-3 py-2">
          <span class="font-mono text-xs font-medium tracking-wide text-text-secondary uppercase">Servers</span>
          <router-link
            to="/add"
            class="flex h-6 w-6 items-center justify-center rounded text-text-muted transition-colors hover:bg-surface-3 hover:text-text-primary"
          >
            <span class="text-sm leading-none">+</span>
          </router-link>
        </div>
        <ServerList />
        <div class="mt-auto border-t border-border">
          <router-link
            to="/settings"
            class="flex items-center gap-2 px-3 py-2 text-xs text-text-muted transition-colors hover:bg-surface-2 hover:text-text-secondary"
          >
            Settings
          </router-link>
        </div>
      </aside>

      <!-- Main content -->
      <main class="flex min-w-0 flex-1 flex-col bg-surface-0">
        <router-view />
      </main>
    </div>
    <StatusBar />
  </div>
</template>
