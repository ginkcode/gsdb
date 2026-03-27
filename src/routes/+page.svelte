<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Plus, Play, X, Database, ChevronRight } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import * as ResizablePrimitive from "$lib/components/ui/resizable";
  import SqlEditor from "$lib/components/SqlEditor.svelte";
  import ResultTable from "$lib/components/ResultTable.svelte";
  import ConnectionForm from "$lib/components/ConnectionForm.svelte";
  import {
    connections,
    activeConnectionId,
    queryTabs,
    activeTabId,
    activeTab,
    addTab,
    closeTab,
    updateTab,
  } from "$lib/stores/connections";
  import type { Connection, QueryResult } from "$lib/types";

  let showConnectionForm = $state(false);
  let editorRun = $state<() => void>();

  const driverColors: Record<string, string> = {
    postgres: "bg-blue-500/15 text-blue-400 border-blue-500/20",
    mysql:    "bg-orange-500/15 text-orange-400 border-orange-500/20",
    sqlite:   "bg-green-500/15 text-green-400 border-green-500/20",
  };

  const driverLabel: Record<string, string> = {
    postgres: "PG",
    mysql:    "MY",
    sqlite:   "SQ",
  };

  async function saveConnection(conn: Connection) {
    try {
      await invoke("add_connection", { connection: conn });
      connections.update((c) => [...c, conn]);
      activeConnectionId.set(conn.id);
      addTab(conn.id);
    } catch (err) {
      console.error("Connection failed:", err);
    }
  }

  async function runQuery(tabId: string, sql: string) {
    const tab = $queryTabs.find((t) => t.id === tabId);
    if (!tab) return;
    updateTab(tabId, { isLoading: true, result: undefined });
    try {
      const result: QueryResult = await invoke("run_query", {
        connectionId: tab.connectionId,
        sql,
      });
      updateTab(tabId, { result, isLoading: false });
    } catch (err) {
      updateTab(tabId, {
        result: { columns: [], rows: [], error: String(err) },
        isLoading: false,
      });
    }
  }
</script>

<!-- Force dark mode -->
<div class="dark flex h-screen bg-background text-foreground overflow-hidden font-sans antialiased">

  <!-- Sidebar -->
  <aside class="w-56 shrink-0 flex flex-col border-r border-border bg-background">
    <div class="flex items-center justify-between px-4 py-3 border-b border-border">
      <div class="flex items-center gap-2">
        <Database class="w-4 h-4 text-primary" />
        <span class="text-sm font-semibold tracking-tight">gs-data</span>
      </div>
      <Button
        variant="ghost"
        size="icon"
        class="h-7 w-7"
        onclick={() => (showConnectionForm = true)}
        title="New connection"
      >
        <Plus class="w-4 h-4" />
      </Button>
    </div>

    <div class="px-2 py-2 flex-1 overflow-y-auto">
      <p class="px-2 mb-1 text-xs font-medium text-muted-foreground uppercase tracking-wider">Connections</p>
      {#each $connections as conn}
        <button
          class="w-full flex items-center gap-2.5 px-2 py-2 rounded-md text-sm transition-colors text-left
            {$activeConnectionId === conn.id
              ? 'bg-accent text-accent-foreground'
              : 'text-muted-foreground hover:bg-accent/60 hover:text-foreground'}"
          onclick={() => { activeConnectionId.set(conn.id); addTab(conn.id); }}
        >
          <span class="shrink-0 text-[10px] font-bold px-1.5 py-0.5 rounded border {driverColors[conn.driver]}">
            {driverLabel[conn.driver]}
          </span>
          <span class="truncate">{conn.name}</span>
          {#if $activeConnectionId === conn.id}
            <ChevronRight class="ml-auto w-3.5 h-3.5 shrink-0 text-muted-foreground" />
          {/if}
        </button>
      {/each}

      {#if $connections.length === 0}
        <button
          class="w-full mt-1 flex items-center gap-2 px-2 py-2 rounded-md text-sm text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors border border-dashed border-border"
          onclick={() => (showConnectionForm = true)}
        >
          <Plus class="w-3.5 h-3.5" /> Add connection
        </button>
      {/if}
    </div>
  </aside>

  <!-- Main area -->
  <div class="flex-1 flex flex-col overflow-hidden">

    <!-- Tab bar -->
    <div class="flex items-stretch border-b border-border bg-background shrink-0 overflow-x-auto">
      {#each $queryTabs as tab}
        {@const conn = $connections.find(c => c.id === tab.connectionId)}
        <div
          role="tab"
          aria-selected={$activeTabId === tab.id}
          tabindex="0"
          class="group flex items-center gap-2 px-4 py-2.5 text-sm border-r border-border transition-colors shrink-0 cursor-pointer select-none
            {$activeTabId === tab.id
              ? 'bg-muted text-foreground border-b-2 border-b-primary -mb-px'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
          onclick={() => activeTabId.set(tab.id)}
          onkeydown={(e) => e.key === 'Enter' && activeTabId.set(tab.id)}
        >
          {#if conn}
            <span class="text-[9px] font-bold px-1 py-0.5 rounded border {driverColors[conn.driver]}">
              {driverLabel[conn.driver]}
            </span>
          {/if}
          <span>{tab.title}</span>
          {#if tab.isLoading}
            <span class="w-1.5 h-1.5 rounded-full bg-primary animate-pulse"></span>
          {/if}
          <button
            class="ml-1 rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
            onclick={(e) => { e.stopPropagation(); closeTab(tab.id); }}
          >
            <X class="w-3 h-3" />
          </button>
        </div>
      {/each}
    </div>

    <!-- Workspace -->
    {#if $activeTab}
      {@const tab = $activeTab}
      <ResizablePrimitive.PaneGroup direction="vertical" class="flex-1">
        <ResizablePrimitive.Pane defaultSize={40} minSize={20}>
          <div class="h-full flex flex-col bg-[#1e1e2e]">
            <SqlEditor
              bind:value={tab.sql}
              bind:runRef={editorRun}
              onRun={(sql) => runQuery(tab.id, sql)}
            />
            <div class="flex items-center gap-2 px-3 py-1.5 border-t border-border/60 bg-background/40 shrink-0">
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
              <ResultTable result={tab.result} />
            {:else if tab.isLoading}
              <div class="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground">
                <span class="w-2 h-2 rounded-full bg-primary animate-bounce"></span>
                Executing query…
              </div>
            {:else}
              <div class="flex flex-col items-center justify-center h-full gap-2 text-muted-foreground">
                <Play class="w-8 h-8 opacity-20" />
                <p class="text-sm">Run a query to see results</p>
                <p class="text-xs opacity-60">Ctrl+Enter to execute</p>
              </div>
            {/if}
          </div>
        </ResizablePrimitive.Pane>
      </ResizablePrimitive.PaneGroup>
    {:else}
      <div class="flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground">
        <Database class="w-12 h-12 opacity-20" />
        <p class="text-sm">No query tab open</p>
        <Button variant="outline" size="sm" onclick={() => (showConnectionForm = true)}>
          <Plus class="w-4 h-4 mr-2" /> Add a connection
        </Button>
      </div>
    {/if}
  </div>
</div>

<ConnectionForm bind:open={showConnectionForm} onSave={saveConnection} />
