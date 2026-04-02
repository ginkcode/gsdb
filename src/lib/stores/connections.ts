import { writable, derived } from "svelte/store";
import { Store } from "@tauri-apps/plugin-store";
import { invoke } from "@tauri-apps/api/core";
import { getPassword, setPassword, deletePassword } from "tauri-plugin-keyring-api";
import type { Connection, QueryTab } from "../types";

const CONNECTIONS_STORE_KEY = "saved-connections";
const TABS_STORE_KEY = "saved-tabs";
const ACTIVE_TAB_STORE_KEY = "active-tab";
const ACTIVE_CONNECTION_STORE_KEY = "active-connection";
const KEYRING_SERVICE = "gsdb";

export const connections = writable<Connection[]>([]);
export const activeConnectionId = writable<string | null>(null);
export const queryTabs = writable<QueryTab[]>([]);
export const activeTabId = writable<string | null>(null);

export const activeConnection = derived(
  [connections, activeConnectionId],
  ([$connections, $activeConnectionId]) =>
    $connections.find((c) => c.id === $activeConnectionId) ?? null
);

export const activeTab = derived(
  [queryTabs, activeTabId],
  ([$queryTabs, $activeTabId]) =>
    $queryTabs.find((t) => t.id === $activeTabId) ?? null
);

// Store instance (lazy loaded)
let store: Store | null = null;

async function getStore(): Promise<Store> {
  if (!store) {
    store = await Store.load("connections.json");
  }
  return store;
}

// Generate a unique key for storing password in keyring
function getPasswordKey(connectionId: string): string {
  return `connection-${connectionId}`;
}

// Load saved connections from store and retrieve passwords from keyring
export async function loadSavedConnections(): Promise<Connection[]> {
  try {
    const s = await getStore();
    const saved = await s.get<Connection[]>(CONNECTIONS_STORE_KEY);
    console.log("[Connections] Loaded from store:", saved);
    
    if (saved && saved.length > 0) {
      // Retrieve passwords from keyring
      const connectionsWithPasswords = await Promise.all(
        saved.map(async (conn) => {
          try {
            const password = await getPassword(KEYRING_SERVICE, getPasswordKey(conn.id));
            console.log(`[Connections] Retrieved password for ${conn.name}:`, password ? 'found' : 'empty');
            return { ...conn, password: password || undefined };
          } catch (err) {
            // Password not found in keyring (first time or deleted)
            // On Linux, this can also happen if keyring service is not available
            console.warn(`[Connections] Could not retrieve password for ${conn.name}:`, err);
            return { ...conn, password: undefined };
          }
        })
      );
      
      connections.set(connectionsWithPasswords);
      console.log("[Connections] Restored connections:", connectionsWithPasswords);
      
      // Reconnect each connection to the backend
      for (const conn of connectionsWithPasswords) {
        try {
          await invoke("add_connection", { connection: conn });
          console.log(`[Connections] Reconnected: ${conn.name}`);
        } catch (err) {
          console.error(`[Connections] Failed to reconnect ${conn.name}:`, err);
        }
      }
      
      // Load saved tabs
      const { tabs, activeTabId: savedActiveTabId, activeConnectionId: savedActiveConnectionId } = await loadTabs();
      if (tabs.length > 0) {
        queryTabs.set(tabs);
        if (savedActiveTabId) {
          activeTabId.set(savedActiveTabId);
        }
        if (savedActiveConnectionId) {
          activeConnectionId.set(savedActiveConnectionId);
        }
      }
      
      // Setup tab persistence
      setupTabPersistence();
      
      return connectionsWithPasswords;
    }
  } catch (err) {
    console.error("[Connections] Failed to load saved connections:", err);
  }
  // Setup tab persistence even if no connections
  setupTabPersistence();
  return [];
}

