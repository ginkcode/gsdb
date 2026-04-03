<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeFile } from "@tauri-apps/plugin-fs";
  import { downloadDir } from "@tauri-apps/api/path";
  import { Download } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import type { QueryResult } from "$lib/types";

  let {
    result,
    selectedRow,
    onRowSelect,
  }: {
    result: QueryResult;
    selectedRow: Record<string, unknown> | null;
    onRowSelect: (row: Record<string, unknown>) => void;
  } = $props();

  // Virtual scroll constants
  const ROW_HEIGHT = 33; // px — matches py-1.5 + text-sm line-height
  const BUFFER = 15;     // extra rows rendered above/below the viewport

  let containerEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let containerHeight = $state(400);

  $effect(() => {
    if (!containerEl) return;
    const ro = new ResizeObserver((entries) => {
      containerHeight = entries[0].contentRect.height;
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  // Reset scroll position when result changes
  $effect(() => {
    result; // track
    scrollTop = 0;
    if (containerEl) containerEl.scrollTop = 0;
  });

  const startIndex = $derived(
    Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER)
  );
  const endIndex = $derived(
    Math.min(
      result.rows.length,
      startIndex + Math.ceil(containerHeight / ROW_HEIGHT) + BUFFER * 2
    )
  );
  const visibleRows = $derived(result.rows.slice(startIndex, endIndex));
  const topPadding = $derived(startIndex * ROW_HEIGHT);
  const bottomPadding = $derived((result.rows.length - endIndex) * ROW_HEIGHT);

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "object") return JSON.stringify(value);
    return String(value);
  }

  const MAX_DISPLAY_LENGTH = 100;

  function truncateText(text: string): { display: string; full: string; truncated: boolean } {
    if (text.length <= MAX_DISPLAY_LENGTH)
      return { display: text, full: text, truncated: false };
    return { display: text.slice(0, MAX_DISPLAY_LENGTH) + "…", full: text, truncated: true };
  }

  function isRowSelected(row: Record<string, unknown>): boolean {
    if (!selectedRow) return false;
    return result.columns.every((col) => row[col] === selectedRow[col]);
  }

  function escapeCSVField(value: string): string {
    if (value.includes(",") || value.includes('"') || value.includes("\n")) {
      return `"${value.replace(/"/g, '""')}"`;
    }
    return value;
  }

  async function exportToCSV() {
    const headers = result.columns.map(escapeCSVField).join(",");
    const rows = result.rows.map((row) =>
      result.columns.map((col) => escapeCSVField(formatValue(row[col]))).join(",")
    );
    const csv = [headers, ...rows].join("\n");
    const encoder = new TextEncoder();
    const csvBytes = encoder.encode(csv);
    const defaultName = `query-results-${new Date().toISOString().slice(0, 10)}.csv`;
    const downloadsPath = await downloadDir();
    try {
      const filePath = await save({
        defaultPath: downloadsPath ? `${downloadsPath}/${defaultName}` : defaultName,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (filePath) await writeFile(filePath, csvBytes);
    } catch (err) {
      console.error("Failed to save CSV:", err);
    }
  }
</script>

{#if result.error}
  <div class="p-4 font-mono text-sm text-destructive whitespace-pre-wrap">
    {result.error}
  </div>
{:else if result.columns.length === 0}
  <div class="flex items-center justify-center h-full text-sm text-muted-foreground">
    Query executed successfully. No rows returned.
  </div>
{:else}
  <div class="flex flex-col h-full overflow-hidden">
    <!-- Scrollable area — native overflow so we can track scrollTop -->
    <div
      bind:this={containerEl}
      class="flex-1 h-0 overflow-auto"
      onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    >
      <table class="text-sm font-mono border-collapse">
        <thead class="sticky top-0 z-10">
          <tr>
            <th
              class="px-2 py-2 text-right text-xs text-muted-foreground/50 bg-muted border-b border-r border-border select-none w-10 min-w-10"
            >#</th>
            {#each result.columns as col}
              <th
                class="px-3 py-2 text-left text-xs font-semibold text-muted-foreground tracking-wide bg-muted border-b border-r border-border whitespace-nowrap max-w-48"
              >
                <div class="truncate" title={col}>{col}</div>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          <!-- Top spacer — represents rows above the viewport -->
          {#if topPadding > 0}
            <tr style="height: {topPadding}px;"><td></td></tr>
          {/if}

          {#each visibleRows as row, localIdx}
            {@const i = startIndex + localIdx}
            <tr
              style="height: {ROW_HEIGHT}px;"
              class="border-b border-border/40 transition-colors cursor-pointer
                {isRowSelected(row)
                  ? 'bg-accent'
                  : i % 2 === 0
                    ? 'bg-background hover:bg-muted/50'
                    : 'bg-muted/20 hover:bg-muted/50'}"
              onclick={() => onRowSelect(row)}
            >
              <td
                class="px-2 py-1.5 text-right text-xs text-muted-foreground/40 border-r border-border/40 select-none tabular-nums"
              >{i + 1}</td>
              {#each result.columns as col}
                {@const value = formatValue(row[col])}
                {@const truncated = truncateText(value)}
                <td class="px-3 py-1.5 border-r border-border/40 max-w-xs">
                  {#if row[col] === null || row[col] === undefined}
                    <span class="text-muted-foreground/60 italic text-xs">NULL</span>
                  {:else if truncated.truncated}
                    <span title={truncated.full} class="block truncate text-foreground/90"
                      >{truncated.display}</span>
                  {:else}
                    <span class="block truncate text-foreground/90">{truncated.display}</span>
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}

          <!-- Bottom spacer — represents rows below the viewport -->
          {#if bottomPadding > 0}
            <tr style="height: {bottomPadding}px;"><td></td></tr>
          {/if}
        </tbody>
      </table>
    </div>

    <div
      class="flex items-center justify-between px-3 h-8 text-xs text-muted-foreground border-t border-border shrink-0"
    >
      <span>{result.rows.length} {result.rows.length === 1 ? "row" : "rows"}</span>
      <Button variant="ghost" size="sm" class="h-6 gap-1.5 text-xs" onclick={exportToCSV}>
        <Download class="w-3 h-3" />
        Export CSV
      </Button>
    </div>
  </div>
{/if}
