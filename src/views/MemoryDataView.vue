<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useMemoriesStore } from '@/stores/memories';

const exporting = ref(false);
const exportCount = ref(0);
const exportDone = ref<number | null>(null);
const exportError = ref<string | null>(null);

const importing = ref(false);
const importProgress = ref<{ imported: number; totalLines: number } | null>(null);
const importDone = ref<number | null>(null);
const importError = ref<string | null>(null);

const formatError = ref<string | null>(null);
const showFormatModal = ref(false);
const formatInput = ref('');
const formatting = ref(false);
const formatSuccess = ref(false);

const memoriesStore = useMemoriesStore();
let unlistenExport: UnlistenFn | null = null;
let unlistenImport: UnlistenFn | null = null;

async function doExport() {
  exportError.value = null;
  exportDone.value = null;
  exportCount.value = 0;

  const date = new Date().toISOString().slice(0, 10);
  const defaultName = `mcp-memories-export-${date}.jsonl`;

  const filePath = await save({
    defaultPath: `/tmp/${defaultName}`,
    filters: [{ name: 'JSONL', extensions: ['jsonl'] }],
  });

  if (!filePath) return;

  exporting.value = true;
  try {
    const count = await invoke<number>('export_memories', { path: filePath });
    exportDone.value = count;
  } catch (e) {
    exportError.value = String(e);
  } finally {
    exporting.value = false;
  }
}

async function doImport() {
  importError.value = null;
  importDone.value = null;
  importProgress.value = null;

  const filePath = await open({
    filters: [{ name: 'JSONL', extensions: ['jsonl'] }],
    multiple: false,
  });

  if (!filePath) return;

  importing.value = true;
  try {
    const count = await invoke<number>('import_memories', { path: filePath });
    importDone.value = count;
    memoriesStore.indexing = true;
  } catch (e) {
    importError.value = String(e);
  } finally {
    importing.value = false;
  }
}

function openFormatModal() {
  formatInput.value = '';
  formatError.value = null;
  formatSuccess.value = false;
  showFormatModal.value = true;
}

async function doFormat() {
  formatError.value = null;
  formatting.value = true;
  try {
    await invoke('format_memory_data', { confirmation: formatInput.value });
    formatSuccess.value = true;
    showFormatModal.value = false;
    memoriesStore.indexing = true;
    memoriesStore.reset();
  } catch (e) {
    formatError.value = String(e);
  } finally {
    formatting.value = false;
  }
}

onMounted(async () => {
  unlistenExport = await listen<{ exported: number }>('export-progress', (event) => {
    exportCount.value = event.payload.exported;
  });
  unlistenImport = await listen<{ imported: number; totalLines: number }>('import-progress', (event) => {
    importProgress.value = event.payload;
  });
});