// Save connections to store and passwords to keyring
export async function saveConnections(conns: Connection[]): Promise<void> {
  try {
    const s = await getStore();
    
    // Save connection metadata (without passwords) to store
    const connectionsWithoutPasswords = conns.map((conn) => ({
      ...conn,
      password: undefined,
    }));
    
    console.log("[Connections] Saving:", connectionsWithoutPasswords);
    await s.set(CONNECTIONS_STORE_KEY, connectionsWithoutPasswords);
    await s.save();
    
    // Save passwords to keyring
    for (const conn of conns) {
      const passwordKey = getPasswordKey(conn.id);
      if (conn.password) {
        try {
          console.log(`[Connections] Saving password for ${conn.id}...`);
          await setPassword(KEYRING_SERVICE, passwordKey, conn.password);
          console.log(`[Connections] Password saved for ${conn.id}`);
        } catch (err) {
          console.error(`[Connections] Failed to save password for ${conn.id}:`, err);
        }
      } else {
        // Remove password from keyring if it was cleared
        try {
          await deletePassword(KEYRING_SERVICE, passwordKey);
        } catch {
          // Password might not exist, ignore
        }
      }
    }
    
    console.log("[Connections] Saved successfully");
  } catch (err) {
    console.error("[Connections] Failed to save connections:", err);
  }
}

// Add a new connection and persist it
export async function addConnection(conn: Connection): Promise<void> {
  connections.update((conns) => {
    const updated = [...conns, conn];
    saveConnections(updated);
    return updated;
  });
}

// Remove a connection and its password
export async function removeConnection(connId: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.filter((c) => c.id !== connId);
    saveConnections(updated);
    return updated;
  });
  
  // Also remove password from keyring
  try {
    await deletePassword(KEYRING_SERVICE, getPasswordKey(connId));
  } catch {
    // Password might not exist, ignore
  }
}

// Rename a connection
export async function renameConnection(connId: string, newName: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.map((c) => 
      c.id === connId ? { ...c, name: newName } : c
    );
    saveConnections(updated);
    return updated;
  });
}

// Toggle connection lock state
export async function toggleConnectionLock(connId: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.map((c) => 
      c.id === connId ? { ...c, locked: !c.locked } : c
    );
    saveConnections(updated);
    return updated;
  });
}

// Add a query to the connection's history (keeps last 1000 entries)
export function addQueryHistory(
  connId: string,
  sql: string,
  success: boolean,
  error?: string,
  rowsAffected?: number
): void {
  const MAX_HISTORY = 1000;
  const entry: import("$lib/types").QueryHistoryEntry = {
    sql,
    timestamp: new Date().toISOString(),
    success,
    error,
    rowsAffected,
  };
  
  connections.update((conns) => {
    const updated = conns.map((c) => {
      if (c.id !== connId) return c;
      const history = [...(c.queryHistory ?? []), entry];
      // Keep only the last MAX_HISTORY entries
      const trimmed = history.slice(-MAX_HISTORY);
      return { ...c, queryHistory: trimmed };
    });
    saveConnections(updated);
    return updated;
  });
}

// Update an existing connection (reconnects to backend)
export async function updateConnection(conn: Connection): Promise<void> {
  try {
    // Reconnect to the backend with updated settings
    await invoke("add_connection", { connection: conn });
    
    // Update the store
    connections.update((conns) => {
      const updated = conns.map((c) => 
        c.id === conn.id ? conn : c
      );
      saveConnections(updated);
      return updated;
    });
    
    console.log(`[Connections] Updated: ${conn.name}`);
  } catch (err) {
    console.error(`[Connections] Failed to update ${conn.name}:`, err);
    throw err;
  }
}

// Reconnect a connection (useful when connection drops)
export async function reconnectConnection(connId: string): Promise<void> {
  try {
    await invoke("reconnect_connection", { connectionId: connId });
    console.log(`[Connections] Reconnected: ${connId}`);
  } catch (err) {
    console.error(`[Connections] Failed to reconnect ${connId}:`, err);
    throw err;
  }
}

