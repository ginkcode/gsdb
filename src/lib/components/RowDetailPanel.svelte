<script lang="ts">
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { X, Copy, Check } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";

  let {
    row,
    columns,
    onClose,
  }: {
    row: Record<string, unknown> | null;
    columns: string[];
    onClose: () => void;
  } = $props();

  let copiedColumn = $state<string | null>(null);

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) {
      return "NULL";
    }
    if (typeof value === "object") {
      return JSON.stringify(value, null, 2);
    }
    return String(value);
  }

  function getValueType(value: unknown): string {
    if (value === null || value === undefined) return "null";
    if (typeof value === "boolean") return "boolean";
    if (typeof value === "number") return "number";
    if (typeof value === "string") return "string";
    if (typeof value === "object") return "object";
    return "unknown";
  }

  async function copyToClipboard(text: string, column: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedColumn = column;
      setTimeout(() => {
        copiedColumn = null;
      }, 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  }
</script>

<div
  class="h-full flex flex-col overflow-hidden border-l border-border bg-background"
>
  <div
    class="flex items-center justify-between px-4 py-3 border-b border-border shrink-0"
  >
    <h3 class="text-sm font-semibold">Row Details</h3>
    <Button variant="ghost" size="icon" class="h-7 w-7" onclick={onClose}>
      <X class="w-4 h-4" />
    </Button>
  </div>

  {#if row}
    <ScrollArea class="flex-1 h-0">
      <div class="p-4 space-y-4">
        {#each columns as col}
          {@const value = row[col]}
          {@const formattedValue = formatValue(value)}
          {@const valueType = getValueType(value)}

          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <span
                class="text-xs font-medium text-muted-foreground uppercase tracking-wider"
              >
                {col}
              </span>
              <div class="flex items-center gap-1">
                <span
                  class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground"
                >
                  {valueType}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6"
                  onclick={() => copyToClipboard(formattedValue, col)}
                  title="Copy value"
                >
                  {#if copiedColumn === col}
                    <Check class="w-3 h-3 text-green-500" />
                  {:else}
                    <Copy class="w-3 h-3" />
                  {/if}
                </Button>
              </div>
            </div>
            <div
              class="text-sm font-mono bg-muted/50 rounded px-3 py-2 break-all whitespace-pre-wrap max-h-48 overflow-y-auto"
            >
              {#if value === null || value === undefined}
                <span class="text-muted-foreground italic">NULL</span>
              {:else}
                {formattedValue}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </ScrollArea>
  {:else}
    <div
      class="flex-1 flex items-center justify-center text-sm text-muted-foreground"
    >
      <p>Select a row to view details</p>
    </div>
  {/if}
</div>
