<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { Copy, Check, X, CheckCircle, XCircle } from "@lucide/svelte";
  import type { QueryHistoryEntry } from "$lib/types";
  import { toast } from "svelte-sonner";

  let {
    open,
    connectionName,
    history,
    onSelect,
    onClose,
  }: {
    open: boolean;
    connectionName: string;
    history: QueryHistoryEntry[];
    onSelect?: (sql: string) => void;
    onClose: () => void;
  } = $props();

  let copiedIndex = $state<number | null>(null);

  async function copyToClipboard(sql: string, index: number) {
    try {
      await navigator.clipboard.writeText(sql);
      copiedIndex = index;
      setTimeout(() => (copiedIndex = null), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  }

  function formatTimestamp(iso: string): string {
    const date = new Date(iso);
    return date.toLocaleString();
  }

  function truncateSql(sql: string, maxLen = 100): string {
    if (sql.length <= maxLen) return sql;
    return sql.substring(0, maxLen) + "...";
  }
</script>

<Dialog.Root {open} onOpenChange={(v) => !v && onClose()}>
  <Dialog.Content
    class="sm:max-w-2xl max-h-[80vh] flex flex-col overflow-hidden"
  >
    <Dialog.Header class="shrink-0">
      <Dialog.Title>Query History - {connectionName}</Dialog.Title>
      <Dialog.Description>
        Last {history.length} executed queries
      </Dialog.Description>
    </Dialog.Header>

    <ScrollArea class="flex-1 min-h-0 mt-4 overflow-auto">
      {#if history.length === 0}
        <div
          class="flex items-center justify-center h-32 text-muted-foreground"
        >
          No queries executed yet
        </div>
      {:else}
        <div class="space-y-2 pr-4">
          {#each [...history].reverse() as entry, i}
            {@const actualIndex = history.length - 1 - i}
            <div
              class="p-3 rounded-lg border bg-muted/30 hover:bg-muted/50 transition-colors"
            >
              <div class="flex items-start justify-between gap-2">
                <div class="flex-1 min-w-0 overflow-hidden">
                  <div class="flex items-center gap-2 mb-1 flex-wrap">
                    {#if entry.success}
                      <CheckCircle class="w-4 h-4 text-green-500 shrink-0" />
                    {:else}
                      <XCircle class="w-4 h-4 text-red-500 shrink-0" />
                    {/if}
                    <span class="text-xs text-muted-foreground">
                      {formatTimestamp(entry.timestamp)}
                    </span>
                    {#if entry.rowsAffected !== undefined}
                      <span class="text-xs text-muted-foreground">
                        ({entry.rowsAffected} rows)
                      </span>
                    {/if}
                  </div>
                  <pre
                    class="text-sm font-mono whitespace-pre-wrap break-all bg-background/50 p-2 rounded overflow-auto max-h-40">{entry.sql}</pre>
                  {#if entry.error}
                    <p class="text-xs text-red-500 mt-1 break-all">
                      {entry.error}
                    </p>
                  {/if}
                </div>
                <div class="flex items-center gap-1 shrink-0">
                  {#if onSelect}
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7"
                      onclick={() => onSelect(entry.sql)}
                      title="Use this query"
                    >
                      <svg
                        class="w-4 h-4"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                      >
                        <path d="M5 12h14M12 5l7 7-7 7" />
                      </svg>
                    </Button>
                  {/if}
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-7 w-7"
                    onclick={() => copyToClipboard(entry.sql, actualIndex)}
                    title="Copy to clipboard"
                  >
                    {#if copiedIndex === actualIndex}
                      <Check class="w-4 h-4 text-green-500" />
                    {:else}
                      <Copy class="w-4 h-4" />
                    {/if}
                  </Button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </ScrollArea>

    <Dialog.Footer>
      <Button variant="outline" onclick={onClose}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
