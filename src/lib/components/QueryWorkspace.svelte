<script lang="ts">
  import { Play, AlignLeft } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import * as ResizablePrimitive from "$lib/components/ui/resizable";
  import SqlEditor from "$lib/components/SqlEditor.svelte";
  import ResultTable from "$lib/components/ResultTable.svelte";
  import {
    queryTabs,
    activeTabId,
    activeTab,
    updateTab,
    activeConnection,
  } from "$lib/stores/connections";
  import type { QueryResult } from "$lib/types";

  let {
    runQuery,
    selectedRow,
    onRowSelect,
  }: {
    runQuery: (tabId: string, sql: string) => void;
    selectedRow: Record<string, unknown> | null;
    onRowSelect: (row: Record<string, unknown>) => void;
  } = $props();

  let editorRun = $state<() => void>();
  let editorFormat = $state<() => void>();

  $effect(() => {
    if ($queryTabs.length > 0 && !$activeTabId) {
      activeTabId.set($queryTabs[0].id);
    }
  });
</script>

{#if $activeTab}
  {@const tab = $activeTab}
  <ResizablePrimitive.PaneGroup direction="vertical" class="flex-1">
    <ResizablePrimitive.Pane defaultSize={40} minSize={20}>
      <div class="h-full flex flex-col bg-muted">
        <SqlEditor
          bind:value={tab.sql}
          bind:runRef={editorRun}
          bind:formatRef={editorFormat}
          dialect={$activeConnection?.driver === "mysql" ? "mysql" : $activeConnection?.driver === "postgres" ? "postgresql" : "sql"}
          onRun={(sql) => runQuery(tab.id, sql)}
        />
        <div
          class="flex items-center gap-2 px-3 py-1.5 border-t border-border/60 bg-background/40 shrink-0"
        >
          <Button
            size="sm"
            variant="outline"
            class="h-7 gap-1.5 text-xs"
            onclick={() => editorFormat?.()}
          >
            <AlignLeft class="w-3 h-3" />
            Format
          </Button>
          <Button
            size="sm"
            class="h-7 gap-1.5 text-xs"
            disabled={tab.isLoading}
            onclick={() => editorRun?.()}
          >
            <Play class="w-3 h-3" />
            {tab.isLoading ? "Running…" : "Run"}
          </Button>
          <span class="text-xs text-muted-foreground">Ctrl+Enter</span>
        </div>
      </div>
    </ResizablePrimitive.Pane>

    <ResizablePrimitive.Handle withHandle />

    <ResizablePrimitive.Pane defaultSize={60} minSize={20}>
      <div class="h-full overflow-hidden bg-background">
        {#if tab.result}
          <ResultTable result={tab.result} {selectedRow} {onRowSelect} />
        {:else if tab.isLoading}
          <div
            class="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground"
          >
            <span class="w-2 h-2 rounded-full bg-primary animate-bounce"></span>
            Executing query…
          </div>
        {:else}
          <div
            class="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground"
          >
            <Play class="w-8 h-8 opacity-20" />
            <p class="text-sm">Run a query to see results</p>
            <p class="text-xs opacity-60">Ctrl+Enter to execute</p>
          </div>
        {/if}
      </div>
    </ResizablePrimitive.Pane>
  </ResizablePrimitive.PaneGroup>
{:else}
  <div
    class="h-full flex flex-col items-center justify-center gap-3 text-muted-foreground"
  >
    <p class="text-sm">No query tab open</p>
    <p class="text-xs opacity-60">
      Select a connection and open a table to start
    </p>
  </div>
{/if}