// Save tabs to store
async function saveTabs(tabs: QueryTab[], activeId: string | null, activeConnId: string | null): Promise<void> {
  try {
    const s = await getStore();
    // Save tabs without results (too large and transient); drop temporary preview tabs
    const tabsWithoutResults = tabs
      .filter((t) => !t.temporary)
      .map((t) => ({
        ...t,
        result: undefined,
        isLoading: false,
      }));
    await s.set(TABS_STORE_KEY, tabsWithoutResults);
    await s.set(ACTIVE_TAB_STORE_KEY, activeId);
    await s.set(ACTIVE_CONNECTION_STORE_KEY, activeConnId);
    await s.save();
    console.log("[Tabs] Saved:", tabsWithoutResults.length, "tabs");
  } catch (err) {
    console.error("[Tabs] Failed to save tabs:", err);
  }
}

// Load tabs from store
async function loadTabs(): Promise<{ tabs: QueryTab[]; activeTabId: string | null; activeConnectionId: string | null }> {
  try {
    const s = await getStore();
    const savedTabs = await s.get<QueryTab[]>(TABS_STORE_KEY);
    const savedActiveTabId = await s.get<string>(ACTIVE_TAB_STORE_KEY);
    const savedActiveConnectionId = await s.get<string>(ACTIVE_CONNECTION_STORE_KEY);
    
    if (savedTabs && savedTabs.length > 0) {
      console.log("[Tabs] Loaded:", savedTabs.length, "tabs");
      return {
        tabs: savedTabs,
        activeTabId: savedActiveTabId || null,
        activeConnectionId: savedActiveConnectionId || null,
      };
    }
  } catch (err) {
    console.error("[Tabs] Failed to load tabs:", err);
  }
  return { tabs: [], activeTabId: null, activeConnectionId: null };
}

// Subscribe to tab changes and save them
export function setupTabPersistence(): void {
  queryTabs.subscribe((tabs) => {
    let currentActiveTabId: string | null = null;
    let currentActiveConnectionId: string | null = null;
    activeTabId.subscribe((id) => { currentActiveTabId = id; })();
    activeConnectionId.subscribe((id) => { currentActiveConnectionId = id; })();
    saveTabs(tabs, currentActiveTabId, currentActiveConnectionId);
  });
}

export function addTab(connectionId: string, sql = "", title = "Query"): QueryTab {
  const tab: QueryTab = {
    id: crypto.randomUUID(),
    connectionId,
    title,
    sql,
    isLoading: false,
  };
  queryTabs.update((tabs) => [...tabs, tab]);
  activeTabId.set(tab.id);
  return tab;
}

export function findTabByTitle(connectionId: string, title: string): QueryTab | undefined {
  let found: QueryTab | undefined;
  queryTabs.subscribe((tabs) => {
    found = tabs.find((t) => t.connectionId === connectionId && t.title === title);
  })();
  return found;
}

export function openTableTab(connectionId: string, tableName: string, sql: string): QueryTab {
  activeConnectionId.set(connectionId);

  let tabs: QueryTab[] = [];
  queryTabs.subscribe((t) => { tabs = t; })();

  // If there's an existing permanent tab for this table, just activate it
  const existingTab = tabs.find(
    (t) => t.connectionId === connectionId && t.title === tableName && !t.temporary
  );
  if (existingTab) {
    activeTabId.set(existingTab.id);
    return existingTab;
  }

  // Promote the temporary tab for this table to permanent (double-clicked)
  const tempTab = tabs.find(
    (t) => t.connectionId === connectionId && t.title === tableName && t.temporary
  );
  if (tempTab) {
    queryTabs.update((ts) =>
      ts.map((t) => (t.id === tempTab.id ? { ...t, temporary: false } : t))
    );
    activeTabId.set(tempTab.id);
    return { ...tempTab, temporary: false };
  }

  // Remove any existing temporary tab before opening a new permanent one
  queryTabs.update((ts) => ts.filter((t) => !t.temporary));

  const tab: QueryTab = {
    id: crypto.randomUUID(),
    connectionId,
    title: tableName,
    sql,
    isLoading: false,
    temporary: false,
    autoRun: true,
  };
  queryTabs.update((ts) => [...ts, tab]);
  activeTabId.set(tab.id);
  return tab;
}

