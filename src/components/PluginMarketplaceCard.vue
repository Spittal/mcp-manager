<script setup lang="ts">
import type { PluginInfo } from '@/types/plugin';

defineProps<{
  plugin: PluginInfo;
  installing: boolean;
}>();

const emit = defineEmits<{
  install: [];
}>();

function onInstall(event: Event) {
  event.stopPropagation();
  emit('install');
}
</script>

<template>
  <div
    class="flex items-start gap-3 rounded-lg border border-border bg-surface-1 px-4 py-3 transition-colors hover:border-border-active"
  >
    <!-- Icon -->
    <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-surface-3 text-text-muted">
      <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
        <path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd" />
      </svg>
    </div>

    <!-- Content -->
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="truncate text-sm font-medium text-text-primary">{{ plugin.name }}</span>
        <span
          v-if="plugin.isRemote"
          class="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[10px] text-text-muted"
        >
          Remote
        </span>
      </div>
      <div v-if="plugin.description" class="mt-0.5 line-clamp-2 text-[11px] text-text-muted">{{ plugin.description }}</div>
      <div class="mt-0.5 flex items-center gap-2 text-[11px] text-text-muted">
        <span v-if="plugin.marketplace">{{ plugin.marketplace }}</span>
        <span v-if="plugin.installCount">{{ plugin.installCount.toLocaleString() }} installs</span>
      </div>
    </div>

    <!-- Action -->
    <div class="shrink-0 pt-0.5">
      <span
        v-if="plugin.installed"
        class="inline-flex items-center rounded-md bg-status-connected/10 px-2.5 py-1 text-xs font-medium text-status-connected"
      >
        Installed
      </span>
      <button
        v-else-if="installing"
        disabled
        class="rounded-md bg-surface-3 px-3 py-1 text-xs text-text-muted"
      >
        Installing...
      </button>
      <button
        v-else
        class="rounded-md bg-accent px-3 py-1 text-xs font-medium text-white transition-colors hover:bg-accent-hover"
        @click="onInstall($event)"
      >
        Install
      </button>
    </div>
  </div>
</template>
