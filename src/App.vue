<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import ServerList from './components/ServerList.vue';
import SkillList from './components/SkillList.vue';
import { useServersStore } from '@/stores/servers';
import { useSkillsStore } from '@/stores/skills';
import { useEvents } from '@/composables/useEvents';
import { useServerLogs } from '@/composables/useServerLogs';

const serversStore = useServersStore();
const skillsStore = useSkillsStore();

useEvents();
useServerLogs();

const serversCollapsed = ref(false);
const skillsCollapsed = ref(false);
const memoryCollapsed = ref(false);

onMounted(async () => {
  await serversStore.loadServers();
  serversStore.autoConnectServers();
  skillsStore.loadSkills();
  // Set app icon at runtime (works during tauri dev)
  fetch('/app-icon.png')
    .then((r) => r.arrayBuffer())
    .then((buf) => getCurrentWindow().setIcon(new Uint8Array(buf)))
    .catch(() => {});
});
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Titlebar drag region — spans full window width -->
    <div data-tauri-drag-region class="h-8 shrink-0" />

    <div class="flex min-h-0 flex-1">
      <!-- Sidebar -->
      <aside class="flex w-60 flex-col border-r border-border bg-surface-1">

        <div class="min-h-0 flex-1 overflow-y-auto">
          <!-- SERVERS section -->
          <div class="flex items-center justify-between border-b border-border px-3 py-2">
            <button
              class="flex items-center gap-1 font-mono text-xs font-medium tracking-wide text-text-secondary uppercase"
              @click="serversCollapsed = !serversCollapsed"
            >
              <span
                class="inline-block text-[10px] leading-none transition-transform"
                :class="serversCollapsed ? '-rotate-90' : ''"
              >&#9662;</span>
              Servers
            </button>
            <router-link
              to="/add"
              class="rounded bg-accent px-2 py-0.5 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover"
            >
              Add
            </router-link>
          </div>
          <ServerList v-show="!serversCollapsed" />

          <!-- SKILLS section -->
          <div class="flex items-center justify-between border-b border-border px-3 py-2">
            <button
              class="flex items-center gap-1 font-mono text-xs font-medium tracking-wide text-text-secondary uppercase"
              @click="skillsCollapsed = !skillsCollapsed"
            >
              <span
                class="inline-block text-[10px] leading-none transition-transform"
                :class="skillsCollapsed ? '-rotate-90' : ''"
              >&#9662;</span>
              Skills
            </button>
          </div>
          <SkillList v-show="!skillsCollapsed" />

          <!-- MEMORY section -->
          <div class="flex items-center justify-between border-b border-border px-3 py-2">
            <button
              class="flex items-center gap-1 font-mono text-xs font-medium tracking-wide text-text-secondary uppercase"
              @click="memoryCollapsed = !memoryCollapsed"
            >
              <span
                class="inline-block text-[10px] leading-none transition-transform"
                :class="memoryCollapsed ? '-rotate-90' : ''"
              >&#9662;</span>
              Memory
            </button>
          </div>
          <div v-show="!memoryCollapsed">
            <router-link
              to="/memories"
              class="flex items-center px-3 py-1.5 text-xs text-text-muted transition-colors hover:bg-surface-2 hover:text-text-secondary"
              active-class="bg-surface-2 text-text-secondary"
            >
              Browse
            </router-link>
            <router-link
              to="/memory-graph"
              class="flex items-center px-3 py-1.5 text-xs text-text-muted transition-colors hover:bg-surface-2 hover:text-text-secondary"
              active-class="bg-surface-2 text-text-secondary"
            >
              Graph
            </router-link>
            <router-link
              to="/memory-data"
              class="flex items-center px-3 py-1.5 text-xs text-text-muted transition-colors hover:bg-surface-2 hover:text-text-secondary"
              active-class="bg-surface-2 text-text-secondary"
            >
              Data Management
            </router-link>
          </div>
        </div>

        <!-- Bottom nav links -->
        <div class="shrink-0 border-t border-border">
          <router-link
            to="/status"
            class="flex items-center gap-2 px-3 py-2 text-xs text-text-muted transition-colors hover:bg-surface-2 hover:text-text-secondary"
            active-class="bg-surface-2 text-text-secondary"
          >
            Status
          </router-link>
          <router-link
            to="/settings"
            class="flex items-center gap-2 px-3 py-2 text-xs transition-colors hover:bg-surface-2"
            :class="$route.path === '/settings' ? 'text-text-primary' : 'text-text-muted hover:text-text-secondary'"
          >
            Settings
          </router-link>
        </div>
      </aside>

      <!-- Main content -->
      <main class="min-h-0 min-w-0 flex-1 overflow-hidden bg-surface-0">
        <router-view />
      </main>
    </div>
  </div>
</template>
