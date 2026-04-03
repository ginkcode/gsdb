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

// ── Keyring helpers ───────────────────────────────────────────────────────────
// Only SHORT secrets go into the OS keyring. All backends (including Windows
// Credential Manager which caps at ~1024 bytes) can handle these safely.
// The SSH private key is large and is encrypted with AES-256-GCM instead:
//   • Encryption key (32 bytes → 44-char base64) → keyring  ✓ fits everywhere
//   • Ciphertext (arbitrary size)                 → Tauri store file

function keyDbPassword(id: string)    { return `connection-${id}`; }
function keySshPassword(id: string)   { return `connection-${id}-ssh-password`; }
function keySshPassphrase(id: string) { return `connection-${id}-ssh-passphrase`; }
const SSH_ENC_KEY_ACCOUNT             = "gsdb-ssh-encryption-key";

async function getSecret(key: string): Promise<string | undefined> {
  try { return (await getPassword(KEYRING_SERVICE, key)) || undefined; } catch { return undefined; }
}
async function setOrDelete(key: string, value: string | undefined): Promise<void> {
  try {
    if (value) { await setPassword(KEYRING_SERVICE, key, value); }
    else { await deletePassword(KEYRING_SERVICE, key).catch(() => {}); }
  } catch (err) { console.error(`[Keyring] Failed for key ${key}:`, err); }
}

// ── AES-256-GCM encryption via the Web Crypto API ────────────────────────────
// crypto.subtle is available in Tauri's WebKit / WebView2 on all platforms.

function _b64(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes));
}
function _bytes(b64: string): Uint8Array {
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}

// Single promise ensures the key is generated only once even under concurrent calls.
let _encKeyPromise: Promise<CryptoKey | null> | null = null;
function _getEncKey(): Promise<CryptoKey | null> {
  if (!_encKeyPromise) _encKeyPromise = _loadOrCreateEncKey();
  return _encKeyPromise;
}
async function _loadOrCreateEncKey(): Promise<CryptoKey | null> {
  try {
    const stored = await getSecret(SSH_ENC_KEY_ACCOUNT);
    if (stored) {
      return await crypto.subtle.importKey(
        "raw", _bytes(stored), { name: "AES-GCM" }, false, ["encrypt", "decrypt"]
      );
    }
    const key = await crypto.subtle.generateKey(
      { name: "AES-GCM", length: 256 }, true, ["encrypt", "decrypt"]
    );
    const raw = new Uint8Array(await crypto.subtle.exportKey("raw", key));
    await setOrDelete(SSH_ENC_KEY_ACCOUNT, _b64(raw));
    return key;
  } catch (err) {
    console.error("[Crypto] Failed to initialise SSH encryption key:", err);
    return null;
  }
}

async function _encrypt(plaintext: string): Promise<string> {
  const key = await _getEncKey();
  if (!key) throw new Error("Encryption key unavailable");
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const data = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv }, key, new TextEncoder().encode(plaintext)
  );
  return JSON.stringify({ iv: _b64(iv), d: _b64(new Uint8Array(data)) });
}

async function _decrypt(ciphertext: string): Promise<string | undefined> {
  try {
    const { iv, d } = JSON.parse(ciphertext);
    const key = await _getEncKey();
    if (!key) return undefined;
    const data = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: _bytes(iv) }, key, _bytes(d)
    );
    return new TextDecoder().decode(data);
  } catch { return undefined; }
}

// Save the encrypted SSH private key to the Tauri store file (not keyring —
// keys can be kilobytes, exceeding keyring size limits on some platforms).
async function saveEncryptedSshKey(conn: Connection): Promise<void> {
  const s = await getStore();
  const k = `ssh-key-${conn.id}`;
  if (conn.ssh?.privateKey) {
    try { await s.set(k, await _encrypt(conn.ssh.privateKey)); }
    catch (err) { console.error(`[Crypto] Failed to save SSH key for ${conn.id}:`, err); }
  } else {
    await s.delete(k).catch(() => {});
  }
  await s.save();
}

