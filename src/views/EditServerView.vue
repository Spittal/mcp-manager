<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import ServerForm from '@/components/ServerForm.vue';

const route = useRoute();
const router = useRouter();
const store = useServersStore();

const serverId = route.params.id as string;
const showDeleteConfirm = ref(false);
const initialValues = ref<{
  name: string;
  transport: 'stdio' | 'http';
  command: string;
  args: string;
  url: string;
  headers: string;
}>();

onMounted(async () => {
  if (!store.servers.length) {
    await store.loadServers();
  }
  const server = store.servers.find(s => s.id === serverId);
  if (!server) {
    router.push('/');
    return;
  }
  initialValues.value = {
    name: server.name,
    transport: server.transport,
    command: server.command ?? '',
    args: server.args?.join(' ') ?? '',
    url: server.url ?? '',
    headers: server.headers
      ? Object.entries(server.headers).map(([k, v]) => `${k}: ${v}`).join('\n')
      : '',
  };
});

function parseHeaders(raw: string): Record<string, string> {
  const parsed: Record<string, string> = {};
  for (const line of raw.split('\n')) {
    const idx = line.indexOf(':');
    if (idx > 0) {
      parsed[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
    }
  }
  return parsed;
}

async function onSubmit(values: { name: string; transport: 'stdio' | 'http'; command: string; args: string; url: string; headers: string }) {
  try {
    await store.updateServer(serverId, {
      name: values.name.trim(),
      transport: values.transport,
      enabled: true,
      ...(values.transport === 'stdio'
        ? {
            command: values.command.trim(),
            args: values.args.split(/\s+/).filter(Boolean),
          }
        : {
            url: values.url.trim(),
            headers: parseHeaders(values.headers),
          }),
    });
    router.push('/');
  } catch {
    // error already logged by store
  }
}

async function deleteServer() {
  await store.removeServer(serverId);
  router.push('/');
}
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Edit Server</h1>
    </header>

    <ServerForm
      v-if="initialValues"
      :initial="initialValues"
      submit-label="Save"
      @submit="onSubmit"
    >
      <template #actions>
        <button
          type="button"
          class="ml-auto rounded bg-status-error/10 px-4 py-2 text-xs font-medium text-status-error transition-colors hover:bg-status-error/20"
          @click="showDeleteConfirm = true"
        >
          Delete
        </button>
      </template>

      <template #footer>
        <div v-if="showDeleteConfirm" class="rounded border border-status-error/30 bg-status-error/10 p-3">
          <p class="mb-2 text-xs text-text-secondary">Are you sure you want to delete this server?</p>
          <div class="flex gap-2">
            <button
              type="button"
              class="rounded bg-status-error px-3 py-1 text-xs text-white transition-colors hover:bg-status-error/80"
              @click="deleteServer"
            >
              Delete
            </button>
            <button
              type="button"
              class="rounded bg-surface-3 px-3 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-2"
              @click="showDeleteConfirm = false"
            >
              Cancel
            </button>
          </div>
        </div>
      </template>
    </ServerForm>
  </div>
</template>
