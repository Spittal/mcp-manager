<script setup lang="ts">
import { useSkillsStore } from '@/stores/skills';
import { storeToRefs } from 'pinia';
import { computed, ref } from 'vue';
import MarkdownIt from 'markdown-it';

const md = new MarkdownIt();

const store = useSkillsStore();
const { selectedSkillId, selectedKind, skillContent, localSkillContent, installedSkills, localSkills } = storeToRefs(store);

const selectedInstalledSkill = computed(() =>
  installedSkills.value.find(s => s.id === selectedSkillId.value)
);

const selectedLocalSkill = computed(() =>
  localSkills.value.find(s => s.id === selectedSkillId.value)
);

const renderedContent = computed(() => {
  const raw = selectedKind.value === 'installed'
    ? skillContent.value?.content
    : localSkillContent.value?.content;
  if (!raw) return '';
  return md.render(raw);
});

const confirmUninstall = ref(false);

async function onToggle() {
  if (!selectedInstalledSkill.value) return;
  await store.toggleSkill(selectedInstalledSkill.value.id, !selectedInstalledSkill.value.enabled);
}

async function onUninstall() {
  if (!selectedInstalledSkill.value) return;
  await store.uninstallSkill(selectedInstalledSkill.value.id);
  confirmUninstall.value = false;
}
</script>

<template>
  <!-- INSTALLED (marketplace) skill detail -->
  <div v-if="selectedKind === 'installed' && selectedInstalledSkill && skillContent" class="flex h-full flex-col">
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">{{ selectedInstalledSkill.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedInstalledSkill.source }}</span>
      <span
        v-if="selectedInstalledSkill.managed"
        class="rounded bg-status-connected/10 px-1.5 py-0.5 text-[10px] font-medium text-status-connected"
      >
        Managed
      </span>
      <span
        v-if="selectedInstalledSkill.installs != null"
        class="flex items-center gap-0.5 text-[10px] text-text-muted"
      >
        <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3" viewBox="0 0 20 20" fill="currentColor">
          <path fill-rule="evenodd" d="M3 17a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm3.293-7.707a1 1 0 011.414 0L9 10.586V3a1 1 0 112 0v7.586l1.293-1.293a1 1 0 111.414 1.414l-3 3a1 1 0 01-1.414 0l-3-3a1 1 0 010-1.414z" clip-rule="evenodd" />
        </svg>
        {{ selectedInstalledSkill.installs.toLocaleString() }}
      </span>
      <div class="ml-auto flex items-center gap-2">
        <button
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none"
          :class="selectedInstalledSkill.enabled ? 'bg-status-connected' : 'bg-surface-3'"
          @click="onToggle"
        >
          <span
            class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out"
            :class="selectedInstalledSkill.enabled ? 'translate-x-4' : 'translate-x-0'"
          />
        </button>
        <template v-if="!selectedInstalledSkill.managed">
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
        </template>
      </div>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto p-4">
      <section v-if="selectedInstalledSkill.description" class="mb-4">
        <p class="text-xs text-text-secondary">{{ selectedInstalledSkill.description }}</p>
      </section>
      <section>
        <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">SKILL.md</h2>
        <div class="prose-skill rounded border border-border bg-surface-1 p-3 text-xs leading-relaxed text-text-secondary" v-html="renderedContent" />
      </section>
    </div>
  </div>

  <!-- LOCAL skill detail -->
  <div v-else-if="selectedKind === 'local' && selectedLocalSkill && localSkillContent" class="flex h-full flex-col">
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">{{ selectedLocalSkill.name }}</h1>
      <span class="rounded bg-surface-3 px-1.5 py-0.5 text-[10px] font-medium text-text-muted">{{ selectedLocalSkill.toolName }}</span>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto p-4">
      <section v-if="selectedLocalSkill.description" class="mb-4">
        <p class="text-xs text-text-secondary">{{ selectedLocalSkill.description }}</p>
      </section>
      <section class="mb-4">
        <div class="truncate font-mono text-[10px] text-text-muted">{{ selectedLocalSkill.filePath }}</div>
      </section>
      <section>
        <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">SKILL.md</h2>
        <div class="prose-skill rounded border border-border bg-surface-1 p-3 text-xs leading-relaxed text-text-secondary" v-html="renderedContent" />
      </section>
    </div>
  </div>

  <!-- No selection -->
  <div v-else-if="!selectedSkillId" class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No skill selected</p>
      <p class="text-xs">Select a skill from the sidebar or browse the marketplace.</p>
    </div>
  </div>
</template>
