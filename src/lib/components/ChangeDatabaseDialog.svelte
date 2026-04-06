<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Loader, Database, Plus, Trash2 } from "@lucide/svelte";
  import { toast } from "svelte-sonner";
  import type { Connection } from "$lib/types";

  let {
    open = $bindable(false),
    connection,
    onSelect,
  }: {
    open: boolean;
    connection: Connection | null;
    onSelect: (database: string) => void;
  } = $props();

  let databases = $state<string[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let newDbName = $state("");
  let showNewDb = $state(false);
  let creating = $state(false);

  // Delete confirmation state
  let deleteTarget = $state<string | null>(null);
  let deleteConfirmInput = $state("");
  let deleting = $state(false);

  $effect(() => {
    if (open && connection) {
      loadDatabases(connection.id);
    } else {
      databases = [];
      error = null;
      newDbName = "";
      showNewDb = false;
      deleteTarget = null;
      deleteConfirmInput = "";
    }
  });

  async function loadDatabases(connectionId: string) {
    loading = true;
    error = null;
    try {
      databases = await invoke<string[]>("list_databases", { connectionId });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function select(db: string) {
    onSelect(db);
    open = false;
  }

  async function confirmNew() {
    const name = newDbName.trim();
    if (!name || !connection) return;
    creating = true;
    try {
      await invoke("create_database", {
        connectionId: connection.id,
        dbName: name,
      });
      onSelect(name);
      open = false;
    } catch (e) {
      toast.error("Failed to create database", { description: String(e) });
    } finally {
      creating = false;
    }
  }

  function requestDelete(db: string) {
    deleteTarget = db;
    deleteConfirmInput = "";
  }

  function cancelDelete() {
    deleteTarget = null;
    deleteConfirmInput = "";
  }

  async function confirmDelete() {
    if (!deleteTarget || !connection) return;
    if (deleteConfirmInput !== deleteTarget) return;
    deleting = true;
    try {
      await invoke("drop_database", {
        connectionId: connection.id,
        dbName: deleteTarget,
      });
      databases = databases.filter((d) => d !== deleteTarget);
      toast.success(`Database "${deleteTarget}" deleted`);
      deleteTarget = null;
      deleteConfirmInput = "";
    } catch (e) {
      toast.error("Failed to delete database", { description: String(e) });
    } finally {
      deleting = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="sm:max-w-sm"
    showCloseButton={true}
    interactOutsideBehavior={deleteTarget ? "ignore" : "close"}
  >
    <Dialog.Header>
      <Dialog.Title>Change Database</Dialog.Title>
      <Dialog.Description>
        Select a database on <span class="font-medium text-foreground"
          >{connection?.name}</span
        >
      </Dialog.Description>
    </Dialog.Header>

    <div class="mt-2 flex flex-col gap-3">
      {#if loading}
        <div
          class="flex items-center justify-center py-8 gap-2 text-sm text-muted-foreground"
        >
          <Loader class="w-4 h-4 animate-spin" />
          Loading databases…
        </div>
      {:else if error}
        <div class="py-4 text-sm text-destructive">{error}</div>
      {:else if databases.length === 0}
        <div class="py-4 text-sm text-muted-foreground text-center">
          No databases found
        </div>
      {:else if deleteTarget}
        <!-- Delete confirmation -->
        <div class="flex flex-col gap-3">
          <p class="text-sm text-muted-foreground">
            This will permanently drop <span
              class="font-mono font-medium text-foreground">{deleteTarget}</span
            >
            and all its data. Type the database name to confirm.
          </p>
          <Input
            placeholder={deleteTarget}
            bind:value={deleteConfirmInput}
            class="h-8 text-sm font-mono"
            onkeydown={(e) => {
              if (e.key === "Enter") confirmDelete();
              if (e.key === "Escape") cancelDelete();
            }}
          />
          <div class="flex gap-2 justify-end">
            <Button variant="outline" size="sm" onclick={cancelDelete}
              >Cancel</Button
            >
            <Button
              variant="destructive"
              size="sm"
              onclick={confirmDelete}
              disabled={deleteConfirmInput !== deleteTarget || deleting}
            >
              {deleting ? "Dropping…" : "Drop database"}
            </Button>
          </div>
        </div>
      {:else}
        <ScrollArea class="max-h-60">
          <div class="flex flex-col gap-0.5 pr-2">
            {#each databases as db}
              <div class="group flex items-center gap-1">
                <button
                  class="flex items-center gap-2 flex-1 min-w-0 px-3 py-2 rounded-md text-sm text-left transition-colors
                    {db === connection?.database
                    ? 'bg-accent text-accent-foreground font-medium'
                    : 'hover:bg-muted text-foreground'}"
                  onclick={() => select(db)}
                >
                  <Database class="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
                  <span class="truncate">{db}</span>
                  {#if db === connection?.database}
                    <span class="ml-auto text-xs text-muted-foreground shrink-0"
                      >current</span
                    >
                  {/if}
                </button>
                {#if db !== connection?.database && !connection?.locked}
                  <button
                    class="p-1.5 rounded-md text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-destructive hover:bg-destructive/10 transition-all shrink-0"
                    onclick={() => requestDelete(db)}
                    aria-label="Delete {db}"
                  >
                    <Trash2 class="w-3.5 h-3.5" />
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </ScrollArea>
      {/if}

      <!-- New database section — hidden while in delete confirm -->
      {#if !deleteTarget}
        <div class="border-t border-border pt-3">
          {#if connection?.locked}
            <p class="text-xs text-muted-foreground text-center py-2">
              Create database is disabled for locked connections
            </p>
          {:else if showNewDb}
            <div class="flex gap-2">
              <Input
                placeholder="New database name"
                bind:value={newDbName}
                class="h-8 text-sm"
                onkeydown={(e) => {
                  if (e.key === "Enter") confirmNew();
                  if (e.key === "Escape") showNewDb = false;
                }}
              />
              <Button
                size="sm"
                class="h-8 shrink-0"
                onclick={confirmNew}
                disabled={!newDbName.trim() || creating}
              >
                {creating ? "Creating…" : "Create & Connect"}
              </Button>
            </div>
          {:else}
            <button
              class="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              onclick={() => {
                showNewDb = true;
              }}
            >
              <Plus class="w-3.5 h-3.5 shrink-0" />
              <span>Connect to a new database</span>
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
