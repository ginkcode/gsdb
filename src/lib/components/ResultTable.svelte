<script lang="ts">
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import type { QueryResult } from "$lib/types";

  let { result }: { result: QueryResult } = $props();

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) {
      return "NULL";
    }
    if (typeof value === "object") {
      return JSON.stringify(value);
    }
    return String(value);
  }

  const MAX_DISPLAY_LENGTH = 100;

  function truncateText(text: string): {
    display: string;
    full: string;
    truncated: boolean;
  } {
    if (text.length <= MAX_DISPLAY_LENGTH) {
      return { display: text, full: text, truncated: false };
    }
    return {
      display: text.slice(0, MAX_DISPLAY_LENGTH) + "…",
      full: text,
      truncated: true,
    };
  }
</script>

{#if result.error}
  <div class="p-4 font-mono text-sm text-destructive whitespace-pre-wrap">
    {result.error}
  </div>
{:else if result.columns.length === 0}
  <div
    class="flex items-center justify-center h-full text-sm text-muted-foreground"
  >
    Query executed successfully. No rows returned.
  </div>
{:else}
  <div class="flex flex-col h-full overflow-hidden">
    <ScrollArea class="flex-1 h-0" orientation="both">
      <table class="text-sm border-collapse font-mono">
        <thead class="sticky top-0 z-10">
          <tr>
            {#each result.columns as col}
              <th
                class="px-3 py-2 text-left font-semibold text-xs text-muted-foreground uppercase tracking-wider bg-muted border-b border-border whitespace-nowrap"
              >
                {col}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each result.rows as row, i}
            <tr
              class="border-b border-border/50 hover:bg-muted/40 transition-colors"
            >
              {#each result.columns as col}
                {@const value = formatValue(row[col])}
                {@const truncated = truncateText(value)}
                <td class="px-3 py-1.5 whitespace-nowrap text-foreground/90">
                  {#if row[col] === null || row[col] === undefined}
                    <span class="text-muted-foreground italic text-xs"
                      >NULL</span
                    >
                  {:else if truncated.truncated}
                    <span title={truncated.full} class="cursor-default"
                      >{truncated.display}</span
                    >
                  {:else}
                    {truncated.display}
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </ScrollArea>
    <div
      class="px-3 py-1.5 text-xs text-muted-foreground border-t border-border shrink-0"
    >
      {result.rows.length}
      {result.rows.length === 1 ? "row" : "rows"}
    </div>
  </div>
{/if}
