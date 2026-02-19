<script setup lang="ts">
import { useSkillsStore } from '@/stores/skills';
import { storeToRefs } from 'pinia';

const store = useSkillsStore();
const { selectedSkillId, skillContent, installedSkills } = storeToRefs(store);

const selectedSkill = computed(() =>
  installedSkills.value.find(s => s.id === selectedSkillId.value)
);

import { computed, ref } from 'vue';

const confirmUninstall = ref(false);

async function onToggle() {
  if (!selectedSkill.value) return;
  await store.toggleSkill(selectedSkill.value.id, !selectedSkill.value.enabled);
}

async function onUninstall() {
  if (!selectedSkill.value) return;
  await store.uninstallSkill(selectedSkill.value.id);
  confirmUninstall.value = false;
}
</script>

<template>
  <div v-if="selectedSkill && skillContent" class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">{{ selectedSkill.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedSkill.source }}</span>
      <span
        v-if="selectedSkill.installs != null"
        class="flex items-center gap-0.5 text-[10px] text-text-muted"
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clip-rule="evenodd" />
        </svg>
        {{ selectedSkill.installs.toLocaleString() }}
      </span>
      <div class="ml-auto flex items-center gap-2">
        <!-- Toggle -->
        <button
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none"
          :class="selectedSkill.enabled ? 'bg-status-connected' : 'bg-surface-3'"
          @click="onToggle"
        >
          <span
            class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out"
            :class="selectedSkill.enabled ? 'translate-x-4' : 'translate-x-0'"
          />
        </button>
        <!-- Uninstall -->
        <template v-if="confirmUninstall">
          <button
            class="rounded bg-status-error px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-status-error/80"
            @click="onUninstall"
          >
            Confirm
          </button>
          <button
            class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2"
            @click="confirmUninstall = false"
          >
            Cancel
          </button>
        </template>
        <button
          v-else
          class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2"
          @click="confirmUninstall = true"
        >
          Uninstall
        </button>
      </div>
    </header>

    <!-- Content -->
    <div class="min-h-0 flex-1 overflow-y-auto p-4">
      <!-- Description -->
      <section v-if="selectedSkill.description" class="mb-4">
        <p class="text-xs text-text-secondary">{{ selectedSkill.description }}</p>
      </section>

      <!-- SKILL.md content -->
      <section>
        <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">SKILL.md</h2>
        <pre class="whitespace-pre-wrap break-words rounded border border-border bg-surface-1 p-3 font-mono text-xs leading-relaxed text-text-secondary">{{ skillContent.content }}</pre>
      </section>
    </div>
  </div>

  <div v-else-if="!selectedSkillId" class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No skill selected</p>
      <p class="text-xs">Select a skill from the sidebar or browse the marketplace.</p>
    </div>
  </div>
</template>