onUnmounted(() => {
  unlistenExport?.();
  unlistenImport?.();
});
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Data Management</h1>
    </header>

    <div class="flex-1 overflow-y-auto p-4">
      <div class="mx-auto max-w-lg space-y-6">

        <!-- Export -->
        <div class="rounded border border-border bg-surface-1 p-4 space-y-3">
          <div>
            <h2 class="text-xs font-medium text-text-primary">Export Memories</h2>
            <p class="mt-1 text-[11px] text-text-secondary">
              Export all memories to a JSONL file. This file can be imported into another agent-memory-server instance.
            </p>
          </div>

          <div v-if="exportError" class="rounded bg-status-error/10 px-3 py-2 text-[11px] text-status-error">
            {{ exportError }}
          </div>

          <div v-if="exporting" class="flex items-center gap-2 text-[11px] text-text-secondary">
            <span class="h-1 w-1 animate-pulse rounded-full bg-accent" />
            Exporting... {{ exportCount }} memories
          </div>

          <div v-if="exportDone !== null" class="rounded bg-status-connected/10 px-3 py-2 text-[11px] text-status-connected">
            Exported {{ exportDone }} memories.
          </div>

          <button
            class="rounded bg-accent px-3 py-1.5 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
            :disabled="exporting"
            @click="doExport"
          >
            {{ exporting ? 'Exporting...' : 'Export' }}
          </button>
        </div>

        <!-- Import -->
        <div class="rounded border border-border bg-surface-1 p-4 space-y-3">
          <div>
            <h2 class="text-xs font-medium text-text-primary">Import Memories</h2>
            <p class="mt-1 text-[11px] text-text-secondary">
              Import memories from a JSONL file. Memories are added to existing data, not replaced. Duplicates are automatically skipped.
            </p>
          </div>

          <div v-if="importError" class="rounded bg-status-error/10 px-3 py-2 text-[11px] text-status-error">
            {{ importError }}
          </div>

          <div v-if="importing && importProgress" class="flex items-center gap-2 text-[11px] text-text-secondary">
            <span class="h-1 w-1 animate-pulse rounded-full bg-accent" />
            Importing... {{ importProgress.imported }} / {{ importProgress.totalLines }}
          </div>

          <div v-if="importDone !== null" class="rounded bg-status-connected/10 px-3 py-2 text-[11px] text-status-connected">
            Imported {{ importDone }} memories.
          </div>

          <button
            class="rounded bg-accent px-3 py-1.5 text-[11px] font-medium text-white transition-colors hover:bg-accent-hover disabled:opacity-50"
            :disabled="importing"
            @click="doImport"
          >
            {{ importing ? 'Importing...' : 'Import' }}
          </button>
        </div>

        <!-- Format -->
        <div class="rounded border border-status-error/30 bg-status-error/5 p-4 space-y-3">
          <div>
            <h2 class="text-xs font-medium text-status-error">Format Memory Data</h2>
            <p class="mt-1 text-[11px] text-text-secondary">
              This will permanently delete all memories stored in Redis. This action is irreversible.
              Consider exporting your data first.
            </p>
          </div>

          <div v-if="formatSuccess" class="rounded bg-status-connected/10 px-3 py-2 text-[11px] text-status-connected">
            All memory data has been cleared.
          </div>

          <button
            class="rounded border border-status-error/50 bg-transparent px-3 py-1.5 text-[11px] font-medium text-status-error transition-colors hover:bg-status-error/10"
            @click="openFormatModal"
          >
            Format
          </button>
        </div>
      </div>
    </div>

    <!-- Format confirmation modal -->
    <Teleport to="body">
      <div
        v-if="showFormatModal"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
        @click.self="showFormatModal = false"
      >
        <div class="w-96 rounded-lg border border-border bg-surface-1 p-5 shadow-lg space-y-4">
          <h3 class="text-sm font-medium text-status-error">Confirm Format</h3>
          <p class="text-[11px] text-text-secondary">
            This will permanently delete all memories. Type
            <strong class="text-text-primary">format my data</strong>
            to confirm.
          </p>

          <input
            v-model="formatInput"
            type="text"
            placeholder="Type 'format my data'"
            class="w-full rounded border border-border bg-surface-0 px-3 py-2 text-xs text-text-primary placeholder:text-text-muted focus:border-status-error focus:outline-none"
            @keydown.enter="formatInput === 'format my data' && doFormat()"
          />

          <div v-if="formatError" class="rounded bg-status-error/10 px-3 py-2 text-[11px] text-status-error">
            {{ formatError }}
          </div>

          <div class="flex items-center justify-end gap-2">
            <button
              class="rounded border border-border px-3 py-1.5 text-[11px] text-text-muted transition-colors hover:bg-surface-2"
              @click="showFormatModal = false"
            >
              Cancel
            </button>
            <button
              class="rounded bg-status-error px-3 py-1.5 text-[11px] font-medium text-white transition-colors hover:bg-status-error/80 disabled:opacity-50"
              :disabled="formatInput !== 'format my data' || formatting"
              @click="doFormat"
            >
              {{ formatting ? 'Formatting...' : 'Format All Data' }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
