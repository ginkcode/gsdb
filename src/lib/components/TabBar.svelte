<script lang="ts">
    import { X } from "@lucide/svelte";
    import {
        queryTabs,
        activeTabId,
        closeTab,
        connections,
    } from "$lib/stores/connections";

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
</script>

<div
    class="flex items-stretch border-b border-border bg-background shrink-0 overflow-x-auto"
>
    {#each $queryTabs as tab}
        {@const conn = $connections.find((c) => c.id === tab.connectionId)}
        <div
            role="tab"
            aria-selected={$activeTabId === tab.id}
            tabindex="0"
            class="group flex items-center gap-2 px-4 py-2.5 text-sm border-r border-border transition-colors shrink-0 cursor-pointer select-none
            {$activeTabId === tab.id
                ? 'bg-muted text-foreground border-b-2 border-b-primary -mb-px'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
            onclick={() => activeTabId.set(tab.id)}
            onkeydown={(e) => e.key === "Enter" && activeTabId.set(tab.id)}
        >
            {#if conn}
                <span
                    class="text-[9px] font-bold px-1 py-0.5 rounded border {driverColors[
                        conn.driver
                    ]}"
                >
                    {driverLabel[conn.driver]}
                </span>
            {/if}
            <span>{tab.title}</span>
            {#if tab.isLoading}
                <span class="w-1.5 h-1.5 rounded-full bg-primary animate-pulse"
                ></span>
            {/if}
            <button
                class="ml-1 rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-muted-foreground/20 transition-all"
                onclick={(e) => {
                    e.stopPropagation();
                    closeTab(tab.id);
                }}
            >
                <X class="w-3 h-3" />
            </button>
        </div>
    {/each}
</div>
