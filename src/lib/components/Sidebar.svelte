<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { EditorView } from "@codemirror/view";
  import { sql } from "@codemirror/lang-sql";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { toast } from "svelte-sonner";
  import {
    Plus,
    Database,
    ChevronRight,
    ChevronDown,
    Table,
    Loader,
    Pencil,
    Trash2,
    Eraser,
    RefreshCw,
    Info,
    Copy,
    Download,
    Upload,
  } from "@lucide/svelte";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as Dialog from "$lib/components/ui/dialog";
  import {
    connections,
    activeConnectionId,
    addTab,
    openTableTab,
    reconnectConnection,
  } from "$lib/stores/connections";
  import type { Connection } from "$lib/types";

  let {
    onEditConnection,
    onDeleteConnection,
    onRenameConnection,
    onNewConnection,
    header,
  }: {
    onEditConnection: (conn: Connection) => void;
    onDeleteConnection: (conn: Connection) => void;
    onRenameConnection: (conn: Connection) => void;
    onNewConnection: () => void;
    header?: Snippet;
  } = $props();

  let expandedConnections = $state<Set<string>>(new Set());
  let connectionTables = $state<Record<string, string[]>>({});
  let loadingTables = $state<Set<string>>(new Set());
  let reconnectingConnections = $state<Set<string>>(new Set());

  // Table info dialog state
  let tableInfoOpen = $state(false);
  let tableInfoName = $state("");
  let tableInfoDefinition = $state("");
  let tableInfoLoading = $state(false);
  let tableInfoEditorEl = $state<HTMLDivElement>();
  let tableInfoEditor: EditorView | null = null;

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

  async function toggleConnection(connId: string) {
    // Set this connection as active when clicked
    activeConnectionId.set(connId);

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
          connectionTables = {
            ...connectionTables,
            [connId]: tables,
          };
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

  async function refreshTables(connId: string) {
    loadingTables = new Set([...loadingTables, connId]);
    try {
      const tables: string[] = await invoke("list_tables", {
        connectionId: connId,
      });
      connectionTables = { ...connectionTables, [connId]: tables };
    } catch (err) {
      console.error("Failed to refresh tables:", err);
    } finally {
      loadingTables = new Set([...loadingTables].filter((id) => id !== connId));
    }
  }

  async function handleReconnect(conn: Connection) {
    reconnectingConnections = new Set([...reconnectingConnections, conn.id]);
    try {
      await reconnectConnection(conn.id);
      // Clear cached tables after reconnecting
      const { [conn.id]: _, ...rest } = connectionTables;
      connectionTables = rest;
      await toggleConnection(conn.id);
    } catch (err) {
      console.error("Failed to reconnect:", err);
      alert(`Failed to reconnect: ${err}`);
    } finally {
      reconnectingConnections = new Set(
        [...reconnectingConnections].filter((id) => id !== conn.id),
      );
    }
  }

  async function showTableInfo(connId: string, tableName: string) {
    tableInfoName = tableName;
    tableInfoDefinition = "";
    tableInfoLoading = true;
    tableInfoOpen = true;

    try {
      const definition: string = await invoke("get_table_definition", {
        connectionId: connId,
        tableName: tableName,
      });
      tableInfoDefinition = definition;
    } catch (err) {
      tableInfoDefinition = `Error: ${err}`;
    } finally {
      tableInfoLoading = false;
    }
  }

  async function copyTableDefinition() {
    if (tableInfoDefinition) {
      await navigator.clipboard.writeText(tableInfoDefinition);
      toast.success("Copied");
    }
  }

  // Effect to update CodeMirror editor when definition changes
  $effect(() => {
    if (
      tableInfoOpen &&
      tableInfoEditorEl &&
      tableInfoDefinition &&
      !tableInfoLoading
    ) {
      // Destroy existing editor if any
      if (tableInfoEditor) {
        tableInfoEditor.destroy();
      }
      // Create new editor - minimal setup without line numbers, read-only
      tableInfoEditor = new EditorView({
        doc: tableInfoDefinition,
        extensions: [
          sql(),
          oneDark,
          EditorView.editable.of(false),
          EditorView.lineWrapping,
        ],
        parent: tableInfoEditorEl,
      });
    }
    // Cleanup when dialog closes
    if (!tableInfoOpen && tableInfoEditor) {
      tableInfoEditor.destroy();
      tableInfoEditor = null;
    }
  });

  // Table destructive action dialog state
  type TableActionType = "delete" | "truncate" | "drop";
  let tableActionDialogOpen = $state(false);
  let tableActionConnId = $state("");
  let tableActionTableName = $state("");
  let tableActionType = $state<TableActionType>("delete");
  let tableActionSql = $state("");
  let tableActionLoading = $state(false);

  const tableActionMeta: Record<
    TableActionType,
    { title: string; description: string; confirm: string }
  > = {
    delete: {
      title: "Delete all rows",
      description:
        "All rows will be permanently deleted. The table structure will remain.",
      confirm: "Delete Rows",
    },
    truncate: {
      title: "Truncate table",
      description:
        "All rows will be removed from the table. This cannot be undone.",
      confirm: "Truncate",
    },
    drop: {
      title: "Drop table",
      description: "The table and all its data will be permanently deleted.",
      confirm: "Drop Table",
    },
  };

  function openTableAction(
    type: TableActionType,
    connId: string,
    tableName: string,
    driver: string,
  ) {
    const q = driver === "mysql" ? `\`${tableName}\`` : `"${tableName}"`;
    let sql: string;
    if (type === "delete") {
      sql = `DELETE FROM ${q}`;
    } else if (type === "truncate") {
      sql = driver === "sqlite" ? `DELETE FROM ${q}` : `TRUNCATE TABLE ${q}`;
    } else {
      sql = `DROP TABLE ${q}`;
    }
    tableActionConnId = connId;
    tableActionTableName = tableName;
    tableActionType = type;
    tableActionSql = sql;
    tableActionDialogOpen = true;
  }

  async function confirmTableAction() {
    tableActionLoading = true;
    try {
      await invoke("run_query", {
        connectionId: tableActionConnId,
        sql: tableActionSql,
      });
      tableActionDialogOpen = false;
      if (tableActionType === "drop") {
        connectionTables = {
          ...connectionTables,
          [tableActionConnId]: (
            connectionTables[tableActionConnId] ?? []
          ).filter((t) => t !== tableActionTableName),
        };
      }
      toast.success(
        tableActionType === "delete"
          ? `All rows deleted from "${tableActionTableName}"`
          : tableActionType === "truncate"
            ? `Table "${tableActionTableName}" truncated`
            : `Table "${tableActionTableName}" dropped`,
      );
    } catch (err) {
      toast.error(`Operation failed: ${err}`);
    } finally {
      tableActionLoading = false;
    }
  }

  async function exportDatabase(conn: Connection) {
    const filePath = await save({
      defaultPath: `${conn.name}_export.sql`,
      filters: [{ name: "SQL Files", extensions: ["sql"] }],
    });
    if (!filePath) return;
    try {
      await invoke("export_database", {
        connectionId: conn.id,
        filePath,
      });
      toast.success("Database exported successfully");
    } catch (err) {
      toast.error(`Export failed: ${err}`);
    }
  }

  async function exportTable(connId: string, tableName: string) {
    const filePath = await save({
      defaultPath: `${tableName}_export.sql`,
      filters: [{ name: "SQL Files", extensions: ["sql"] }],
    });
    if (!filePath) return;
    try {
      await invoke("export_table", {
        connectionId: connId,
        tableName,
        filePath,
      });
      toast.success(`Table "${tableName}" exported successfully`);
    } catch (err) {
      toast.error(`Export failed: ${err}`);
    }
  }

  // Import dialog state
  let importDialogOpen = $state(false);
  let importConnId = $state("");
  let importFilePath = $state("");
  let importFileName = $state("");
  let importPreview = $state("");
  let importTruncated = $state(false);
  let importTotalBytes = $state(0);
  let importDisableFkChecks = $state(false);
  let importLoading = $state(false);

  async function importSql(connId: string) {
    const filePath = await open({
      filters: [{ name: "SQL Files", extensions: ["sql"] }],
      multiple: false,
    });
    if (!filePath) return;
    try {
      const preview: {
        content: string;
        truncated: boolean;
        total_bytes: number;
      } = await invoke("read_file_preview", { filePath });
      importConnId = connId;
      importFilePath = filePath as string;
      importFileName =
        (filePath as string).split("/").pop() ?? (filePath as string);
      importPreview = preview.content;
      importTruncated = preview.truncated;
      importTotalBytes = preview.total_bytes;
      importDisableFkChecks = false;
      importDialogOpen = true;
    } catch (err) {
      toast.error(`Failed to read file: ${err}`);
    }
  }

  async function confirmImport() {
    importLoading = true;
    try {
      const result: string = await invoke("import_sql", {
        connectionId: importConnId,
        filePath: importFilePath,
        disableFkChecks: importDisableFkChecks,
      });
      importDialogOpen = false;
      toast.success(result);
    } catch (err) {
      toast.error(`Import failed: ${err}`);
    } finally {
      importLoading = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  export function refreshConnectionTables(connId: string) {
    refreshTables(connId);
  }
</script>

<aside class="h-full flex flex-col border-r border-border bg-background">
  <div
    class="flex items-center justify-between px-4 py-3 border-b border-border"
  >
    <div class="flex items-center gap-2">
      <Database class="w-4 h-4 text-primary" />
      <span class="text-sm font-semibold tracking-tight">GSDB</span>
    </div>
    <div class="flex items-center gap-1">
      {#if header}{@render header()}{/if}
      <Button
        variant="ghost"
        size="icon"
        class="h-7 w-7"
        onclick={() => onNewConnection()}
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

      {#snippet connMenuItems(Item: any, Separator: any)}
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => onEditConnection(conn)}
        >
          <Pencil class="w-4 h-4" /><span>Edit</span>
        </Item>
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => handleReconnect(conn)}
          disabled={reconnectingConnections.has(conn.id)}
        >
          <RefreshCw
            class="w-4 h-4 {reconnectingConnections.has(conn.id)
              ? 'animate-spin'
              : ''}"
          />
          <span
            >{reconnectingConnections.has(conn.id)
              ? "Reconnecting..."
              : "Reconnect"}</span
          >
        </Item>
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => refreshTables(conn.id)}
        >
          <svg
            class="w-4 h-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
            <path d="M21 3v5h-5" />
          </svg>
          <span>Refresh Tables</span>
        </Item>
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => onRenameConnection(conn)}
        >
          <svg
            class="w-4 h-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
          </svg>
          <span>Rename</span>
        </Item>
        <Separator />
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => exportDatabase(conn)}
        >
          <Download class="w-4 h-4" /><span>Export Database</span>
        </Item>
        <Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => importSql(conn.id)}
        >
          <Upload class="w-4 h-4" /><span>Import SQL</span>
        </Item>
        <Separator />
        <Item
          class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
          onclick={() => onDeleteConnection(conn)}
        >
          <Trash2 class="w-4 h-4" /><span>Delete</span>
        </Item>
      {/snippet}

      <!-- Connection row -->
      <ContextMenu.Root>
        <ContextMenu.Trigger>
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

            <!-- Three-dot dropdown -->
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                <button
                  class="shrink-0 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
                  title="More options"
                >
                  <svg
                    class="w-3.5 h-3.5 text-muted-foreground"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                  >
                    <circle cx="12" cy="5" r="1" />
                    <circle cx="12" cy="12" r="1" />
                    <circle cx="12" cy="19" r="1" />
                  </svg>
                </button>
              </DropdownMenu.Trigger>
              <DropdownMenu.Content class="min-w-44">
                {@render connMenuItems(
                  DropdownMenu.Item,
                  DropdownMenu.Separator,
                )}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>
        </ContextMenu.Trigger>
        <ContextMenu.Content>
          {@render connMenuItems(ContextMenu.Item, ContextMenu.Separator)}
        </ContextMenu.Content>
      </ContextMenu.Root>

      <!-- Table list -->
      {#if isExpanded}
        <div class="ml-4 mb-1">
          {#if tables.length === 0 && !isLoading}
            <p class="px-2 py-1 text-xs text-muted-foreground italic">
              No tables found
            </p>
          {/if}
          {#each tables as table}
            {#snippet tableMenuItems(Item: any, Separator: any)}
              <Item
                class="flex items-center gap-2 cursor-pointer"
                onclick={() => showTableInfo(conn.id, table)}
              >
                <Info class="w-4 h-4" /><span>Info</span>
              </Item>
              <Separator />
              <Item
                class="flex items-center gap-2 cursor-pointer"
                onclick={() => exportTable(conn.id, table)}
              >
                <Download class="w-4 h-4" /><span>Export Table</span>
              </Item>
              <Item
                class="flex items-center gap-2 cursor-pointer"
                onclick={() => importSql(conn.id)}
              >
                <Upload class="w-4 h-4" /><span>Import SQL</span>
              </Item>
              <Separator />
              <Item
                class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
                onclick={() =>
                  openTableAction("delete", conn.id, table, conn.driver)}
              >
                <Trash2 class="w-4 h-4" /><span>Delete</span>
              </Item>
              <Item
                class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
                onclick={() =>
                  openTableAction("truncate", conn.id, table, conn.driver)}
              >
                <Eraser class="w-4 h-4" /><span>Truncate</span>
              </Item>
              <Item
                class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
                onclick={() =>
                  openTableAction("drop", conn.id, table, conn.driver)}
              >
                <Trash2 class="w-4 h-4" /><span>Drop Table</span>
              </Item>
            {/snippet}

            <ContextMenu.Root>
              <ContextMenu.Trigger>
                <div class="group flex items-center gap-1">
                  <button
                    class="flex-1 flex items-center gap-2 px-2 py-1 rounded text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors text-left"
                    onclick={() => {
                      openTableTab(
                        conn.id,
                        table,
                        `SELECT * FROM ${table} LIMIT 100;`,
                      );
                    }}
                  >
                    <Table class="w-3 h-3 shrink-0" />
                    <span class="truncate">{table}</span>
                  </button>
                  <!-- Three-dot dropdown -->
                  <DropdownMenu.Root>
                    <DropdownMenu.Trigger>
                      <button
                        class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
                        title="Table options"
                      >
                        <svg
                          class="w-3 h-3 text-muted-foreground"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                        >
                          <circle cx="12" cy="5" r="1" />
                          <circle cx="12" cy="12" r="1" />
                          <circle cx="12" cy="19" r="1" />
                        </svg>
                      </button>
                    </DropdownMenu.Trigger>
                    <DropdownMenu.Content class="min-w-36">
                      {@render tableMenuItems(
                        DropdownMenu.Item,
                        DropdownMenu.Separator,
                      )}
                    </DropdownMenu.Content>
                  </DropdownMenu.Root>
                </div>
              </ContextMenu.Trigger>
              <ContextMenu.Content>
                {@render tableMenuItems(
                  ContextMenu.Item,
                  ContextMenu.Separator,
                )}
              </ContextMenu.Content>
            </ContextMenu.Root>
          {/each}
        </div>
      {/if}
    {/each}

    {#if $connections.length === 0}
      <button
        class="w-full mt-1 flex items-center gap-2 px-2 py-2 rounded-md text-sm text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors border border-dashed border-border"
        onclick={() => onNewConnection()}
      >
        <Plus class="w-3.5 h-3.5" /> Add connection
      </button>
    {/if}
  </div>

  <!-- Table Action Confirmation Dialog -->
  <Dialog.Root bind:open={tableActionDialogOpen}>
    <Dialog.Content class="sm:max-w-md">
      <Dialog.Header>
        <Dialog.Title
          >{tableActionMeta[tableActionType].title}:
          <strong>{tableActionTableName}</strong></Dialog.Title
        >
        <Dialog.Description>
          {tableActionMeta[tableActionType].description}
        </Dialog.Description>
      </Dialog.Header>
      <div
        class="rounded border border-border bg-muted/40 px-3 py-2 font-mono text-xs text-muted-foreground"
      >
        {tableActionSql}
      </div>
      <Dialog.Footer>
        <Button
          variant="outline"
          onclick={() => (tableActionDialogOpen = false)}
          disabled={tableActionLoading}>Cancel</Button
        >
        <Button
          variant="destructive"
          onclick={confirmTableAction}
          disabled={tableActionLoading}
        >
          {#if tableActionLoading}
            <Loader class="w-4 h-4 mr-2 animate-spin" />
            Processing...
          {:else}
            {tableActionMeta[tableActionType].confirm}
          {/if}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Import SQL Dialog -->
  <Dialog.Root bind:open={importDialogOpen}>
    <Dialog.Content
      class="sm:max-w-3xl max-h-[85vh] overflow-hidden flex flex-col w-full"
    >
      <Dialog.Header>
        <Dialog.Title>Import SQL</Dialog.Title>
        <Dialog.Description class="text-xs text-muted-foreground truncate">
          {importFileName}
          <span class="ml-2 text-muted-foreground/60"
            >({formatBytes(importTotalBytes)})</span
          >
        </Dialog.Description>
      </Dialog.Header>

      <!-- SQL preview -->
      <div
        class="flex-1 overflow-auto rounded border border-border bg-muted/30"
      >
        <pre
          class="p-3 text-xs font-mono whitespace-pre-wrap break-all leading-relaxed text-foreground">{importPreview}</pre>
        {#if importTruncated}
          <div
            class="sticky bottom-0 px-3 py-2 text-xs text-amber-500 bg-muted/80 border-t border-border"
          >
            Preview truncated — showing first 16 KB of {formatBytes(
              importTotalBytes,
            )} file. Full file will be imported.
          </div>
        {/if}
      </div>

      <!-- Options -->
      <div class="pt-2">
        <label class="flex items-center gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            class="rounded border-border"
            bind:checked={importDisableFkChecks}
          />
          <span class="text-sm">Disable foreign key checks during import</span>
        </label>
        <p class="mt-1 ml-6 text-xs text-muted-foreground">
          Useful when importing data with circular references or out-of-order
          inserts.
        </p>
      </div>

      <Dialog.Footer>
        <Button
          variant="outline"
          onclick={() => (importDialogOpen = false)}
          disabled={importLoading}>Cancel</Button
        >
        <Button onclick={confirmImport} disabled={importLoading}>
          {#if importLoading}
            <Loader class="w-4 h-4 mr-2 animate-spin" />
            Importing...
          {:else}
            Confirm Import
          {/if}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Table Info Dialog -->
  <Dialog.Root bind:open={tableInfoOpen}>
    <Dialog.Content
      class="sm:max-w-3xl max-h-[85vh] overflow-hidden flex flex-col w-full"
    >
      <Dialog.Header>
        <Dialog.Title>Table: <strong>{tableInfoName}</strong></Dialog.Title>
      </Dialog.Header>
      <div class="flex-1 overflow-auto">
        {#if tableInfoLoading}
          <div class="flex items-center justify-center py-8">
            <Loader class="w-6 h-6 animate-spin text-muted-foreground" />
          </div>
        {:else if tableInfoDefinition}
          <div bind:this={tableInfoEditorEl} class="cm-editor-wrapper"></div>
        {/if}
      </div>
      <Dialog.Footer>
        <Button
          variant="ghost"
          size="icon"
          onclick={copyTableDefinition}
          disabled={!tableInfoDefinition || tableInfoLoading}
          title="Copy to clipboard"
        >
          <Copy class="w-4 h-4" />
        </Button>
        <Button variant="outline" onclick={() => (tableInfoOpen = false)}
          >Close</Button
        >
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
</aside>

<style>
  .cm-editor-wrapper :global(.cm-editor) {
    background: transparent;
    font-size: 0.875rem;
    height: auto;
    min-height: 200px;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-scroller) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
      "Liberation Mono", "Courier New", monospace;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-content) {
    padding: 0;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-line) {
    padding: 0;
  }
</style>
