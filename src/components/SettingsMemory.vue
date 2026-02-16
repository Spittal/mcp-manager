<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useServersStore } from '@/stores/servers';
import type { MemoryStatus } from '@/types/memory';
import type { EmbeddingConfigStatus, EmbeddingProvider, EmbeddingModelInfo } from '@/types/embedding';
import { OLLAMA_MODELS, OPENAI_MODELS } from '@/types/embedding';
import ToggleCard from './ToggleCard.vue';

const store = useServersStore();

// Memory status
const status = ref<MemoryStatus | null>(null);
const toggling = ref(false);
const error = ref<string | null>(null);
const progress = ref<string | null>(null);

// Embedding config
const embeddingStatus = ref<EmbeddingConfigStatus | null>(null);
const provider = ref<EmbeddingProvider>('ollama');
const model = ref('mxbai-embed-large');
const dimensions = ref(1024);
const customModel = ref(false);
const customName = ref('');
const customDims = ref(512);
const openaiApiKey = ref('');
const showApiKey = ref(false);
const saving = ref(false);
const saved = ref(false);
const deletingModel = ref<string | null>(null);

const canEnable = computed(() => {
  if (!status.value) return false;
  return status.value.dockerAvailable;
});

const currentModels = computed<EmbeddingModelInfo[]>(() =>
  provider.value === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS
);

const dirty = computed(() => {
  if (!embeddingStatus.value) return false;
  const s = embeddingStatus.value.config;
  const currentModel = customModel.value ? customName.value : model.value;
  const currentDims = customModel.value ? customDims.value : dimensions.value;
  return (
    provider.value !== s.provider ||
    currentModel !== s.model ||
    currentDims !== s.dimensions ||
    (provider.value === 'openai' && openaiApiKey.value.length > 0)
  );
});

const modelChanged = computed(() => {
  if (!status.value?.enabled || !embeddingStatus.value) return false;
  const s = embeddingStatus.value.config;
  const currentModel = customModel.value ? customName.value : model.value;
  const currentDims = customModel.value ? customDims.value : dimensions.value;
  return (
    provider.value !== s.provider ||
    currentModel !== s.model ||
    currentDims !== s.dimensions
  );
});

const description = computed(() => {
  if (provider.value === 'openai') {
    return 'Shared long-term memory across all connected AI tools. Uses OpenAI API for embeddings. Requires Docker (for Redis) and an API key.';
  }
  return 'Shared long-term memory across all connected AI tools. Uses local Ollama models for embeddings — no API keys needed. Requires Docker.';
});

const memorySubtitle = computed<string | undefined>(() => {
  if (toggling.value && progress.value) return undefined; // handled by slot
  if (status.value?.enabled && status.value.serverStatus === 'connected') return 'Connected';
  if (status.value?.enabled && status.value.serverStatus === 'connecting') return 'Connecting...';
  if (status.value?.enabled) return 'Enabled';
  return undefined;
});

const memorySubtitleClass = computed(() => {
  if (status.value?.serverStatus === 'connecting') return 'text-status-connecting';
  return 'text-text-muted';
});

