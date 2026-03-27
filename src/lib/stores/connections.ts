import { writable, derived } from "svelte/store";
import type { Connection, QueryTab } from "../types";

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

export function addTab(connectionId: string, sql = ""): QueryTab {
  const tab: QueryTab = {
    id: crypto.randomUUID(),
    connectionId,
    title: "Query",
    sql,
    isLoading: false,
  };
  queryTabs.update((tabs) => [...tabs, tab]);
  activeTabId.set(tab.id);
  return tab;
}

export function closeTab(tabId: string) {
  queryTabs.update((tabs) => tabs.filter((t) => t.id !== tabId));
  activeTabId.update((id) => (id === tabId ? null : id));
}

export function updateTab(tabId: string, patch: Partial<QueryTab>) {
  queryTabs.update((tabs) =>
    tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t))
  );
}
