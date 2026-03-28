<script lang="ts">
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import {
    Table,
    Download,
    Upload,
    Info,
    Trash2,
    Eraser,
  } from "@lucide/svelte";

  interface Props {
    tableName: string;
    onOpenTable: () => void;
    onShowInfo: () => void;
    onExport: () => void;
    onImport: () => void;
    onDelete: () => void;
    onTruncate: () => void;
    onDrop: () => void;
  }

  let {
    tableName,
    onOpenTable,
    onShowInfo,
    onExport,
    onImport,
    onDelete,
    onTruncate,
    onDrop,
  }: Props = $props();
</script>

{#snippet tableMenuItems(Item: any, Separator: any)}
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onShowInfo}>
    <Info class="w-4 h-4" /><span>Info</span>
  </Item>
  <Separator />
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onExport}>
    <Download class="w-4 h-4" /><span>Export Table</span>
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
  <Item
    class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
    onclick={onTruncate}
  >
    <Eraser class="w-4 h-4" /><span>Truncate</span>
  </Item>
  <Item
    class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
    onclick={onDrop}
  >
    <Trash2 class="w-4 h-4" /><span>Drop Table</span>
  </Item>
{/snippet}

<ContextMenu.Root>
  <ContextMenu.Trigger>
    <div class="group flex items-center gap-1">
      <button
        class="flex-1 flex items-center gap-2 px-2 py-1 rounded text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors text-left"
        onclick={onOpenTable}
      >
        <Table class="w-3 h-3 shrink-0" />
        <span class="truncate">{tableName}</span>
      </button>
      <!-- Three-dot dropdown -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          <button
            class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
            title="Table options"
          >
            <svg
              class="w-3 h-3 text-muted-foreground"
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
        <DropdownMenu.Content class="min-w-36">
          {@render tableMenuItems(DropdownMenu.Item, DropdownMenu.Separator)}
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    {@render tableMenuItems(ContextMenu.Item, ContextMenu.Separator)}
  </ContextMenu.Content>
</ContextMenu.Root>
