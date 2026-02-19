<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { useSkillsStore } from '@/stores/skills';
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';

const router = useRouter();
const store = useSkillsStore();
const serversStore = useServersStore();
const { installedSkills, localSkills, selectedSkillId, selectedKind } = storeToRefs(store);

// Group local skills by tool
const localGroups = computed(() => {
  const groups: Record<string, { toolName: string; skills: typeof localSkills.value }> = {};
  for (const skill of localSkills.value) {
    if (!groups[skill.toolId]) {
      groups[skill.toolId] = { toolName: skill.toolName, skills: [] };
    }
    groups[skill.toolId].skills.push(skill);
  }
  return Object.values(groups);
});

function onSelectInstalled(id: string) {
  store.selectSkill(id);
  serversStore.selectedServerId = null;
  router.push('/skills');
}

function onSelectLocal(id: string, filePath: string) {
  store.selectLocalSkill(id, filePath);
  serversStore.selectedServerId = null;
  router.push('/skills');
}
</script>

<template>
  <div>
    <!-- Marketplace-installed skills -->
    <div
      v-for="skill in installedSkills"
      :key="skill.id"
      class="flex cursor-pointer items-center gap-2 border-b border-border/50 px-3 py-2 transition-colors hover:bg-surface-2"
      :class="selectedSkillId === skill.id && selectedKind === 'installed' ? 'bg-surface-2' : ''"
      @click="onSelectInstalled(skill.id)"
    >
      <span
        class="h-1.5 w-1.5 shrink-0 rounded-full"
        :class="skill.enabled ? 'bg-status-connected' : 'bg-surface-3'"
      />
      <span class="truncate text-xs" :class="skill.enabled ? '' : 'text-text-muted'">{{ skill.name }}</span>
      <span
        v-if="skill.managed"
        class="ml-auto shrink-0 rounded bg-status-connected/10 px-1.5 py-0.5 text-[9px] font-medium text-status-connected"
      >
        Managed
      </span>
    </div>

    <!-- Local skill groups -->
    <template v-for="group in localGroups" :key="group.toolName">
      <div class="border-b border-border/50 px-3 py-1.5">
        <span class="font-mono text-[10px] font-medium tracking-wide text-text-muted uppercase">{{ group.toolName }}</span>
      </div>
      <div
        v-for="skill in group.skills"
        :key="skill.id"
        class="flex cursor-pointer items-center gap-2 border-b border-border/50 px-3 py-2 transition-colors hover:bg-surface-2"
        :class="selectedSkillId === skill.id && selectedKind === 'local' ? 'bg-surface-2' : ''"
        @click="onSelectLocal(skill.id, skill.filePath)"
      >
        <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-surface-3" />
        <span class="truncate text-xs text-text-muted">{{ skill.name }}</span>
      </div>
    </template>

    <div
      v-if="installedSkills.length === 0 && localSkills.length === 0"
      class="px-3 py-6 text-center text-xs text-text-muted"
    >
      No skills found
    </div>
  </div>
</template>
