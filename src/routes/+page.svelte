<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { Plus, Database } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as Dialog from "$lib/components/ui/dialog";
  import * as ResizablePrimitive from "$lib/components/ui/resizable";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import TabBar from "$lib/components/TabBar.svelte";
  import QueryWorkspace from "$lib/components/QueryWorkspace.svelte";
  import RowDetailPanel from "$lib/components/RowDetailPanel.svelte";
  import ConnectionForm from "$lib/components/ConnectionForm.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { toast } from "svelte-sonner";
  import { platform } from "$lib/stores/platform";
  import {
    connections,
    activeConnectionId,
    queryTabs,
    activeTabId,
    activeTab,
    addTab,
    closeTab,
    updateTab,
    loadSavedConnections,
    addConnection,
    removeConnection,
    renameConnection,
    updateConnection,
    makeTabPermanent,
    addQueryHistory,
  } from "$lib/stores/connections";
  import type { Connection, QueryResult } from "$lib/types";

  let showConnectionForm = $state(false);
  let editingConnection = $state<Connection | null>(null);
  let deletingConnection = $state<Connection | null>(null);
  let showRenameDialog = $state(false);
  let showDeleteDialog = $state(false);
  let renameValue = $state("");
  let selectedRow = $state<Record<string, unknown> | null>(null);
  let selectedRowIndex = $state<number>(0);
  let showDetailPanel = $state(true);
  let isMaximized = $state(false);
  let currentPlatform = $state<"windows" | "macos" | "linux" | "unknown">(
    "unknown",
  );

  // Load saved connections on mount
  onMount((): (() => void) => {
    loadSavedConnections();
    platform.initialize();

    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    (async () => {
      isMaximized = await appWindow.isMaximized();
      unlisten = await appWindow.onResized(async () => {
        isMaximized = await appWindow.isMaximized();
      });
    })();

    // Subscribe to platform changes
    const unsubPlatform = platform.subscribe((p) => {
      currentPlatform = p;
    });

    // Handle keyboard shortcuts for closing tabs
    function handleKeyDown(e: KeyboardEvent) {
      // Ctrl+W on Windows/Linux, Cmd+W on Mac
      if ((e.ctrlKey || e.metaKey) && e.key === "w") {
        e.preventDefault();
        const currentTabId = $activeTabId;
        if (currentTabId) {
          closeTab(currentTabId);
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      unlisten?.();
      unsubPlatform();
    };
  });

  async function saveConnection(conn: Connection): Promise<boolean> {
    try {
      if (editingConnection) {
        await updateConnection(conn);
        editingConnection = null;
      } else {
        await invoke("add_connection", { connection: conn });
        await addConnection(conn);
        activeConnectionId.set(conn.id);
        addTab(conn.id);
      }
      return true;
    } catch (err) {
      toast.error("Connection failed", { description: String(err) });
      return false;
    }
  }

  /**
   * Check if a SQL query is read-only (SELECT, SHOW, DESCRIBE, EXPLAIN, etc.)
   * Returns true for safe read-only queries, false for write operations.
   */
  function isReadOnlyQuery(sql: string): boolean {
    const normalizedSql = sql.trim().toUpperCase();
    // Remove comments and leading whitespace
    const cleanSql = normalizedSql
      .replace(/--.*$/gm, "")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .trim();

    // Read-only keywords
    const readOnlyPatterns = [
      /^SELECT\b/i,
      /^SHOW\b/i,
      /^DESCRIBE\b/i,
      /^DESC\b/i,
      /^EXPLAIN\b/i,
      /^WITH\b/i, // CTE queries that start with WITH
      /^\(?\s*SELECT\b/i, // Subqueries starting with SELECT
    ];

    return readOnlyPatterns.some((pattern) => pattern.test(cleanSql));
  }

  async function runQuery(tabId: string, sql: string) {
    const tab = $queryTabs.find((t) => t.id === tabId);
    if (!tab) return;

    // Check if connection is locked and query is not read-only
    const conn = $connections.find((c) => c.id === tab.connectionId);
    if (conn?.locked && !isReadOnlyQuery(sql)) {
      toast.error("Connection is locked", {
        description:
          "This connection is in read-only mode. Only SELECT queries are allowed. Unlock the connection to make changes.",
      });
      return;
    }

    // Make temporary tabs permanent when running a query
    if (tab.temporary) {
      makeTabPermanent(tabId);
    }

    // Clean up SQL - trim and remove trailing semicolon for PostgreSQL compatibility
    const cleanSql = sql.trim().replace(/;+\s*$/, "");

    updateTab(tabId, { isLoading: true, result: undefined });
    try {
      const result: QueryResult = await invoke("run_query", {
        connectionId: tab.connectionId,
        sql: cleanSql,
      });
      // Enrich column nullability from schema when the tab is a table view
      if (tab.title !== "Query" && result.columns.length > 0) {
        try {
          const nullableMap: Record<string, boolean> = await invoke(
            "get_column_nullable",
            { connectionId: tab.connectionId, tableName: tab.title },
          );
          result.columnNullable = result.columns.map(
            (col) => nullableMap[col] ?? true,
          );
        } catch {
          // schema lookup failed — leave as default (all nullable)
        }
      }
      // Store the last executed SQL for refreshing after updates
      updateTab(tabId, { result, isLoading: false, lastExecutedSql: cleanSql });
      // Log successful query to history
      addQueryHistory(
        tab.connectionId,
        cleanSql,
        true,
        undefined,
        result.rowsAffected ?? result.rows?.length,
      );
    } catch (err) {
      updateTab(tabId, {
        result: { columns: [], rows: [], error: String(err) },
        isLoading: false,
      });
      // Log failed query to history
      addQueryHistory(tab.connectionId, cleanSql, false, String(err));
    }
  }

  function handleEditConnection(conn: Connection) {
    editingConnection = conn;
    showConnectionForm = true;
  }

  function handleDeleteConnection(conn: Connection) {
    deletingConnection = conn;
    showDeleteDialog = true;
  }

  function handleRenameConnection(conn: Connection) {
    editingConnection = conn;
    renameValue = conn.name;
    showRenameDialog = true;
  }

  function handleNewConnection() {
    editingConnection = null;
    showConnectionForm = true;
  }

  function handleRowSelect(row: Record<string, unknown>) {
    selectedRow = row;
    showDetailPanel = true;
    // Find the index of the selected row
    const tab = $activeTab;
    if (tab?.result?.rows) {
      const idx = tab.result.rows.findIndex((r) => r === row);
      if (idx >= 0) {
        selectedRowIndex = idx;
      }
    }
  }

  function handleCloseDetailPanel() {
    showDetailPanel = false;
    selectedRow = null;
  }

  // When switching tabs or refreshing results, preserve the selected row by index
  $effect(() => {
    const tab = $activeTab;
    if (tab?.result?.rows && tab.result.rows.length > 0) {
      // Preserve the selected row by index if possible
      if (selectedRowIndex < tab.result.rows.length) {
        selectedRow = tab.result.rows[selectedRowIndex];
      } else {
        // Index out of bounds, select first row
        selectedRow = tab.result.rows[0];
        selectedRowIndex = 0;
      }
    } else {
      // No rows in the current tab, hide the detail panel
      selectedRow = null;
      showDetailPanel = false;
    }
  });

  // Reset selected row index when switching tabs
  $effect(() => {
    // Track activeTabId changes
    const tabId = $activeTabId;
    // Reset to first row when tab changes
    selectedRowIndex = 0;
  });

  let columns = $derived($activeTab?.result?.columns ?? []);
  let columnTypes = $derived($activeTab?.result?.columnTypes ?? []);
  let columnNullable = $derived($activeTab?.result?.columnNullable ?? []);
</script>

<!-- Theme-aware root -->
<div
  class="h-screen bg-background text-foreground overflow-hidden font-sans antialiased flex flex-col {isMaximized
    ? 'rounded-none'
    : currentPlatform === 'windows'
      ? 'rounded-md border border-border'
      : 'rounded-xl border border-border'}"
>
  <!-- Custom Title Bar -->
  <TitleBar />

  <ResizablePrimitive.PaneGroup direction="horizontal" class="flex-1 min-h-0">
    <!-- Sidebar -->
    <ResizablePrimitive.Pane defaultSize={22} minSize={10} maxSize={35}>
      <Sidebar
        onEditConnection={handleEditConnection}
        onDeleteConnection={handleDeleteConnection}
        onRenameConnection={handleRenameConnection}
        onNewConnection={handleNewConnection}
      >
        {#snippet header()}
          <ThemeToggle />
        {/snippet}
      </Sidebar>
    </ResizablePrimitive.Pane>

    <ResizablePrimitive.Handle withHandle />

    <!-- Main area -->
    <ResizablePrimitive.Pane defaultSize={78} minSize={50}>
      <div class="h-full flex flex-col overflow-hidden">
        <!-- Tab bar -->
        <TabBar />

        <!-- Workspace with detail panel -->
        <ResizablePrimitive.PaneGroup
          direction="horizontal"
          class="flex-1 overflow-hidden"
        >
          <ResizablePrimitive.Pane defaultSize={100} minSize={30}>
            <QueryWorkspace
              {runQuery}
              {selectedRow}
              onRowSelect={handleRowSelect}
            />
          </ResizablePrimitive.Pane>

          {#if showDetailPanel && selectedRow}
            <ResizablePrimitive.Handle withHandle />
            <ResizablePrimitive.Pane
              defaultSize={35}
              minSize={20}
              maxSize={50}
              class="h-full overflow-hidden"
            >
              <RowDetailPanel
                row={selectedRow}
                {columns}
                {columnTypes}
                {columnNullable}
                connectionId={$activeTab?.connectionId}
                tableName={$activeTab?.title !== "Query"
                  ? $activeTab?.title
                  : undefined}
                driver={$connections.find(
                  (c) => c.id === $activeTab?.connectionId,
                )?.driver ?? "postgres"}
                locked={$connections.find(
                  (c) => c.id === $activeTab?.connectionId,
                )?.locked ?? false}
                onClose={handleCloseDetailPanel}
                onUpdateSuccess={() => {
                  if ($activeTab) {
                    // Use the last executed SQL for refreshing, not the current editor content
                    const refreshSql =
                      $activeTab.lastExecutedSql ?? $activeTab.sql;
                    runQuery($activeTab.id, refreshSql);
                  }
                }}
                onQueryExecuted={(sql, success, error) => {
                  if ($activeTab) {
                    addQueryHistory(
                      $activeTab.connectionId,
                      sql,
                      success,
                      error,
                    );
                  }
                }}
              />
            </ResizablePrimitive.Pane>
          {/if}
        </ResizablePrimitive.PaneGroup>
      </div>
    </ResizablePrimitive.Pane>
  </ResizablePrimitive.PaneGroup>
</div>

<ConnectionForm
  bind:open={showConnectionForm}
  bind:editing={editingConnection}
  onSave={saveConnection}
/>

<!-- Rename Dialog -->
<Dialog.Root bind:open={showRenameDialog}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Rename Connection</Dialog.Title>
    </Dialog.Header>
    <div class="grid gap-4 py-2">
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="rename-name">Name</label>
        <Input
          id="rename-name"
          bind:value={renameValue}
          placeholder="Connection name"
        />
      </div>
    </div>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (showRenameDialog = false)}>
        Cancel
      </Button>
      <Button
        onclick={async () => {
          if (editingConnection) {
            await renameConnection(editingConnection.id, renameValue);
            showRenameDialog = false;
            editingConnection = null;
          }
        }}
      >
        Save
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- Delete Confirmation Dialog -->
<Dialog.Root bind:open={showDeleteDialog}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Delete Connection</Dialog.Title>
      <Dialog.Description>
        Are you sure you want to delete "{deletingConnection?.name}"? This
        action cannot be undone.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (showDeleteDialog = false)}>
        Cancel
      </Button>
      <Button
        variant="destructive"
        onclick={async () => {
          if (deletingConnection) {
            const connId = deletingConnection.id;
            await removeConnection(connId);
            // Close any tabs for this connection
            queryTabs.update((tabs) =>
              tabs.filter((t) => t.connectionId !== connId),
            );
            if ($activeConnectionId === connId) {
              activeConnectionId.set(null);
            }
            showDeleteDialog = false;
            deletingConnection = null;
          }
        }}
      >
        Delete
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