async function fetchStatus() {
  try {
    status.value = await invoke<MemoryStatus>('get_memory_status');
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}

async function fetchEmbeddingConfig() {
  try {
    embeddingStatus.value = await invoke<EmbeddingConfigStatus>('get_embedding_config');
    const cfg = embeddingStatus.value.config;
    provider.value = cfg.provider;

    const models = cfg.provider === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS;
    const known = models.find(m => m.model === cfg.model);
    if (known) {
      model.value = known.model;
      dimensions.value = known.dimensions;
      customModel.value = false;
    } else {
      customModel.value = true;
      customName.value = cfg.model;
      customDims.value = cfg.dimensions;
    }
  } catch {
    // Non-critical, defaults will be used
  }
}

async function toggle() {
  if (!status.value) return;
  toggling.value = true;
  error.value = null;
  progress.value = null;
  try {
    if (status.value.enabled) {
      await invoke('disable_memory');
      await store.loadServers();
    } else {
      await invoke('enable_memory');
      await store.loadServers();
      store.autoConnectServers();
    }
    await fetchStatus();
  } catch (e) {
    error.value = String(e);
  } finally {
    toggling.value = false;
    progress.value = null;
  }
}

function selectModel(m: EmbeddingModelInfo) {
  model.value = m.model;
  dimensions.value = m.dimensions;
  customModel.value = false;
  saved.value = false;
}

function selectCustomModel() {
  customModel.value = true;
  saved.value = false;
}

function switchProvider(p: EmbeddingProvider) {
  provider.value = p;
  customModel.value = false;
  saved.value = false;
  const models = p === 'ollama' ? OLLAMA_MODELS : OPENAI_MODELS;
  const recommended = models.find(m => m.recommended) ?? models[0];
  model.value = recommended.model;
  dimensions.value = recommended.dimensions;
}

async function saveSettings() {
  saving.value = true;
  saved.value = false;
  try {
    const m = customModel.value ? customName.value : model.value;
    const d = customModel.value ? customDims.value : dimensions.value;
    await invoke('save_embedding_config_cmd', {
      input: {
        config: { provider: provider.value, model: m, dimensions: d },
        openaiApiKey: provider.value === 'openai' && openaiApiKey.value ? openaiApiKey.value : null,
      },
    });
    openaiApiKey.value = '';
    saved.value = true;
    await fetchEmbeddingConfig();
    await fetchStatus();
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

async function deleteModel(m: string) {
  deletingModel.value = m;
  try {
    await invoke('delete_ollama_model', { model: m });
    await fetchEmbeddingConfig();
  } catch (e) {
    error.value = String(e);
  } finally {
    deletingModel.value = null;
  }
}

function isModelPulled(m: string): boolean {
  return embeddingStatus.value?.pulledOllamaModels.includes(m) ?? false;
}

let unlistenProgress: UnlistenFn | null = null;
let unlistenStatus: UnlistenFn | null = null;

onMounted(async () => {
  fetchStatus();
  fetchEmbeddingConfig();
  unlistenProgress = await listen<{ message: string }>('memory-progress', (event) => {
    progress.value = event.payload.message;
  });
  unlistenStatus = await listen('server-status-changed', () => {
    fetchStatus();
  });
});

onUnmounted(() => {
  unlistenProgress?.();
  unlistenStatus?.();
});
</script>

<template>
  <div>
    <h2 class="mb-1 text-xs font-medium text-text-primary">Memory</h2>
    <p class="mb-4 text-xs text-text-secondary">{{ description }}</p>

    <div v-if="error" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
      {{ error }}
    </div>

    <div v-if="status" class="space-y-4">
      <ToggleCard
        label="Agent Memory"
        :enabled="status.enabled"
        :toggling="toggling"
        :can-enable="canEnable"
        :subtitle="memorySubtitle"
        :subtitle-class="memorySubtitleClass"
        @toggle="toggle"
      >
        <template v-if="toggling && progress" #subtitle>
          <span class="flex items-center gap-1.5 text-[10px] text-text-secondary">
            <span class="h-1 w-1 animate-pulse rounded-full bg-accent" />
            {{ progress }}
          </span>
        </template>

        <!-- Prerequisite warnings -->
        <div v-if="!status.enabled && !status.dockerAvailable" class="border-t border-border/50 px-3 py-2 space-y-1">
          <div class="flex items-center gap-1.5 text-[11px] text-status-connecting">
            <span>&#9888;</span>
            <span>Docker not found</span>
          </div>
        </div>
      </ToggleCard>

      <!-- Embedding config panel -->
      <div class="rounded border border-border bg-surface-1">
        <div class="px-3 py-2.5">
          <span class="text-xs font-medium text-text-secondary">Embedding Model</span>
        </div>

        <!-- Provider tabs -->
        <div class="flex border-t border-border/50">
          <button
            class="flex-1 py-1.5 text-[11px] font-medium transition-colors"
            :class="provider === 'ollama'
              ? 'bg-surface-2 text-text-primary'
              : 'text-text-muted hover:text-text-secondary'"
            @click="switchProvider('ollama')"
          >
            Ollama
          </button>
          <button
            class="flex-1 py-1.5 text-[11px] font-medium transition-colors border-l border-border/50"
            :class="provider === 'openai'
              ? 'bg-surface-2 text-text-primary'
              : 'text-text-muted hover:text-text-secondary'"
            @click="switchProvider('openai')"
          >
            OpenAI
          </button>
        </div>

        <div class="border-t border-border/50 px-3 py-2.5 space-y-2">
          <!-- Model list -->
          <div class="space-y-1">
            <div
              v-for="m in currentModels"
              :key="m.model"
              class="flex items-center gap-2 rounded px-2 py-1.5 cursor-pointer transition-colors"
              :class="!customModel && model === m.model
                ? 'bg-accent/10 ring-1 ring-accent/30'
                : 'hover:bg-surface-2'"
              @click="selectModel(m)"
            >
              <span class="flex items-center gap-1.5 min-w-0 flex-1">
                <span
                  class="h-3 w-3 shrink-0 rounded-full border-2 flex items-center justify-center"
                  :class="!customModel && model === m.model
                    ? 'border-accent'
                    : 'border-surface-3'"
                >
                  <span
                    v-if="!customModel && model === m.model"
                    class="h-1.5 w-1.5 rounded-full bg-accent"
                  />
                </span>
                <span class="font-mono text-[11px] text-text-primary">{{ m.displayName }}</span>
                <span v-if="m.recommended" class="text-[9px] font-medium text-accent uppercase tracking-wide">Recommended</span>
                <span
                  v-if="provider === 'ollama' && isModelPulled(m.model)"
                  class="text-[9px] text-status-connected"
                  title="Downloaded"
                >&#10003;</span>
              </span>
              <span class="shrink-0 flex items-center gap-2">
                <span class="text-[10px] text-text-muted">{{ m.dimensions }}d</span>
                <button
                  v-if="provider === 'ollama' && isModelPulled(m.model)"
                  class="text-[10px] text-text-muted transition-colors hover:text-status-error"
                  :disabled="deletingModel === m.model"
                  title="Delete downloaded model"
                  @click.stop="deleteModel(m.model)"
                >
                  {{ deletingModel === m.model ? '...' : 'Delete' }}
                </button>
                <span v-else class="text-[10px] text-text-muted">{{ m.sizeLabel }}</span>
              </span>
            </div>

            <!-- Custom model option (Ollama only) -->
            <div
              v-if="provider === 'ollama'"
              class="flex items-center gap-2 rounded px-2 py-1.5 cursor-pointer transition-colors"
              :class="customModel
                ? 'bg-accent/10 ring-1 ring-accent/30'
                : 'hover:bg-surface-2'"
              @click="selectCustomModel"
            >
              <span
                class="h-3 w-3 shrink-0 rounded-full border-2 flex items-center justify-center"
                :class="customModel ? 'border-accent' : 'border-surface-3'"
              >
                <span v-if="customModel" class="h-1.5 w-1.5 rounded-full bg-accent" />
              </span>
              <span class="text-[11px] text-text-secondary">Custom model...</span>
            </div>
          </div>

          <!-- Custom model inputs -->
          <div v-if="customModel && provider === 'ollama'" class="flex gap-2 pl-5">
            <input
              v-model="customName"
              type="text"
              placeholder="Model name (e.g. bge-m3)"
              class="flex-1 rounded border border-border bg-surface-0 px-2 py-1.5 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
            />
            <input
              v-model.number="customDims"
              type="number"
              min="1"
              placeholder="Dims"
              class="w-20 rounded border border-border bg-surface-0 px-2 py-1.5 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
            />
          </div>

          <!-- OpenAI API key input -->
          <div v-if="provider === 'openai'" class="space-y-1.5">
            <div class="flex items-center gap-2">
              <div class="relative flex-1">
                <input
                  v-model="openaiApiKey"
                  :type="showApiKey ? 'text' : 'password'"
                  placeholder="sk-..."
                  class="w-full rounded border border-border bg-surface-0 px-2 py-1.5 pr-14 font-mono text-[11px] text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
                />
                <button
                  class="absolute top-1/2 right-2 -translate-y-1/2 text-[10px] text-text-muted hover:text-text-secondary"
                  @click="showApiKey = !showApiKey"
                >
                  {{ showApiKey ? 'Hide' : 'Show' }}
                </button>
              </div>
            </div>
            <div class="flex items-center gap-2 text-[10px]">
              <span v-if="embeddingStatus?.hasOpenaiKey" class="text-status-connected">&#10003; Key saved</span>
              <span v-else class="text-text-muted">No API key saved</span>
              <span class="text-text-muted">&middot;</span>
              <a href="https://platform.openai.com/api-keys" target="_blank" class="text-accent hover:underline">
                Get an API key
              </a>
            </div>
          </div>

          <!-- Model change warning -->
          <div v-if="modelChanged" class="rounded bg-status-connecting/10 px-2.5 py-2 text-[11px] text-status-connecting">
            Changing the embedding model requires disabling and re-enabling Memory.
            Existing memories indexed with a different model won't be searchable.
          </div>

          <!-- Save button -->
          <div class="flex items-center gap-2 pt-1">
            <button
              class="rounded bg-accent px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
              :disabled="!dirty || saving"
              @click="saveSettings"
            >
              {{ saving ? 'Saving...' : 'Save' }}
            </button>
            <span v-if="saved && !dirty" class="text-[10px] text-status-connected">&#10003; Saved</span>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="text-xs text-text-muted">Loading memory status...</div>
  </div>
</template>
