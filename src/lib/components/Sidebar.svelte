<script lang="ts">
  import { invoke, Channel } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { Snippet } from "svelte";
  import { toast } from "svelte-sonner";
  import { Plus, Database } from "@lucide/svelte";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { downloadDir } from "@tauri-apps/api/path";
  import { Button } from "$lib/components/ui/button";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import * as Dialog from "$lib/components/ui/dialog";
  import {
    connections,
    activeConnectionId,
    activeTabId,
    queryTabs,
    addTab,
    openTableTab,
    openTableTabPreview,
    reconnectConnection,
    updateConnection,
    closeTabsByConnection,
    openDiagramTab,
    toggleConnectionLock,
  } from "$lib/stores/connections";
  import type { Connection, TableInfo, QueryTab } from "$lib/types";
  import ConnectionItem from "./ConnectionItem.svelte";
  import TableActionDialog from "./TableActionDialog.svelte";
  import ImportSqlDialog from "./ImportSqlDialog.svelte";
  import TableInfoDialog from "./TableInfoDialog.svelte";
  import ChangeDatabaseDialog from "./ChangeDatabaseDialog.svelte";
  import DiagramPickerDialog from "./DiagramPickerDialog.svelte";
  import QueryHistoryDialog from "./QueryHistoryDialog.svelte";
  import ServerInfoDialog from "./ServerInfoDialog.svelte";
  import ExportDatabaseDialog from "./ExportDatabaseDialog.svelte";

  let {
    onEditConnection,
    onDeleteConnection,
    onNewConnection,
    header,
  }: {
    onEditConnection: (conn: Connection) => void;
    onDeleteConnection: (conn: Connection) => void;
    onNewConnection: () => void;
    header?: Snippet;
  } = $props();

  const RELEASES_URL = "https://github.com/ginkcode/gsdb/releases/latest";

  let appVersion = $state("");
  let updateAvailable = $state(false);

  function isNewerVersion(latest: string, current: string): boolean {
    const lp = latest.split(".").map(Number);
    const cp = current.split(".").map(Number);
    for (let i = 0; i < Math.max(lp.length, cp.length); i++) {
      const l = lp[i] ?? 0;
      const c = cp[i] ?? 0;
      if (l > c) return true;
      if (l < c) return false;
    }
    return false;
  }

  getVersion().then(async (v) => {
    appVersion = v;
    try {
      const res = await fetch(
        "https://api.github.com/repos/ginkcode/gsdb/releases/latest",
        {
          headers: {
            Accept: "application/vnd.github.v3+json",
            "User-Agent": "GSDB-App",
          },
        },
      );
      if (!res.ok) {
        console.error("[Update] GitHub API error:", res.status, res.statusText);
        return;
      }
      const data = await res.json();
      const latest = String(data.tag_name ?? "").replace(/^v/, "");
      console.log("[Update] Current:", v, "Latest:", latest);
      if (latest && isNewerVersion(latest, v)) {
        console.log("[Update] Update available!");
        updateAvailable = true;
      }
    } catch (err) {
      console.error("[Update] Failed to check for updates:", err);
      // silently ignore — no network or rate limit
    }
  });

  let expandedConnections = $state<Set<string>>(new Set());
  let connectionTables = $state<Record<string, TableInfo[]>>({});
  let loadingTables = $state<Set<string>>(new Set());
  let reconnectingConnections = $state<Set<string>>(new Set());

  // Table info dialog state
  let tableInfoOpen = $state(false);
  let tableInfoName = $state("");
  let tableInfoDefinition = $state("");
  let tableInfoLoading = $state(false);

  // Unlock confirmation dialog state
  let unlockDialogOpen = $state(false);
  let unlockDialogConnection = $state<Connection | null>(null);

  // Query history dialog state
  let queryHistoryDialogOpen = $state(false);
  let queryHistoryConnection = $state<Connection | null>(null);

  // Server info dialog state
  let serverInfoDialogOpen = $state(false);
  let serverInfoConnection = $state<Connection | null>(null);

  const driverLabel: Record<string, string> = {
    postgres: "PG",
    mysql: "MY",
    sqlite: "SQ",
    sqlserver: "MS",
  };

  // Generate a SELECT query with limit, using dialect-appropriate syntax
  function selectQuery(driver: string, table: string, limit = 100): string {
    if (driver === "sqlserver") {
      return `SELECT TOP ${limit} * FROM [${table}];`;
    }
    const quoted = driver === "mysql" ? `\`${table}\`` : `"${table}"`;
    return `SELECT * FROM ${quoted} LIMIT ${limit};`;
  }

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
          const tables: TableInfo[] = await invoke("list_tables", {
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

  export async function refreshTables(connId: string) {
    loadingTables = new Set([...loadingTables, connId]);
    try {
      const tables: TableInfo[] = await invoke("list_tables", {
        connectionId: connId,
      });
      connectionTables = { ...connectionTables, [connId]: tables };
    } catch (err) {
      console.error("Failed to refresh tables:", err);
    } finally {
      loadingTables = new Set([...loadingTables].filter((id) => id !== connId));
    }
  }

  async function handleReconnect(conn: Connection): Promise<boolean> {
    reconnectingConnections = new Set([...reconnectingConnections, conn.id]);
    try {
      await reconnectConnection(conn.id);
      // Clear cached tables and always expand after reconnecting
      const { [conn.id]: _, ...rest } = connectionTables;
      connectionTables = rest;
      activeConnectionId.set(conn.id);
      expandedConnections = new Set([...expandedConnections, conn.id]);
      await refreshTables(conn.id);
      return true;
    } catch (err) {
      console.error("Failed to reconnect:", err);
      toast.error("Reconnect failed", { description: String(err) });
      return false;
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

  // Diagram picker dialog state
  let diagramPickerOpen = $state(false);
  let diagramPickerConnId = $state("");
  let diagramPickerInitialTables = $state<string[]>([]);

  function openDiagram(conn: Connection) {
    // Check if a diagram tab already exists for this connection
    let existingDiagramTab: QueryTab | undefined;
    queryTabs.subscribe((tabs) => {
      existingDiagramTab = tabs.find(
        (t) => t.connectionId === conn.id && t.kind === "diagram",
      );
    })();

    // Pass the currently selected tables (if any) to the picker dialog
    diagramPickerInitialTables = existingDiagramTab?.selectedTables ?? [];
    diagramPickerConnId = conn.id;
    diagramPickerOpen = true;
  }

  // Change database dialog state
  let changeDatabaseDialogOpen = $state(false);
  let changeDatabaseConnection = $state<Connection | null>(null);

  function openChangeDatabase(conn: Connection) {
    changeDatabaseConnection = conn;
    changeDatabaseDialogOpen = true;
  }

  function handleToggleLock(conn: Connection) {
    if (conn.locked) {
      // Show confirmation dialog before unlocking
      unlockDialogConnection = conn;
      unlockDialogOpen = true;
    } else {
      // Lock immediately without confirmation
      toggleConnectionLock(conn.id);
    }
  }

  function confirmUnlock() {
    if (unlockDialogConnection) {
      toggleConnectionLock(unlockDialogConnection.id);
      unlockDialogConnection = null;
    }
    unlockDialogOpen = false;
  }

  function openQueryHistory(conn: Connection) {
    queryHistoryConnection = conn;
    queryHistoryDialogOpen = true;
  }

  function openServerInfo(conn: Connection) {
    serverInfoConnection = conn;
    serverInfoDialogOpen = true;
  }

  function handleSelectHistoryQuery(sql: string) {
    if (queryHistoryConnection) {
      activeConnectionId.set(queryHistoryConnection.id);
      addTab(queryHistoryConnection.id, sql);
      queryHistoryDialogOpen = false;
    }
  }

  async function handleChangeDatabase(database: string) {
    if (!changeDatabaseConnection) return;
    const updated = {
      ...changeDatabaseConnection,
      database,
      name: `${changeDatabaseConnection.driver}/${database}`,
    };
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
  let tableActionTableKind = $state<"table" | "view">("table");
  let tableActionType = $state<TableActionType>("delete");
  let tableActionSql = $state("");
  let tableActionLoading = $state(false);

  function openTableAction(
    type: TableActionType,
    connId: string,
    tableName: string,
    tableKind: "table" | "view",
    driver: string,
  ) {
    // Check if connection is locked
    const conn = $connections.find((c) => c.id === connId);
    if (conn?.locked) {
      toast.error("Connection is locked", {
        description:
          "This connection is in read-only mode. Unlock the connection to make changes.",
      });
      return;
    }

    const q = driver === "mysql" ? `\`${tableName}\`` : `"${tableName}"`;
    let sql: string;
    if (type === "delete") {
      sql = `DELETE FROM ${q}`;
    } else if (type === "truncate") {
      sql = driver === "sqlite" ? `DELETE FROM ${q}` : `TRUNCATE TABLE ${q}`;
    } else {
      sql = tableKind === "view" ? `DROP VIEW ${q}` : `DROP TABLE ${q}`;
    }
    tableActionConnId = connId;
    tableActionTableName = tableName;
    tableActionTableKind = tableKind;
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
          ).filter((t) => t.name !== tableActionTableName),
        };
      }
      toast.success(
        tableActionType === "delete"
          ? `All rows deleted from "${tableActionTableName}"`
          : tableActionType === "truncate"
            ? `Table "${tableActionTableName}" truncated`
            : tableActionTableKind === "view"
              ? `View "${tableActionTableName}" dropped`
              : `Table "${tableActionTableName}" dropped`,
      );
    } catch (err) {
      toast.error(`Operation failed: ${err}`);
    } finally {
      tableActionLoading = false;
    }
  }

  // Export database dialog state
  let exportDialogOpen = $state(false);
  let exportDialogConnection = $state<Connection | null>(null);

  // Export progress state
  let exportProgressOpen = $state(false);
  let exportProgressCurrent = $state("");
  let exportProgressIndex = $state(0);
  let exportProgressTotal = $state(0);

  function utcTimestamp(): string {
    return (
      new Date()
        .toISOString()
        .replace(/[-:]/g, "")
        .replace("T", "_")
        .slice(0, 15) + "Z"
    );
  }

  function openExportDialog(conn: Connection) {
    exportDialogConnection = conn;
    exportDialogOpen = true;
  }

  async function handleExportConfirm(
    tables: import("$lib/types").TableExportOptions[],
  ) {
    if (!exportDialogConnection) return;
    const conn = exportDialogConnection;
    exportDialogOpen = false;

    const downloads = await downloadDir();
    const ts = utcTimestamp();
    const filePath = await save({
      defaultPath: downloads
        ? `${downloads}/${conn.name}_export_${ts}.sql`
        : `${conn.name}_export_${ts}.sql`,
      filters: [{ name: "SQL Files", extensions: ["sql"] }],
    });
    if (!filePath) return;

    exportProgressOpen = true;
    exportProgressCurrent = "Starting export...";
    exportProgressIndex = 0;
    exportProgressTotal = 0;

    const onEvent = new Channel<import("$lib/types").ExportProgress>();
    onEvent.onmessage = (progress) => {
      if (progress.type === "started") {
        exportProgressTotal = progress.totalTables;
        exportProgressCurrent = `Exporting ${progress.totalTables} tables...`;
      } else if (progress.type === "table") {
        exportProgressIndex = progress.index + 1;
        exportProgressCurrent = `Exporting table: ${progress.name}`;
      } else if (progress.type === "done") {
        exportProgressOpen = false;
        toast.success("Database exported successfully");
      } else if (progress.type === "error") {
        exportProgressOpen = false;
        toast.error(`Export failed: ${progress.message}`);
      }
    };

    try {
      await invoke("export_tables", {
        connectionId: conn.id,
        tables,
        filePath,
        onEvent,
      });
    } catch (err) {
      exportProgressOpen = false;
      toast.error(`Export failed: ${err}`);
    }
  }

  async function exportTable(connId: string, tableName: string) {
    const downloads = await downloadDir();
    const ts = utcTimestamp();
    const filePath = await save({
      defaultPath: downloads
        ? `${downloads}/${tableName}_export_${ts}.sql`
        : `${tableName}_export_${ts}.sql`,
      filters: [{ name: "SQL Files", extensions: ["sql"] }],
    });
    if (!filePath) return;

    const onEvent = new Channel<import("$lib/types").ExportProgress>();
    onEvent.onmessage = (progress) => {
      if (progress.type === "done") {
        toast.success(`Table "${tableName}" exported successfully`);
      } else if (progress.type === "error") {
        toast.error(`Export failed: ${progress.message}`);
      }
    };

    try {
      await invoke("export_table", {
        connectionId: connId,
        tableName,
        filePath,
        onEvent,
      });
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
    // Check if connection is locked
    const conn = $connections.find((c) => c.id === connId);
    if (conn?.locked) {
      toast.error("Connection is locked", {
        description:
          "This connection is in read-only mode. Unlock the connection to import data.",
      });
      return;
    }

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

  // Import progress state
  let importProgressOpen = $state(false);
  let importProgressDone = $state(0);
  let importProgressTotal = $state(0);
  let importCancelConfirm = $state(false);

  async function requestCancelImport() {
    importCancelConfirm = true;
  }

  async function confirmCancelImport() {
    importCancelConfirm = false;
    await invoke("cancel_import", { connectionId: importConnId });
  }

  async function confirmImport() {
    importLoading = true;
    importProgressOpen = true;
    importProgressDone = 0;
    importProgressTotal = 0;

    const onEvent = new Channel<import("$lib/types").ImportProgress>();
    onEvent.onmessage = (progress) => {
      if (progress.type === "progress") {
        importProgressDone = progress.done;
        importProgressTotal = progress.total;
      } else if (progress.type === "done") {
        importProgressOpen = false;
        importCancelConfirm = false;
        importDialogOpen = false;
        toast.success(`${progress.count} statement(s) executed successfully`);
        refreshTables(importConnId);
      } else if (progress.type === "cancelled") {
        importProgressOpen = false;
        importCancelConfirm = false;
        toast.info("Import cancelled");
      } else if (progress.type === "error") {
        importProgressOpen = false;
        importCancelConfirm = false;
        importDialogOpen = false;
        toast.error(`Import failed: ${progress.message}`);
      }
    };

    try {
      await invoke("import_sql", {
        connectionId: importConnId,
        filePath: importFilePath,
        disableFkChecks: importDisableFkChecks,
        onEvent,
      });
    } catch {
      // Error already reported via the Channel onmessage handler above
      importProgressOpen = false;
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
        onToggleLock={() => handleToggleLock(conn)}
        onShowHistory={() => openQueryHistory(conn)}
        onServerInfo={() => openServerInfo(conn)}
        onExport={() => openExportDialog(conn)}
        onImport={() => importSql(conn.id)}
        onChangeDatabase={() => openChangeDatabase(conn)}
        onViewDiagram={() => openDiagram(conn)}
        onDelete={() => onDeleteConnection(conn)}
        onOpenTable={(table) =>
          openTableTab(conn.id, table, selectQuery(conn.driver, table))}
        onPreviewTable={(table) =>
          openTableTabPreview(conn.id, table, selectQuery(conn.driver, table))}
        onShowTableInfo={(table) => showTableInfo(conn.id, table)}
        onExportTable={(table) => exportTable(conn.id, table)}
        onImportTable={() => importSql(conn.id)}
        onDeleteTable={(table, kind) =>
          openTableAction("delete", conn.id, table, kind, conn.driver)}
        onTruncateTable={(table, kind) =>
          openTableAction("truncate", conn.id, table, kind, conn.driver)}
        onDropTable={(table, kind) =>
          openTableAction("drop", conn.id, table, kind, conn.driver)}
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

  <!-- Diagram Picker Dialog -->
  <DiagramPickerDialog
    bind:open={diagramPickerOpen}
    connectionId={diagramPickerConnId}
    initialSelected={diagramPickerInitialTables}
    onConfirm={(tables) => openDiagramTab(diagramPickerConnId, tables)}
  />

  <!-- Export Database Dialog -->
  <ExportDatabaseDialog
    open={exportDialogOpen}
    connectionId={exportDialogConnection?.id ?? ""}
    connectionName={exportDialogConnection?.name ?? ""}
    onConfirm={handleExportConfirm}
    onCancel={() => (exportDialogOpen = false)}
  />

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
    tableKind={tableActionTableKind}
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

  <!-- Unlock Confirmation Dialog -->
  <Dialog.Root bind:open={unlockDialogOpen}>
    <Dialog.Content class="sm:max-w-md">
      <Dialog.Header>
        <Dialog.Title>Unlock Connection</Dialog.Title>
        <Dialog.Description>
          This will allow write operations on <strong
            >{unlockDialogConnection?.name ?? "this connection"}</strong
          >. Are you sure you want to enable write access?
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button variant="outline" onclick={() => (unlockDialogOpen = false)}>
          Cancel
        </Button>
        <Button onclick={confirmUnlock}>Unlock</Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Query History Dialog -->
  <QueryHistoryDialog
    open={queryHistoryDialogOpen}
    connectionName={queryHistoryConnection?.name ?? ""}
    history={queryHistoryConnection?.queryHistory ?? []}
    onSelect={handleSelectHistoryQuery}
    onClose={() => (queryHistoryDialogOpen = false)}
  />

  <!-- Server Info Dialog -->
  <ServerInfoDialog
    connection={serverInfoConnection}
    open={serverInfoDialogOpen}
    onClose={() => (serverInfoDialogOpen = false)}
  />

  <!-- Export Progress Dialog -->
  <Dialog.Root bind:open={exportProgressOpen}>
    <Dialog.Content class="sm:max-w-md" interactOutsideBehavior="ignore">
      <Dialog.Header>
        <Dialog.Title>Exporting Database</Dialog.Title>
        <Dialog.Description>
          {exportProgressCurrent}
        </Dialog.Description>
      </Dialog.Header>
      <div class="py-4">
        <div class="w-full bg-muted rounded-full h-2 overflow-hidden">
          <div
            class="bg-primary h-full transition-all duration-300"
            style="width: {exportProgressTotal > 0
              ? (exportProgressIndex / exportProgressTotal) * 100
              : 0}%"
          ></div>
        </div>
        {#if exportProgressTotal > 0}
          <p class="text-xs text-muted-foreground text-center mt-2">
            {exportProgressIndex} / {exportProgressTotal} tables
          </p>
        {/if}
      </div>
    </Dialog.Content>
  </Dialog.Root>

  <!-- Import Progress Dialog -->
  <Dialog.Root bind:open={importProgressOpen}>
    <Dialog.Content
      class="sm:max-w-md"
      showCloseButton={false}
      interactOutsideBehavior="ignore"
      escapeKeydownBehavior="ignore"
    >
      <Dialog.Header>
        <Dialog.Title>Importing SQL</Dialog.Title>
        <Dialog.Description>
          {#if importCancelConfirm}
            Cancel the import? Changes made so far will be rolled back.
          {:else}
            Executing SQL statements...
          {/if}
        </Dialog.Description>
      </Dialog.Header>
      <div class="py-4">
        <div class="w-full bg-muted rounded-full h-2 overflow-hidden">
          <div
            class="bg-primary h-full transition-all duration-300"
            style="width: {importProgressTotal > 0
              ? (importProgressDone / importProgressTotal) * 100
              : 0}%"
          ></div>
        </div>
        {#if importProgressTotal > 0}
          <p class="text-xs text-muted-foreground text-center mt-2">
            {importProgressDone} / {importProgressTotal} statements
          </p>
        {/if}
      </div>
      <Dialog.Footer>
        {#if importCancelConfirm}
          <Button
            variant="outline"
            onclick={() => (importCancelConfirm = false)}>Keep importing</Button
          >
          <Button variant="destructive" onclick={confirmCancelImport}
            >Yes, cancel</Button
          >
        {:else}
          <Button variant="outline" onclick={requestCancelImport}>Cancel</Button
          >
        {/if}
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>

  {#if appVersion}
    <div
      class="px-3 h-8 border-t border-border shrink-0 flex items-center justify-between gap-2"
    >
      <p class="text-[10px] text-muted-foreground">v{appVersion}</p>
      {#if updateAvailable}
        <button
          class="text-[10px] text-primary hover:underline underline-offset-2 shrink-0"
          onclick={() => openUrl(RELEASES_URL)}
        >
          Update available
        </button>
      {/if}
    </div>
  {/if}
</aside>
