<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import { Search } from "@lucide/svelte";
  import type { TableInfo, SchemaForeignKey, TableExportOptions } from "$lib/types";

  interface Props {
    open: boolean;
    connectionId: string;
    connectionName: string;
    onConfirm: (tables: TableExportOptions[]) => void;
    onCancel: () => void;
  }

  let { open, connectionId, connectionName, onConfirm, onCancel }: Props =
    $props();

  let tables = $state<TableInfo[]>([]);
  let loading = $state(true);
  let search = $state("");
  let selectAll = $state(true);
  let selectAllStructure = $state(true);
  let selectAllData = $state(true);

  // Per-table state: { selected: boolean, structure: boolean, data: boolean }
  let tableState = $state<
    Record<string, { selected: boolean; structure: boolean; data: boolean }>
  >({});

  // Topological sort using Kahn's algorithm
  // Tables are ordered so FK targets come before tables that reference them
  function topologicalSort(
    tableNames: string[],
    fkEdges: [string, string][],
  ): string[] {
    const tableSet = new Set(tableNames);

    // Build in-degree map and adjacency list
    const inDegree = new Map<string, number>();
    const edges = new Map<string, string[]>(); // prerequisite -> dependents

    for (const name of tableNames) {
      inDegree.set(name, 0);
    }

    for (const [fromTable, toTable] of fkEdges) {
      // fromTable has FK pointing to toTable, so toTable must come first
      if (
        fromTable === toTable ||
        !tableSet.has(fromTable) ||
        !tableSet.has(toTable)
      ) {
        continue;
      }

      if (!edges.has(toTable)) {
        edges.set(toTable, []);
      }
      edges.get(toTable)!.push(fromTable);
      inDegree.set(fromTable, (inDegree.get(fromTable) ?? 0) + 1);
    }

    // Start with tables that have no dependencies
    const queue: string[] = [];
    for (const [name, degree] of inDegree) {
      if (degree === 0) {
        queue.push(name);
      }
    }
    queue.sort(); // Deterministic order

    const sorted: string[] = [];
    while (queue.length > 0) {
      const current = queue.shift()!;
      sorted.push(current);

      const dependents = edges.get(current) ?? [];
      dependents.sort(); // Deterministic order
      for (const dep of dependents) {
        const newDegree = (inDegree.get(dep) ?? 0) - 1;
        inDegree.set(dep, newDegree);
        if (newDegree === 0) {
          queue.push(dep);
        }
      }
    }

    // Append any tables not reached (cycles) in original order
    const sortedSet = new Set(sorted);
    for (const name of tableNames) {
      if (!sortedSet.has(name)) {
        sorted.push(name);
      }
    }

    return sorted;
  }

  async function loadTables() {
    loading = true;
    try {
      const [tableList, schema] = await Promise.all([
        invoke<TableInfo[]>("list_tables", { connectionId }),
        invoke<{ foreignKeys: SchemaForeignKey[] }>("get_schema", {
          connectionId,
        }),
      ]);

      // Filter to only tables (not views) for export
      const tableNames = tableList
        .filter((t) => t.kind === "table")
        .map((t) => t.name);

      // Build FK edges for topological sort
      const fkEdges: [string, string][] = schema.foreignKeys.map((fk) => [
        fk.fromTable,
        fk.toTable,
      ]);

      // Sort tables by FK dependencies
      const sortedNames = topologicalSort(tableNames, fkEdges);

      // Create sorted table list
      const sortedTables: TableInfo[] = sortedNames
        .map((name) => tableList.find((t) => t.name === name))
        .filter((t): t is TableInfo => t !== undefined);

      tables = sortedTables;

      // Initialize state: all selected by default
      tableState = {};
      for (const t of tables) {
        tableState[t.name] = { selected: true, structure: true, data: true };
      }
      updateSelectAll();
    } catch (err) {
      toast.error(`Failed to load tables: ${err}`);
    } finally {
      loading = false;
    }
  }

  let filteredTables = $derived(
    tables.filter((t) => t.name.toLowerCase().includes(search.toLowerCase())),
  );

  function updateSelectAll() {
    const entries = Object.values(tableState);
    selectAll = entries.length > 0 && entries.every((e) => e.selected);
    selectAllStructure = entries
      .filter((e) => e.selected)
      .every((e) => e.structure);
    selectAllData = entries.filter((e) => e.selected).every((e) => e.data);
  }

  function toggleSelectAll(checked: boolean) {
    for (const name in tableState) {
      tableState[name] = { ...tableState[name], selected: checked };
    }
    selectAll = checked;
    updateSelectAll();
  }

  function toggleSelectAllStructure(checked: boolean) {
    for (const name in tableState) {
      if (tableState[name].selected) {
        tableState[name] = { ...tableState[name], structure: checked };
      }
    }
    selectAllStructure = checked;
  }

  function toggleSelectAllData(checked: boolean) {
    for (const name in tableState) {
      if (tableState[name].selected) {
        tableState[name] = { ...tableState[name], data: checked };
      }
    }
    selectAllData = checked;
  }

  function toggleTable(name: string, checked: boolean) {
    tableState[name] = { ...tableState[name], selected: checked };
    updateSelectAll();
  }

  function toggleTableStructure(name: string, checked: boolean) {
    tableState[name] = { ...tableState[name], structure: checked };
    updateSelectAll();
  }

  function toggleTableData(name: string, checked: boolean) {
    tableState[name] = { ...tableState[name], data: checked };
    updateSelectAll();
  }

  function handleConfirm() {
    const selectedTables: TableExportOptions[] = tables
      .filter((t) => tableState[t.name]?.selected)
      .map((t) => ({
        name: t.name,
        includeStructure: tableState[t.name]?.structure ?? true,
        includeData: tableState[t.name]?.data ?? true,
      }));

    onConfirm(selectedTables);
  }

  // Load tables when dialog opens
  $effect(() => {
    if (open) {
      search = "";
      loadTables();
    }
  });
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>Export Database</Dialog.Title>
      <Dialog.Description>
        Select tables to export from <strong>{connectionName}</strong>
      </Dialog.Description>
    </Dialog.Header>

    <div class="space-y-4">
      {#if loading}
        <div
          class="flex items-center justify-center py-8 text-muted-foreground"
        >
          <span class="w-2 h-2 rounded-full bg-primary animate-bounce"></span>
          <span class="ml-2">Loading tables...</span>
        </div>
      {:else if tables.length === 0}
        <div
          class="flex items-center justify-center py-8 text-muted-foreground"
        >
          No tables found
        </div>
      {:else}
        <!-- Search -->
        <div class="relative">
          <Search
            class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground"
          />
          <Input
            bind:value={search}
            placeholder="Search tables..."
            class="pl-9"
          />
        </div>

        <!-- Table list -->
        <ScrollArea class="h-80 border rounded-md">
          <table class="w-full text-sm">
            <thead class="bg-muted/50 sticky top-0 z-10">
              <tr>
                <th class="w-10 px-3 py-2.5 text-left">
                  <Checkbox
                    checked={selectAll}
                    onCheckedChange={(v) => toggleSelectAll(!!v)}
                    aria-label="Select all tables"
                  />
                </th>
                <th
                  class="px-3 py-2.5 text-left font-medium text-muted-foreground"
                >
                  Table ({Object.values(tableState).filter((s) => s.selected)
                    .length}/{tables.length})
                </th>
                <th
                  class="w-24 px-3 py-2.5 text-center font-medium text-muted-foreground"
                >
                  <label
                    class="flex items-center justify-center gap-1.5 cursor-pointer"
                  >
                    <Checkbox
                      checked={selectAllStructure}
                      onCheckedChange={(v) => toggleSelectAllStructure(!!v)}
                      aria-label="Select all structure"
                    />
                    <span class="text-xs">Structure</span>
                  </label>
                </th>
                <th
                  class="w-20 px-3 py-2.5 text-center font-medium text-muted-foreground"
                >
                  <label
                    class="flex items-center justify-center gap-1.5 cursor-pointer"
                  >
                    <Checkbox
                      checked={selectAllData}
                      onCheckedChange={(v) => toggleSelectAllData(!!v)}
                      aria-label="Select all data"
                    />
                    <span class="text-xs">Data</span>
                  </label>
                </th>
              </tr>
            </thead>
            <tbody class="divide-y">
              {#each filteredTables as table (table.name)}
                <tr class="hover:bg-muted/30 transition-colors">
                  <td class="px-3 py-2">
                    <Checkbox
                      checked={tableState[table.name]?.selected ?? false}
                      onCheckedChange={(v) => toggleTable(table.name, !!v)}
                      aria-label="Select {table.name}"
                    />
                  </td>
                  <td class="px-3 py-2 max-w-0 w-full">
                    <span class="font-mono text-xs block truncate" title={table.name}>{table.name}</span>
                  </td>
                  <td class="px-3 py-2 text-center">
                    <Checkbox
                      checked={tableState[table.name]?.structure ?? false}
                      disabled={!tableState[table.name]?.selected}
                      onCheckedChange={(v) =>
                        toggleTableStructure(table.name, !!v)}
                      aria-label="Include structure for {table.name}"
                    />
                  </td>
                  <td class="px-3 py-2 text-center">
                    <Checkbox
                      checked={tableState[table.name]?.data ?? false}
                      disabled={!tableState[table.name]?.selected}
                      onCheckedChange={(v) => toggleTableData(table.name, !!v)}
                      aria-label="Include data for {table.name}"
                    />
                  </td>
                </tr>
              {/each}
              {#if filteredTables.length === 0 && tables.length > 0}
                <tr>
                  <td
                    colspan="4"
                    class="px-3 py-8 text-center text-muted-foreground text-sm"
                  >
                    No tables match your search
                  </td>
                </tr>
              {/if}
            </tbody>
          </table>
        </ScrollArea>

        <p class="text-xs text-muted-foreground">
          Tables are ordered by foreign key dependencies. Structure is exported
          before data.
        </p>
      {/if}
    </div>

    <Dialog.Footer>
      <Button variant="outline" onclick={onCancel}>Cancel</Button>
      <Button
        onclick={handleConfirm}
        disabled={loading ||
          tables.length === 0 ||
          !Object.values(tableState).some((s) => s.selected)}
      >
        Export {Object.values(tableState).filter((s) => s.selected).length} tables
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
