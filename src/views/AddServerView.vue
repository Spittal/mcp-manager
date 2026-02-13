<script setup lang="ts">
import { useRouter } from 'vue-router';
import { useServersStore } from '@/stores/servers';
import ServerForm from '@/components/ServerForm.vue';

const router = useRouter();
const store = useServersStore();

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
  store.addServer({
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
}
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Add Server</h1>
    </header>

    <ServerForm submit-label="Add Server" @submit="onSubmit" />
  </div>
</template>
