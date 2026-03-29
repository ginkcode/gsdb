<script lang="ts">
  import { X, SquareX, PanelLeftClose, PanelRightClose } from "@lucide/svelte";
  import {
    queryTabs,
    activeTabId,
    closeTab,
    connections,
  } from "$lib/stores/connections";
  import * as ContextMenu from "$lib/components/ui/context-menu";

  const colorClasses: Record<string, string> = {
    blue: "bg-blue-500/15 text-blue-400 border-blue-500/20",
    green: "bg-green-500/15 text-green-400 border-green-500/20",
    orange: "bg-orange-500/15 text-orange-400 border-orange-500/20",
    purple: "bg-purple-500/15 text-purple-400 border-purple-500/20",
    red: "bg-red-500/15 text-red-400 border-red-500/20",
    yellow: "bg-yellow-500/15 text-yellow-400 border-yellow-500/20",
    pink: "bg-pink-500/15 text-pink-400 border-pink-500/20",
    cyan: "bg-cyan-500/15 text-cyan-400 border-cyan-500/20",
  };

  const driverLabel: Record<string, string> = {
    postgres: "PG",
    mysql: "MY",
    sqlite: "SQ",
  };

  function getLabelColor(conn: { driver: string; color?: string }): string {
    return colorClasses[conn.color ?? "blue"] ?? colorClasses.blue;
  }

  function closeOtherTabs(keepTabId: string) {
    const tabsToClose = $queryTabs
      .filter((t) => t.id !== keepTabId)
      .map((t) => t.id);
    for (const tabId of tabsToClose) {
      closeTab(tabId);
    }
  }

  function closeTabsToRight(tabId: string) {
    const tabIndex = $queryTabs.findIndex((t) => t.id === tabId);
    if (tabIndex === -1) return;
    const tabsToClose = $queryTabs.slice(tabIndex + 1).map((t) => t.id);
    for (const id of tabsToClose) {
      closeTab(id);
    }
  }

  function closeTabsToLeft(tabId: string) {
    const tabIndex = $queryTabs.findIndex((t) => t.id === tabId);
    if (tabIndex === -1) return;
    const tabsToClose = $queryTabs.slice(0, tabIndex).map((t) => t.id);
    for (const id of tabsToClose) {
      closeTab(id);
    }
  }
</script>

<div
  class="flex items-stretch border-b border-border bg-background shrink-0 overflow-x-auto overflow-y-hidden scrollbar-hide"
>
  {#each $queryTabs as tab}
    {@const conn = $connections.find((c) => c.id === tab.connectionId)}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        <button
          type="button"
          role="tab"
          aria-selected={$activeTabId === tab.id}
          tabindex="0"
          class="group flex items-center gap-2 px-4 py-2.5 text-sm border-r border-border transition-colors shrink-0 cursor-pointer select-none whitespace-nowrap h-full
            {$activeTabId === tab.id
            ? 'bg-muted text-foreground border-b-2 border-b-primary -mb-px'
            : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
          onclick={() => activeTabId.set(tab.id)}
          onkeydown={(e) => e.key === "Enter" && activeTabId.set(tab.id)}
        >
          {#if conn}
            <span
              class="text-[9px] font-bold px-1 py-0.5 rounded border {getLabelColor(
                conn,
              )}"
            >
              {driverLabel[conn.driver]}
            </span>
          {/if}
          <span>{tab.title}</span>
          {#if tab.isLoading}
            <span class="w-1.5 h-1.5 rounded-full bg-primary animate-pulse"
            ></span>
          {/if}
          <span
            role="button"
            tabindex="-1"
            class="ml-1 rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
            onclick={(e) => {
              e.stopPropagation();
              closeTab(tab.id);
            }}
            onkeydown={(e) => e.key === "Enter" && e.stopPropagation()}
          >
            <X class="w-3 h-3" />
          </span>
        </button>
      </ContextMenu.Trigger>
      <ContextMenu.Content>
        <ContextMenu.Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => closeTab(tab.id)}
        >
          <X class="w-4 h-4" />
          <span>Close</span>
        </ContextMenu.Item>
        <ContextMenu.Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => closeOtherTabs(tab.id)}
        >
          <SquareX class="w-4 h-4" />
          <span>Close other tabs</span>
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => closeTabsToLeft(tab.id)}
        >
          <PanelLeftClose class="w-4 h-4" />
          <span>Close to the left</span>
        </ContextMenu.Item>
        <ContextMenu.Item
          class="flex items-center gap-2 cursor-pointer"
          onclick={() => closeTabsToRight(tab.id)}
        >
          <PanelRightClose class="w-4 h-4" />
          <span>Close to the right</span>
        </ContextMenu.Item>
      </ContextMenu.Content>
    </ContextMenu.Root>
  {/each}
</div>