export function openTableTabPreview(connectionId: string, tableName: string, sql: string): QueryTab {
  activeConnectionId.set(connectionId);

  let tabs: QueryTab[] = [];
  queryTabs.subscribe((t) => { tabs = t; })();

  // If a permanent tab for this table already exists, just activate it
  const existingPermanent = tabs.find(
    (t) => t.connectionId === connectionId && t.title === tableName && !t.temporary
  );
  if (existingPermanent) {
    activeTabId.set(existingPermanent.id);
    return existingPermanent;
  }

  // If the temporary tab is already showing this table, just activate it
  const existingTemp = tabs.find(
    (t) => t.connectionId === connectionId && t.title === tableName && t.temporary
  );
  if (existingTemp) {
    activeTabId.set(existingTemp.id);
    return existingTemp;
  }

  // Replace any existing temporary tab with one for the new table
  const tab: QueryTab = {
    id: crypto.randomUUID(),
    connectionId,
    title: tableName,
    sql,
    isLoading: false,
    temporary: true,
    autoRun: true,
  };
  queryTabs.update((ts) => {
    const withoutTemp = ts.filter((t) => !t.temporary);
    return [...withoutTemp, tab];
  });
  activeTabId.set(tab.id);
  return tab;
}

export function closeTab(tabId: string) {
  let tabsBeforeClose: QueryTab[] = [];
  let currentActiveId: string | null = null;
  
  // Get current state
  queryTabs.subscribe((tabs) => {
    tabsBeforeClose = tabs;
  })();
  activeTabId.subscribe((id) => {
    currentActiveId = id;
  })();
  
  const currentIndex = tabsBeforeClose.findIndex((t) => t.id === tabId);
  const remainingTabs = tabsBeforeClose.filter((t) => t.id !== tabId);
  
  queryTabs.set(remainingTabs);
  
  // If closing the active tab, switch to adjacent tab
  if (currentActiveId === tabId && remainingTabs.length > 0) {
    // Try to activate the next tab (right side), or fall back to previous (left side)
    const nextTab = currentIndex < remainingTabs.length 
      ? remainingTabs[currentIndex] 
      : remainingTabs[remainingTabs.length - 1];
    activeTabId.set(nextTab.id);
  } else if (remainingTabs.length === 0) {
    activeTabId.set(null);
  }
}

export function closeTabsByConnection(connectionId: string) {
  let currentActiveId: string | null = null;
  activeTabId.subscribe((id) => { currentActiveId = id; })();

  queryTabs.update((tabs) => {
    const remaining = tabs.filter((t) => t.connectionId !== connectionId);
    // If active tab was removed, switch to last remaining or null
    if (currentActiveId && !remaining.find((t) => t.id === currentActiveId)) {
      activeTabId.set(remaining.length > 0 ? remaining[remaining.length - 1].id : null);
    }
    return remaining;
  });
}

export function updateTab(tabId: string, patch: Partial<QueryTab>) {
  queryTabs.update((tabs) =>
    tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t))
  );
}

export function makeTabPermanent(tabId: string) {
  queryTabs.update((tabs) =>
    tabs.map((t) => (t.id === tabId ? { ...t, temporary: false } : t))
  );
}

export function openDiagramTab(
  connectionId: string,
  selectedTables: string[]
): QueryTab {
  activeConnectionId.set(connectionId);

  let tabs: QueryTab[] = [];
  queryTabs.subscribe((t) => { tabs = t; })();

  // Reuse existing diagram tab for this connection if one exists
  const existing = tabs.find(
    (t) => t.connectionId === connectionId && t.kind === "diagram"
  );
  if (existing) {
    queryTabs.update((ts) =>
      ts.map((t) =>
        t.id === existing.id ? { ...t, selectedTables } : t
      )
    );
    activeTabId.set(existing.id);
    return { ...existing, selectedTables };
  }

  const tab: QueryTab = {
    id: crypto.randomUUID(),
    connectionId,
    title: "Diagram",
    kind: "diagram",
    sql: "",
    isLoading: false,
    selectedTables,
    nodePositions: {},
  };
  queryTabs.update((ts) => [...ts, tab]);
  activeTabId.set(tab.id);
  return tab;
}
