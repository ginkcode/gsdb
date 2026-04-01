<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";
  import type { SchemaTable } from "$lib/types";

  let { data }: { data: { table: SchemaTable } } = $props();
  const { table } = data;
</script>

<!-- Left (target) handle — invisible, centred on the node -->
<Handle type="target" position={Position.Left} style="opacity:0; pointer-events:none" />

<div
  class="rounded-lg border border-border bg-background shadow-md overflow-hidden min-w-[200px] max-w-[260px] text-xs"
>
  <!-- Header -->
  <div class="px-3 py-2 font-semibold bg-primary/10 border-b border-border text-foreground truncate">
    {table.name}
  </div>

  <!-- Columns -->
  {#each table.columns as col}
    <div
      class="flex items-center gap-1.5 px-3 py-1 border-b border-border/40 last:border-0
             {col.pk ? 'bg-yellow-500/5' : ''}"
    >
      <!-- PK / nullable indicator -->
      {#if col.pk}
        <span class="shrink-0 text-yellow-500" title="Primary key">
          <svg class="w-3 h-3" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 1a5 5 0 0 1 5 5 5 5 0 0 1-5 5 5 5 0 0 1-5-5 5 5 0 0 1 5-5m0 12c5.5 0 10 2.2 10 5v2H2v-2c0-2.8 4.5-5 10-5z"/>
          </svg>
        </span>
      {:else if col.nullable}
        <span class="shrink-0 text-muted-foreground/60 text-[10px] font-bold leading-none">?</span>
      {:else}
        <span class="w-3 shrink-0"></span>
      {/if}
      <span class="truncate text-foreground">{col.name}</span>
      <span class="ml-auto shrink-0 text-[10px] text-orange-400 font-mono">{col.colType}</span>
    </div>
  {/each}
</div>

<!-- Right (source) handle — invisible, centred on the node -->
<Handle type="source" position={Position.Right} style="opacity:0; pointer-events:none" />
