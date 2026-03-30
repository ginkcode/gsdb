<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import { toast } from "svelte-sonner";
  import { Plus, Database } from "@lucide/svelte";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import {
    connections,
    activeConnectionId,
    addTab,
    openTableTab,
    reconnectConnection,
    updateConnection,
    closeTabsByConnection,
  } from "$lib/stores/connections";
  import type { Connection } from "$lib/types";
  import ConnectionItem from "./ConnectionItem.svelte";
  import TableActionDialog from "./TableActionDialog.svelte";
  import ImportSqlDialog from "./ImportSqlDialog.svelte";
  import TableInfoDialog from "./TableInfoDialog.svelte";
  import ChangeDatabaseDialog from "./ChangeDatabaseDialog.svelte";

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

  // Change database dialog state
  let changeDatabaseDialogOpen = $state(false);
  let changeDatabaseConnection = $state<Connection | null>(null);

  function openChangeDatabase(conn: Connection) {
    changeDatabaseConnection = conn;
    changeDatabaseDialogOpen = true;
  }

  async function handleChangeDatabase(database: string) {
    if (!changeDatabaseConnection) return;
    const updated = { ...changeDatabaseConnection, database, name: `${changeDatabaseConnection.driver}/${database}` };
    try {
      await invoke("add_connection", { connection: updated });
      closeTabsByConnection(updated.id);
      updateConnection(updated);
      refreshTables(updated.id);
    } catch (e) {
      toast.error("Failed to change database", { description: String(e) });
    }
  }

  // Table destructive action dialog state
  type TableActionType = "delete" | "truncate" | "drop";
  let tableActionDialogOpen = $state(false);
  let tableActionConnId = $state("");
  let tableActionTableName = $state("");
  let tableActionType = $state<TableActionType>("delete");
  let tableActionSql = $state("");
  let tableActionLoading = $state(false);

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

  <ScrollArea class="flex-1 h-0 px-2 py-2">
    <p
      class="px-2 mb-1 text-xs font-medium text-muted-foreground uppercase tracking-wider"
    >
      Connections
    </p>
    {#each $connections as conn}
      <ConnectionItem
        connection={conn}
        isActive={$activeConnectionId === conn.id}
        isExpanded={expandedConnections.has(conn.id)}
        isLoading={loadingTables.has(conn.id)}
        tables={connectionTables[conn.id] ?? []}
        isReconnecting={reconnectingConnections.has(conn.id)}
        onToggle={() => toggleConnection(conn.id)}
        onNewQuery={() => {
          activeConnectionId.set(conn.id);
          addTab(conn.id);
        }}
        onEdit={() => onEditConnection(conn)}
        onReconnect={() => handleReconnect(conn)}
        onRefreshTables={() => refreshTables(conn.id)}
        onRename={() => onRenameConnection(conn)}
        onExport={() => exportDatabase(conn)}
        onImport={() => importSql(conn.id)}
        onChangeDatabase={() => openChangeDatabase(conn)}
        onDelete={() => onDeleteConnection(conn)}
        onOpenTable={(table) =>
          openTableTab(conn.id, table, `SELECT * FROM ${table} LIMIT 100;`)}
        onShowTableInfo={(table) => showTableInfo(conn.id, table)}
        onExportTable={(table) => exportTable(conn.id, table)}
        onImportTable={() => importSql(conn.id)}
        onDeleteTable={(table) =>
          openTableAction("delete", conn.id, table, conn.driver)}
        onTruncateTable={(table) =>
          openTableAction("truncate", conn.id, table, conn.driver)}
        onDropTable={(table) =>
          openTableAction("drop", conn.id, table, conn.driver)}
      />
    {/each}

    {#if $connections.length === 0}
      <button
        class="w-full mt-1 flex items-center gap-2 px-2 py-2 rounded-md text-sm text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors border border-dashed border-border"
        onclick={() => onNewConnection()}
      >
        <Plus class="w-3.5 h-3.5" /> Add connection
      </button>
    {/if}
  </ScrollArea>

  <!-- Change Database Dialog -->
  <ChangeDatabaseDialog
    bind:open={changeDatabaseDialogOpen}
    connection={changeDatabaseConnection}
    onSelect={handleChangeDatabase}
  />

  <!-- Table Action Confirmation Dialog -->
  <TableActionDialog
    bind:open={tableActionDialogOpen}
    actionType={tableActionType}
    tableName={tableActionTableName}
    sql={tableActionSql}
    loading={tableActionLoading}
    onConfirm={confirmTableAction}
    onCancel={() => (tableActionDialogOpen = false)}
  />

  <!-- Import SQL Dialog -->
  <ImportSqlDialog
    bind:open={importDialogOpen}
    fileName={importFileName}
    preview={importPreview}
    truncated={importTruncated}
    totalBytes={importTotalBytes}
    bind:disableFkChecks={importDisableFkChecks}
    loading={importLoading}
    onConfirm={confirmImport}
    onCancel={() => (importDialogOpen = false)}
  />

  <!-- Table Info Dialog -->
  <TableInfoDialog
    bind:open={tableInfoOpen}
    tableName={tableInfoName}
    definition={tableInfoDefinition}
    loading={tableInfoLoading}
    onClose={() => (tableInfoOpen = false)}
  />
</aside>
