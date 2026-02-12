<script setup lang="ts">
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Settings</h1>
    </header>
    <div class="flex-1 overflow-y-auto p-4">
      <div class="mx-auto max-w-md space-y-6">
        <section>
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Proxy</h2>
          <p class="text-xs text-text-secondary">
            MCP Manager exposes all connected servers as a single MCP proxy server.
            Configure Claude Desktop to connect to this app instead of individual servers.
          </p>
        </section>

        <section>
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Runtimes</h2>
          <p class="text-xs text-text-muted">Runtime detection coming soon.</p>
        </section>
      </div>
    </div>
  </div>
</template>
