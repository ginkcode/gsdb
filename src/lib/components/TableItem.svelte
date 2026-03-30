<script lang="ts">
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import {
    Table,
    Eye,
    Download,
    Upload,
    Info,
    Trash2,
    Eraser,
  } from "@lucide/svelte";

  interface Props {
    tableName: string;
    tableKind: "table" | "view";
    onOpenTable: () => void; // double click → permanent tab
    onPreviewTable: () => void; // single click → temporary tab
    onShowInfo: () => void;
    onExport: () => void;
    onImport: () => void;
    onDelete: () => void;
    onTruncate: () => void;
    onDrop: () => void;
  }

  let {
    tableName,
    tableKind,
    onOpenTable,
    onPreviewTable,
    onShowInfo,
    onExport,
    onImport,
    onDelete,
    onTruncate,
    onDrop,
  }: Props = $props();

  // Distinguish single vs double click without a delay by tracking clicks manually
  let clickTimer: ReturnType<typeof setTimeout> | undefined;

  function handleClick() {
    if (clickTimer) {
      // Second click arrived quickly — treat as double click
      clearTimeout(clickTimer);
      clickTimer = undefined;
      onOpenTable();
    } else {
      clickTimer = setTimeout(() => {
        clickTimer = undefined;
        onPreviewTable();
      }, 250);
    }
  }
</script>

{#snippet tableMenuItems(Item: any, Separator: any)}
  <Item class="flex items-center gap-2 cursor-pointer" onclick={onShowInfo}>
    <Info class="w-4 h-4" /><span>Info</span>
  </Item>
  <Separator />
  {#if tableKind === "table"}
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
      <Trash2 class="w-4 h-4" /><span>Delete All Rows</span>
    </Item>
    <Item
      class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
      onclick={onTruncate}
    >
      <Eraser class="w-4 h-4" /><span>Truncate</span>
    </Item>
  {/if}
  <Item
    class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
    onclick={onDrop}
  >
    <Trash2 class="w-4 h-4" /><span
      >{tableKind === "view" ? "Drop View" : "Drop Table"}</span
    >
  </Item>
{/snippet}

<ContextMenu.Root>
  <ContextMenu.Trigger>
    <div class="group flex items-center gap-1 min-w-0">
      <button
        class="flex-1 min-w-0 flex items-center gap-2 px-2 py-1 rounded text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors text-left"
        onclick={handleClick}
      >
        {#if tableKind === "view"}
          <Eye class="w-3 h-3 shrink-0" />
        {:else}
          <Table class="w-3 h-3 shrink-0" />
        {/if}
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
