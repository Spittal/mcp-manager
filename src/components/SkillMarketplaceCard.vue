<script setup lang="ts">
import type { MarketplaceSkillSummary } from '@/types/skill';

const props = defineProps<{
  skill: MarketplaceSkillSummary;
  installing: boolean;
}>();

const emit = defineEmits<{
  install: [];
}>();

function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k`;
  return String(count);
}

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
        <path d="M9 4.804A7.968 7.968 0 005.5 4c-1.255 0-2.443.29-3.5.804v10A7.969 7.969 0 015.5 14c1.669 0 3.218.51 4.5 1.385A7.962 7.962 0 0114.5 14c1.255 0 2.443.29 3.5.804v-10A7.968 7.968 0 0014.5 4c-1.255 0-2.443.29-3.5.804V12a1 1 0 11-2 0V4.804z" />
      </svg>
    </div>

    <!-- Content -->
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="truncate text-sm font-medium text-text-primary">{{ skill.name }}</span>
      </div>
      <div class="mt-0.5 truncate text-[11px] text-text-muted">{{ skill.source }}</div>
      <div class="mt-1 flex items-center gap-2 text-[11px] text-text-muted">
        <span v-if="skill.installs != null" class="flex items-center gap-0.5">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
            <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clip-rule="evenodd" />
          </svg>
          {{ formatInstalls(skill.installs) }}
        </span>
      </div>
    </div>

    <!-- Action -->
    <div class="shrink-0 pt-0.5">
      <span
        v-if="skill.installed"
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
