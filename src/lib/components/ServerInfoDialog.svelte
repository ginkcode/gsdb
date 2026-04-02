<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { invoke } from "@tauri-apps/api/core";
  import type { Connection, ServerInfo } from "$lib/types";

  interface Props {
    connection: Connection | null;
    open: boolean;
    onClose: () => void;
  }

  let { connection, open, onClose }: Props = $props();

  let serverInfo = $state<ServerInfo | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  async function loadServerInfo() {
    if (!connection) return;
    loading = true;
    error = null;
    try {
      const info = await invoke<ServerInfo>("get_server_info", {
        connectionId: connection.id,
      });
      serverInfo = info;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open && connection) {
      loadServerInfo();
    }
  });

  const driverNames: Record<string, string> = {
    postgres: "PostgreSQL",
    mysql: "MySQL",
    sqlite: "SQLite",
    sqlserver: "SQL Server",
  };
</script>

<Dialog.Root {open} onOpenChange={(v) => !v && onClose()}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>Server Info</Dialog.Title>
      <Dialog.Description>
        {#if connection}
          {driverNames[connection.driver] ?? connection.driver}
          {#if connection.host}
            - {connection.host}{#if connection.port}:{connection.port}{/if}
          {/if}
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    {#if loading}
      <div class="flex items-center justify-center py-8">
        <div class="animate-spin w-6 h-6 border-2 border-primary border-t-transparent rounded-full"></div>
      </div>
    {:else if error}
      <div class="text-destructive text-sm py-4">{error}</div>
    {:else if serverInfo}
      <div class="grid gap-3 py-4">
        {#if serverInfo.version}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Version</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.version}</span>
          </div>
        {/if}
        {#if serverInfo.databaseName}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Database</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.databaseName}</span>
          </div>
        {/if}
        {#if serverInfo.host}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Host</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.host}</span>
          </div>
        {/if}
        {#if serverInfo.port}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Port</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.port}</span>
          </div>
        {/if}
        {#if serverInfo.connections !== null && serverInfo.connections !== undefined}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Connections</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.connections}</span>
          </div>
        {/if}
        {#if serverInfo.size}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Size</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.size}</span>
          </div>
        {/if}
        {#if serverInfo.uptime}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">Uptime</span>
            <span class="col-span-2 text-sm font-mono">{serverInfo.uptime}</span>
          </div>
        {/if}
        {#each serverInfo.extra as [key, value]}
          <div class="grid grid-cols-3 gap-2">
            <span class="text-muted-foreground text-sm">{key}</span>
            <span class="col-span-2 text-sm font-mono">{value}</span>
          </div>
        {/each}
      </div>
    {/if}

    <Dialog.Footer>
      <button
        type="button"
        class="inline-flex items-center justify-center rounded-md text-sm font-medium px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90"
        onclick={() => onClose()}
      >
        Close
      </button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>