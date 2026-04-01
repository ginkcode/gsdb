<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";
  import type { SchemaTable } from "$lib/types";

  let { data }: { data: { table: SchemaTable } } = $props();
  const { table } = data;

  // Constants matching DiagramView.svelte
  const HEADER_H = 34;
  const ROW_H = 24;
</script>

<div
  class="rounded-lg border border-border bg-background shadow-md overflow-hidden min-w-[200px] max-w-[260px] text-xs"
>
  <!-- Header -->
  <div
    class="px-3 py-2 font-semibold bg-primary/10 border-b border-border text-foreground truncate"
  >
    {table.name}
  </div>

  <!-- Columns with individual handles -->
  {#each table.columns as col, i}
    <div
      class="flex items-center gap-1.5 px-3 py-1 border-b border-border/40 last:border-0
             {col.pk ? 'bg-yellow-500/5' : ''}"
    >
      <!-- Left handle for this column (target) -->
      <Handle
        type="target"
        position={Position.Left}
        id="{table.name}-{col.name}-left"
        style="top: {HEADER_H +
          i * ROW_H +
          ROW_H / 2}px; opacity: 0; pointer-events: none;"
      />
      <!-- Right handle for this column (source) -->
      <Handle
        type="source"
        position={Position.Right}
        id="{table.name}-{col.name}-right"
        style="top: {HEADER_H +
          i * ROW_H +
          ROW_H / 2}px; opacity: 0; pointer-events: none;"
      />
      <!-- PK / nullable indicator -->
      {#if col.pk}
        <span class="shrink-0 text-yellow-500" title="Primary key">
          <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
            <path
              d="M12 1a5 5 0 0 1 5 5 5 5 0 0 1-5 5 5 5 0 0 1-5-5 5 5 0 0 1 5-5m0 12c5.5 0 10 2.2 10 5v2H2v-2c0-2.8 4.5-5 10-5z"
            />
          </svg>
        </span>
      {:else if col.nullable}
        <span
          class="shrink-0 text-muted-foreground/60 text-[10px] font-bold leading-none"
          >?</span
        >
      {:else}
        <span class="w-3 shrink-0"></span>
      {/if}
      <span class="truncate text-foreground">{col.name}</span>
      <span class="ml-auto shrink-0 text-[10px] text-orange-400 font-mono"
        >{col.colType}</span
      >
    </div>
  {/each}
</div>