async function loadEncryptedSshKey(connId: string): Promise<string | undefined> {
  try {
    const s = await getStore();
    const ct = await s.get<string>(`ssh-key-${connId}`);
    return ct ? await _decrypt(ct) : undefined;
  } catch { return undefined; }
}

// Load saved connections from store and retrieve passwords from keyring
export async function loadSavedConnections(): Promise<Connection[]> {
  try {
    const s = await getStore();
    const saved = await s.get<Connection[]>(CONNECTIONS_STORE_KEY);
    console.log("[Connections] Loaded from store:", saved);
    
    if (saved && saved.length > 0) {
      // Retrieve all secrets from keyring
      const connectionsWithPasswords = await Promise.all(
        saved.map((conn) => loadConnectionSecrets(conn))
      );

      connections.set(connectionsWithPasswords);
      console.log("[Connections] Restored connections:", connectionsWithPasswords.length);
      
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

// Save connection metadata to disk only — never touches keyring
async function saveConnectionMetadata(conns: Connection[]): Promise<void> {
  try {
    const s = await getStore();
    const stripped = conns.map((conn) => ({
      ...conn,
      password: undefined,
      ssh: conn.ssh ? { ...conn.ssh, password: undefined, privateKey: undefined, privateKeyPassphrase: undefined } : undefined,
    }));
    await s.set(CONNECTIONS_STORE_KEY, stripped);
    await s.save();
  } catch (err) {
    console.error("[Connections] Failed to save metadata:", err);
  }
}

// Save all secrets for a single connection
async function saveConnectionSecrets(conn: Connection): Promise<void> {
  // Short secrets → keyring
  await setOrDelete(keyDbPassword(conn.id), conn.password);
  await setOrDelete(keySshPassword(conn.id), conn.ssh?.password);
  await setOrDelete(keySshPassphrase(conn.id), conn.ssh?.privateKeyPassphrase);
  // SSH private key → AES-256-GCM in store file (too large for keyring on some platforms)
  await saveEncryptedSshKey(conn);
}

// Remove all secrets for a single connection
async function deleteConnectionSecrets(connId: string): Promise<void> {
  await setOrDelete(keyDbPassword(connId), undefined);
  await setOrDelete(keySshPassword(connId), undefined);
  await setOrDelete(keySshPassphrase(connId), undefined);
  // Remove encrypted key from store
  const s = await getStore();
  await s.delete(`ssh-key-${connId}`).catch(() => {});
  await s.save();
}

// Load all secrets for a single connection
async function loadConnectionSecrets(conn: Connection): Promise<Connection> {
  const password = await getSecret(keyDbPassword(conn.id));
  const sshPassword = await getSecret(keySshPassword(conn.id));
  const sshPassphrase = await getSecret(keySshPassphrase(conn.id));
  const sshKey = await loadEncryptedSshKey(conn.id);
  return {
    ...conn,
    password,
    ssh: conn.ssh ? { ...conn.ssh, password: sshPassword, privateKey: sshKey, privateKeyPassphrase: sshPassphrase } : conn.ssh,
  };
}

// Add a new connection and persist it
export async function addConnection(conn: Connection): Promise<void> {
  connections.update((conns) => {
    const updated = [...conns, conn];
    saveConnectionMetadata(updated);
    return updated;
  });
  await saveConnectionSecrets(conn);
}

// Remove a connection and its password
export async function removeConnection(connId: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.filter((c) => c.id !== connId);
    saveConnectionMetadata(updated);
    return updated;
  });
  await deleteConnectionSecrets(connId);
}

// Rename a connection
export async function renameConnection(connId: string, newName: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.map((c) =>
      c.id === connId ? { ...c, name: newName } : c
    );
    saveConnectionMetadata(updated);
    return updated;
  });
}

// Toggle connection lock state
export async function toggleConnectionLock(connId: string): Promise<void> {
  connections.update((conns) => {
    const updated = conns.map((c) =>
      c.id === connId ? { ...c, locked: !c.locked } : c
    );
    saveConnectionMetadata(updated);
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
    saveConnectionMetadata(updated);
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
      saveConnectionMetadata(updated);
      return updated;
    });
    await saveConnectionSecrets(conn);
    
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
