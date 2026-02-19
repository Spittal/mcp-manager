<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { useSkillsStore } from '@/stores/skills';
import type { MarketplaceSkillSummary } from '@/types/skill';
import SkillMarketplaceCard from '@/components/SkillMarketplaceCard.vue';

const router = useRouter();
const store = useSkillsStore();

const searchInput = ref('');
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
const installingSkillId = ref<string | null>(null);

onMounted(() => {
  store.searchMarketplace('');
});

function onSearchInput() {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    store.searchMarketplace(searchInput.value);
  }, 300);
}

async function handleInstall(skill: MarketplaceSkillSummary) {
  if (skill.installed) return;
  installingSkillId.value = skill.id;
  try {
    const installed = await store.installSkill(skill);
    store.selectSkill(installed.id);
    router.push('/skills');
  } catch (e) {
    console.error('Install failed:', e);
  } finally {
    installingSkillId.value = null;
    // Refresh marketplace to update installed badges
    store.searchMarketplace(searchInput.value);
  }
}
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <div class="min-h-0 flex-1 overflow-y-auto px-6 py-4">
      <!-- Header -->
      <div class="mb-4">
        <h1 class="text-base font-semibold text-text-primary">Add Skill</h1>
      </div>

      <!-- Disclaimer -->
      <div class="mb-4 rounded-lg bg-surface-2 p-3 text-xs text-text-secondary">
        <strong>Community directory</strong> — Skills listed here are sourced from skills.sh and have not been vetted. Review skill content before installing.
      </div>

      <!-- Search -->
      <div class="relative mb-4">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd" />
        </svg>
        <input
          v-model="searchInput"
          type="text"
          placeholder="Search skills..."
          class="w-full rounded-lg border border-border bg-surface-1 py-2 pl-9 pr-3 text-xs text-text-primary placeholder-text-muted outline-none transition-colors focus:border-accent"
          @input="onSearchInput"
        />
      </div>

      <!-- Results -->
      <div>
        <!-- Loading -->
        <div v-if="store.marketplaceLoading && store.marketplaceSkills.length === 0" class="flex items-center justify-center py-12">
          <span class="text-xs text-text-muted">Loading skills...</span>
        </div>

        <!-- Error -->
        <div v-else-if="store.marketplaceError && store.marketplaceSkills.length === 0" class="flex flex-col items-center justify-center gap-3 py-12">
          <p class="text-xs text-status-error">{{ store.marketplaceError }}</p>
          <button
            class="rounded-md bg-surface-2 px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-surface-3"
            @click="store.searchMarketplace(searchInput)"
          >
            Retry
          </button>
        </div>

        <!-- Empty -->
        <div v-else-if="!store.marketplaceLoading && store.marketplaceSkills.length === 0" class="flex items-center justify-center py-12">
          <span class="text-xs text-text-muted">No skills found</span>
        </div>

        <!-- Grid -->
        <div v-else class="grid gap-2 pb-4" style="grid-template-columns: repeat(auto-fill, minmax(380px, 1fr))">
          <SkillMarketplaceCard
            v-for="skill in store.marketplaceSkills"
            :key="skill.id"
            :skill="skill"
            :installing="installingSkillId === skill.id"
            @install="handleInstall(skill)"
          />
        </div>

        <!-- Loading more -->
        <div v-if="store.marketplaceLoading && store.marketplaceSkills.length > 0" class="flex justify-center pb-4 pt-2">
          <span class="text-xs text-text-muted">Loading...</span>
        </div>
      </div>
    </div>
  </div>
</template>
