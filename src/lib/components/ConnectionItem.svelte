<script lang="ts">
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import {
    Pencil,
    RefreshCw,
    Download,
    Upload,
    Trash2,
    Plus,
    ChevronDown,
    ChevronRight,
    Loader,
  } from "@lucide/svelte";
  import type { Connection } from "$lib/types";
  import TableItem from "./TableItem.svelte";

  const colorClasses: Record<string, string> = {
    blue: "bg-blue-500/15 text-blue-400 border-blue-500/20",
    green: "bg-green-500/15 text-green-400 border-green-500/20",
    orange: "bg-orange-500/15 text-orange-400 border-orange-500/20",
    purple: "bg-purple-500/15 text-purple-400 border-purple-500/20",
    red: "bg-red-500/15 text-red-400 border-red-500/20",
    yellow: "bg-yellow-500/15 text-yellow-400 border-yellow-500/20",
    pink: "bg-pink-500/15 text-pink-400 border-pink-500/20",
    cyan: "bg-cyan-500/15 text-cyan-400 border-cyan-500/20",
  };

  const driverLabel: Record<string, string> = {
    postgres: "PG",
    mysql: "MY",
    sqlite: "SQ",
  };

  interface Props {
    connection: Connection;
    isActive: boolean;
    isExpanded: boolean;
    isLoading: boolean;
    tables: string[];
    isReconnecting: boolean;
    onToggle: () => void;
    onNewQuery: () => void;
    onEdit: () => void;
    onReconnect: () => void;
    onRefreshTables: () => void;
    onRename: () => void;
    onExport: () => void;
    onImport: () => void;
    onDelete: () => void;
    onOpenTable: (tableName: string) => void;
    onShowTableInfo: (tableName: string) => void;
    onExportTable: (tableName: string) => void;
    onImportTable: (tableName: string) => void;
    onDeleteTable: (tableName: string) => void;
    onTruncateTable: (tableName: string) => void;
    onDropTable: (tableName: string) => void;
  }

  let {
    connection,
    isActive,
    isExpanded,
    isLoading,
    tables,
    isReconnecting,
    onToggle,
    onNewQuery,
    onEdit,
    onReconnect,
    onRefreshTables,
    onRename,
    onExport,
    onImport,
    onDelete,
    onOpenTable,
    onShowTableInfo,
    onExportTable,
    onImportTable,
    onDeleteTable,
    onTruncateTable,
    onDropTable,
  }: Props = $props();

  function getLabelColor(conn: Connection): string {
    return colorClasses[conn.color ?? "blue"] ?? colorClasses.blue;
  }
</script>

{#snippet connMenuItems(Item: any, Separator: any)}
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onEdit}>
    <Pencil class="w-4 h-4" /><span>Edit</span>
  </Item>
  <Item
    class="flex items-center gap-2 cursor-pointer"
    onclick={onReconnect}
    disabled={isReconnecting}
  >
    <RefreshCw class="w-4 h-4 {isReconnecting ? 'animate-spin' : ''}" />
    <span>{isReconnecting ? "Reconnecting..." : "Reconnect"}</span>
  </Item>
  <Item
    class="flex items-center gap-2 cursor-pointer"
    onclick={onRefreshTables}
  >
    <svg
      class="w-4 h-4"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
    >
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
    </svg>
    <span>Refresh Tables</span>
  </Item>
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onRename}>
    <svg
      class="w-4 h-4"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
    >
      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
    </svg>
    <span>Rename</span>
  </Item>
  <Separator />
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onExport}>
    <Download class="w-4 h-4" /><span>Export Database</span>
  </Item>
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onImport}>
    <Upload class="w-4 h-4" /><span>Import SQL</span>
  </Item>
  <Separator />
  <Item
    class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
    onclick={onDelete}
  >
    <Trash2 class="w-4 h-4" /><span>Delete</span>
  </Item>
{/snippet}

<ContextMenu.Root>
  <ContextMenu.Trigger>
    <div
      class="flex items-center gap-1 px-1 py-1 rounded-md group
            {isActive ? 'bg-accent' : 'hover:bg-accent/40'}"
    >
      <!-- Expand/collapse toggle -->
      <button
        class="flex items-center gap-1.5 flex-1 min-w-0 text-left"
        onclick={onToggle}
      >
        {#if isLoading}
          <Loader
            class="w-3.5 h-3.5 shrink-0 text-muted-foreground animate-spin"
          />
        {:else if isExpanded}
          <ChevronDown class="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
        {:else}
          <ChevronRight class="w-3.5 h-3.5 shrink-0 text-muted-foreground" />
        {/if}
        <span
          class="shrink-0 text-[10px] font-bold px-1.5 py-0.5 rounded border {getLabelColor(connection)}"
        >
          {driverLabel[connection.driver]}
        </span>
        <span
          class="truncate text-sm
                {isActive
            ? 'text-accent-foreground'
            : 'text-muted-foreground group-hover:text-foreground'}"
        >
          {connection.name}
        </span>
      </button>

      <!-- New query tab button -->
      <button
        class="shrink-0 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
        title="New query tab"
        onclick={onNewQuery}
      >
        <Plus class="w-3.5 h-3.5 text-muted-foreground" />
      </button>

      <!-- Three-dot dropdown -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          <button
            class="shrink-0 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
            title="More options"
          >
            <svg
              class="w-3.5 h-3.5 text-muted-foreground"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="5" r="1" />
              <circle cx="12" cy="12" r="1" />
              <circle cx="12" cy="19" r="1" />
            </svg>
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Content class="min-w-44">
          {@render connMenuItems(DropdownMenu.Item, DropdownMenu.Separator)}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    {@render connMenuItems(ContextMenu.Item, ContextMenu.Separator)}
  </ContextMenu.Content>
</ContextMenu.Root>

<!-- Table list -->
{#if isExpanded}
  <div class="ml-4 mb-1">
    {#if tables.length === 0 && !isLoading}
      <p class="px-2 py-1 text-xs text-muted-foreground italic">
        No tables found
      </p>
    {/if}
    {#each tables as table}
      <TableItem
        tableName={table}
        onOpenTable={() => onOpenTable(table)}
        onShowInfo={() => onShowTableInfo(table)}
        onExport={() => onExportTable(table)}
        onImport={() => onImportTable(table)}
        onDelete={() => onDeleteTable(table)}
        onTruncate={() => onTruncateTable(table)}
        onDrop={() => onDropTable(table)}
      />
    {/each}
  </div>
{/if}
