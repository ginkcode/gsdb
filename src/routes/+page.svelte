<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    Plus,
    Play,
    X,
    Database,
    ChevronRight,
    ChevronDown,
    Table,
    Loader,
  } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import * as ResizablePrimitive from "$lib/components/ui/resizable";
  import SqlEditor from "$lib/components/SqlEditor.svelte";
  import ResultTable from "$lib/components/ResultTable.svelte";
  import ConnectionForm from "$lib/components/ConnectionForm.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
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

  // Sidebar tree state
  let expandedConnections = $state<Set<string>>(new Set());
  let connectionTables = $state<Record<string, string[]>>({});
  let loadingTables = $state<Set<string>>(new Set());

  async function toggleConnection(connId: string) {
    if (expandedConnections.has(connId)) {
      expandedConnections = new Set(
        [...expandedConnections].filter((id) => id !== connId),
      );
    } else {
      expandedConnections = new Set([...expandedConnections, connId]);
      if (!connectionTables[connId]) {
        loadingTables = new Set([...loadingTables, connId]);
        try {
          const tables: string[] = await invoke("list_tables", {
            connectionId: connId,
          });
          connectionTables = { ...connectionTables, [connId]: tables };
        } catch (err) {
          connectionTables = { ...connectionTables, [connId]: [] };
          console.error("Failed to list tables:", err);
        } finally {
          loadingTables = new Set(
            [...loadingTables].filter((id) => id !== connId),
          );
        }
      }
    }
  }

  const driverColors: Record<string, string> = {
    postgres: "bg-blue-500/15 text-blue-400 border-blue-500/20",
    mysql: "bg-orange-500/15 text-orange-400 border-orange-500/20",
    sqlite: "bg-green-500/15 text-green-400 border-green-500/20",
  };

  const driverLabel: Record<string, string> = {
    postgres: "PG",
    mysql: "MY",
    sqlite: "SQ",
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

<!-- Theme-aware root -->
<div
  class="h-screen bg-background text-foreground overflow-hidden font-sans antialiased"
>
  <ResizablePrimitive.PaneGroup direction="horizontal" class="h-full">
    <!-- Sidebar -->
    <ResizablePrimitive.Pane defaultSize={16} minSize={10} maxSize={35}>
      <aside class="h-full flex flex-col border-r border-border bg-background">
        <div
          class="flex items-center justify-between px-4 py-3 border-b border-border"
        >
          <div class="flex items-center gap-2">
            <Database class="w-4 h-4 text-primary" />
            <span class="text-sm font-semibold tracking-tight">gs-data</span>
          </div>
          <div class="flex items-center gap-1">
            <ThemeToggle />
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
        </div>

        <div class="px-2 py-2 flex-1 overflow-y-auto">
          <p
            class="px-2 mb-1 text-xs font-medium text-muted-foreground uppercase tracking-wider"
          >
            Connections
          </p>
          {#each $connections as conn}
            {@const isExpanded = expandedConnections.has(conn.id)}
            {@const isLoading = loadingTables.has(conn.id)}
            {@const tables = connectionTables[conn.id] ?? []}

            <!-- Connection row -->
            <div
              class="flex items-center gap-1 px-1 py-1 rounded-md group
          {$activeConnectionId === conn.id
                ? 'bg-accent'
                : 'hover:bg-accent/40'}"
            >
              <!-- Expand/collapse toggle -->
              <button
                class="flex items-center gap-1.5 flex-1 min-w-0 text-left"
                onclick={() => toggleConnection(conn.id)}
              >
                {#if isLoading}
                  <Loader
                    class="w-3.5 h-3.5 shrink-0 text-muted-foreground animate-spin"
                  />
                {:else if isExpanded}
                  <ChevronDown
                    class="w-3.5 h-3.5 shrink-0 text-muted-foreground"
                  />
                {:else}
                  <ChevronRight
                    class="w-3.5 h-3.5 shrink-0 text-muted-foreground"
                  />
                {/if}
                <span
                  class="shrink-0 text-[10px] font-bold px-1.5 py-0.5 rounded border {driverColors[
                    conn.driver
                  ]}"
                >
                  {driverLabel[conn.driver]}
                </span>
                <span
                  class="truncate text-sm
              {$activeConnectionId === conn.id
                    ? 'text-accent-foreground'
                    : 'text-muted-foreground group-hover:text-foreground'}"
                >
                  {conn.name}
                </span>
              </button>

              <!-- New query tab button -->
              <button
                class="shrink-0 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
                title="New query tab"
                onclick={() => {
                  activeConnectionId.set(conn.id);
                  addTab(conn.id);
                }}
              >
                <Plus class="w-3.5 h-3.5 text-muted-foreground" />
              </button>
            </div>

            <!-- Table list -->
            {#if isExpanded}
              <div class="ml-4 mb-1">
                {#if tables.length === 0 && !isLoading}
                  <p class="px-2 py-1 text-xs text-muted-foreground italic">
                    No tables found
                  </p>
                {/if}
                {#each tables as table}
                  <button
                    class="w-full flex items-center gap-2 px-2 py-1 rounded text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors text-left"
                    onclick={() => {
                      activeConnectionId.set(conn.id);
                      addTab(conn.id, `SELECT * FROM ${table} LIMIT 100;`);
                    }}
                  >
                    <Table class="w-3 h-3 shrink-0" />
                    <span class="truncate">{table}</span>
                  </button>
                {/each}
              </div>
            {/if}
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
    </ResizablePrimitive.Pane>

    <ResizablePrimitive.Handle withHandle />

    <!-- Main area -->
    <ResizablePrimitive.Pane defaultSize={84} minSize={50}>
      <div class="h-full flex flex-col overflow-hidden">
        <!-- Tab bar -->
        <div
          class="flex items-stretch border-b border-border bg-background shrink-0 overflow-x-auto"
        >
          {#each $queryTabs as tab}
            {@const conn = $connections.find((c) => c.id === tab.connectionId)}
            <div
              role="tab"
              aria-selected={$activeTabId === tab.id}
              tabindex="0"
              class="group flex items-center gap-2 px-4 py-2.5 text-sm border-r border-border transition-colors shrink-0 cursor-pointer select-none
            {$activeTabId === tab.id
                ? 'bg-muted text-foreground border-b-2 border-b-primary -mb-px'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
              onclick={() => activeTabId.set(tab.id)}
              onkeydown={(e) => e.key === "Enter" && activeTabId.set(tab.id)}
            >
              {#if conn}
                <span
                  class="text-[9px] font-bold px-1 py-0.5 rounded border {driverColors[
                    conn.driver
                  ]}"
                >
                  {driverLabel[conn.driver]}
                </span>
              {/if}
              <span>{tab.title}</span>
              {#if tab.isLoading}
                <span class="w-1.5 h-1.5 rounded-full bg-primary animate-pulse"
                ></span>
              {/if}
              <button
                class="ml-1 rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
                onclick={(e) => {
                  e.stopPropagation();
                  closeTab(tab.id);
                }}
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
              <div class="h-full flex flex-col bg-muted">
                <SqlEditor
                  bind:value={tab.sql}
                  bind:runRef={editorRun}
                  onRun={(sql) => runQuery(tab.id, sql)}
                />
                <div
                  class="flex items-center gap-2 px-3 py-1.5 border-t border-border/60 bg-background/40 shrink-0"
                >
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
                  <div
                    class="flex items-center justify-center h-full gap-2 text-sm text-muted-foreground"
                  >
                    <span class="w-2 h-2 rounded-full bg-primary animate-bounce"
                    ></span>
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
            class="flex-1 flex flex-col items-center justify-center gap-3 text-muted-foreground"
          >
            <Database class="w-12 h-12 opacity-20" />
            <p class="text-sm">No query tab open</p>
            <Button
              variant="outline"
              size="sm"
              onclick={() => (showConnectionForm = true)}
            >
              <Plus class="w-4 h-4 mr-2" /> Add a connection
            </Button>
          </div>
        {/if}
      </div>
    </ResizablePrimitive.Pane>
  </ResizablePrimitive.PaneGroup>
</div>

<ConnectionForm bind:open={showConnectionForm} onSave={saveConnection} />
