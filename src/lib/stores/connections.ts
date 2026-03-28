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
            return { ...conn, password: password || undefined };
          } catch {
            // Password not found in keyring (first time or deleted)
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
          await setPassword(KEYRING_SERVICE, passwordKey, conn.password);
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
    // Save tabs without results (too large and transient)
    const tabsWithoutResults = tabs.map((t) => ({
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
  // Set the active connection
  activeConnectionId.set(connectionId);
  
  // Check if tab for this table already exists
  let existingTab: QueryTab | undefined;
  queryTabs.subscribe((tabs) => {
    existingTab = tabs.find(
      (t) => t.connectionId === connectionId && t.title === tableName
    );
  })();

  if (existingTab) {
    // Switch to existing tab
    activeTabId.set(existingTab.id);
    return existingTab;
  }

  // Create new tab with table name as title
  return addTab(connectionId, sql, tableName);
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

export function updateTab(tabId: string, patch: Partial<QueryTab>) {
  queryTabs.update((tabs) =>
    tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t))
  );
}
