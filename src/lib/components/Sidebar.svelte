<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import type { Snippet } from "svelte";
    import {
        Plus,
        Database,
        ChevronRight,
        ChevronDown,
        Table,
        Loader,
        Pencil,
        Trash2,
        RefreshCw,
    } from "@lucide/svelte";
    import { Button } from "$lib/components/ui/button";
    import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
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
            loadingTables = new Set(
                [...loadingTables].filter((id) => id !== connId),
            );
        }
    }

    async function handleReconnect(conn: Connection) {
        reconnectingConnections = new Set([
            ...reconnectingConnections,
            conn.id,
        ]);
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

                <!-- Context menu -->
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
                    <DropdownMenu.Content class="min-w-32">
                        <DropdownMenu.Item
                            class="flex items-center gap-2 cursor-pointer"
                            onclick={() => onEditConnection(conn)}
                        >
                            <Pencil class="w-4 h-4" />
                            <span>Edit</span>
                        </DropdownMenu.Item>
                        <DropdownMenu.Item
                            class="flex items-center gap-2 cursor-pointer"
                            onclick={() => handleReconnect(conn)}
                            disabled={reconnectingConnections.has(conn.id)}
                        >
                            <RefreshCw
                                class="w-4 h-4 {reconnectingConnections.has(
                                    conn.id,
                                )
                                    ? 'animate-spin'
                                    : ''}"
                            />
                            <span
                                >{reconnectingConnections.has(conn.id)
                                    ? "Reconnecting..."
                                    : "Reconnect"}</span
                            >
                        </DropdownMenu.Item>
                        <DropdownMenu.Item
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
                                <path
                                    d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"
                                />
                                <path d="M21 3v5h-5" />
                            </svg>
                            <span>Refresh Tables</span>
                        </DropdownMenu.Item>
                        <DropdownMenu.Item
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
                                <path
                                    d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"
                                />
                            </svg>
                            <span>Rename</span>
                        </DropdownMenu.Item>
                        <DropdownMenu.Separator />
                        <DropdownMenu.Item
                            class="flex items-center gap-2 cursor-pointer text-destructive focus:text-destructive"
                            onclick={() => onDeleteConnection(conn)}
                        >
                            <Trash2 class="w-4 h-4" />
                            <span>Delete</span>
                        </DropdownMenu.Item>
                    </DropdownMenu.Content>
                </DropdownMenu.Root>
            </div>

            <!-- Table list -->
            {#if isExpanded}
                <div class="ml-4 mb-1">
                    {#if tables.length === 0 && !isLoading}
                        <p
                            class="px-2 py-1 text-xs text-muted-foreground italic"
                        >
                            No tables found
                        </p>
                    {/if}
                    {#each tables as table}
                        <button
                            class="w-full flex items-center gap-2 px-2 py-1 rounded text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground transition-colors text-left"
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
</aside>
