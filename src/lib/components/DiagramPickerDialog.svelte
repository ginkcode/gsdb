<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import type { SchemaGraph } from "$lib/types";

  let {
    open = $bindable(false),
    connectionId,
    initialSelected = [],
    onConfirm,
  }: {
    open: boolean;
    connectionId: string;
    initialSelected?: string[];
    onConfirm: (selectedTables: string[]) => void;
  } = $props();

  let schema = $state<SchemaGraph | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let search = $state("");
  let selected = $state<Set<string>>(new Set());

  // Load schema when dialog opens
  $effect(() => {
    if (open) {
      loading = true;
      error = null;
      invoke<SchemaGraph>("get_schema", { connectionId })
        .then((g) => {
          schema = g;
          // Use initialSelected if provided, otherwise select all
          if (initialSelected.length > 0) {
            selected = new Set(initialSelected);
          } else {
            selected = new Set(g.tables.map((t) => t.name));
          }
        })
        .catch((e) => { error = String(e); })
        .finally(() => { loading = false; });
    }
    if (!open) {
      // Reset for next open
      schema = null;
      search = "";
    }
  });

  let filtered = $derived(
    schema?.tables.filter((t) =>
      t.name.toLowerCase().includes(search.toLowerCase())
    ) ?? []
  );

  function toggleAll(selectAll: boolean) {
    if (selectAll) {
      selected = new Set(schema?.tables.map((t) => t.name) ?? []);
    } else {
      selected = new Set();
    }
  }

  function toggle(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    selected = next;
  }

  function confirm() {
    onConfirm([...selected]);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Select tables for diagram</Dialog.Title>
      <Dialog.Description>
        Choose which tables to include. You can rearrange them on the canvas afterward.
      </Dialog.Description>
    </Dialog.Header>

    {#if loading}
      <div class="flex items-center justify-center py-8 gap-2 text-sm text-muted-foreground">
        <span class="w-2 h-2 rounded-full bg-primary animate-bounce"></span>
        Loading schema…
      </div>
    {:else if error}
      <p class="text-sm text-destructive py-4">{error}</p>
    {:else if schema}
      <div class="flex flex-col gap-3">
        <Input bind:value={search} placeholder="Search tables…" class="h-8" />

        <div class="flex items-center justify-between text-xs text-muted-foreground px-0.5">
          <span>{selected.size} / {schema.tables.length} selected</span>
          <div class="flex gap-2">
            <button class="hover:text-foreground transition-colors" onclick={() => toggleAll(true)}>
              Select all
            </button>
            <span>·</span>
            <button class="hover:text-foreground transition-colors" onclick={() => toggleAll(false)}>
              None
            </button>
          </div>
        </div>

        <ScrollArea class="h-80 rounded border border-border">
          <div class="p-1">
            {#each filtered as table}
              <label
                class="flex items-center gap-2.5 px-2 py-1.5 rounded cursor-pointer hover:bg-accent/50 text-sm"
              >
                <input
                  type="checkbox"
                  checked={selected.has(table.name)}
                  onchange={() => toggle(table.name)}
                  class="accent-primary w-3.5 h-3.5 shrink-0"
                />
                <span class="truncate flex-1 min-w-0" title={table.name}>{table.name}</span>
                <span class="text-xs text-muted-foreground shrink-0 whitespace-nowrap">
                  {table.columns.length} cols
                </span>
              </label>
            {/each}
            {#if filtered.length === 0}
              <p class="text-xs text-muted-foreground text-center py-4">No tables match</p>
            {/if}
          </div>
        </ScrollArea>
      </div>
    {/if}

    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
      <Button onclick={confirm} disabled={selected.size === 0 || loading}>
        View Diagram
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
