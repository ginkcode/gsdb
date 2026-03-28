<script lang="ts">
    import { save } from "@tauri-apps/plugin-dialog";
    import { writeFile } from "@tauri-apps/plugin-fs";
    import { downloadDir } from "@tauri-apps/api/path";
    import { ScrollArea } from "$lib/components/ui/scroll-area";
    import { Download } from "@lucide/svelte";
    import { Button } from "$lib/components/ui/button";
    import type { QueryResult } from "$lib/types";

    let {
        result,
        selectedRow,
        onRowSelect,
    }: {
        result: QueryResult;
        selectedRow: Record<string, unknown> | null;
        onRowSelect: (row: Record<string, unknown>) => void;
    } = $props();

    function formatValue(value: unknown): string {
        if (value === null || value === undefined) {
            return "NULL";
        }
        if (typeof value === "object") {
            return JSON.stringify(value);
        }
        return String(value);
    }

    const MAX_DISPLAY_LENGTH = 100;

    function truncateText(text: string): {
        display: string;
        full: string;
        truncated: boolean;
    } {
        if (text.length <= MAX_DISPLAY_LENGTH) {
            return { display: text, full: text, truncated: false };
        }
        return {
            display: text.slice(0, MAX_DISPLAY_LENGTH) + "…",
            full: text,
            truncated: true,
        };
    }

    function isRowSelected(row: Record<string, unknown>): boolean {
        if (!selectedRow) return false;
        return result.columns.every((col) => row[col] === selectedRow[col]);
    }

    function escapeCSVField(value: string): string {
        if (
            value.includes(",") ||
            value.includes('"') ||
            value.includes("\n")
        ) {
            return `"${value.replace(/"/g, '""')}"`;
        }
        return value;
    }

    async function exportToCSV() {
        const headers = result.columns.map(escapeCSVField).join(",");
        const rows = result.rows.map((row) =>
            result.columns
                .map((col) => escapeCSVField(formatValue(row[col])))
                .join(","),
        );
        const csv = [headers, ...rows].join("\n");
        const encoder = new TextEncoder();
        const csvBytes = encoder.encode(csv);

        const defaultName = `query-results-${new Date().toISOString().slice(0, 10)}.csv`;
        const downloadsPath = await downloadDir();

        try {
            const filePath = await save({
                defaultPath: downloadsPath
                    ? `${downloadsPath}/${defaultName}`
                    : defaultName,
                filters: [
                    {
                        name: "CSV",
                        extensions: ["csv"],
                    },
                ],
            });

            if (filePath) {
                await writeFile(filePath, csvBytes);
            }
        } catch (err) {
            console.error("Failed to save CSV:", err);
        }
    }
</script>

{#if result.error}
    <div class="p-4 font-mono text-sm text-destructive whitespace-pre-wrap">
        {result.error}
    </div>
{:else if result.columns.length === 0}
    <div
        class="flex items-center justify-center h-full text-sm text-muted-foreground"
    >
        Query executed successfully. No rows returned.
    </div>
{:else}
    <div class="flex flex-col h-full overflow-hidden">
        <ScrollArea class="flex-1 h-0" orientation="both">
            <table class="text-sm border-collapse font-mono">
                <thead class="sticky top-0 z-10">
                    <tr>
                        {#each result.columns as col}
                            <th
                                class="px-3 py-2 text-left font-semibold text-xs text-muted-foreground uppercase tracking-wider bg-muted border-b border-border whitespace-nowrap"
                            >
                                {col}
                            </th>
                        {/each}
                    </tr>
                </thead>
                <tbody>
                    {#each result.rows as row, i}
                        <tr
                            class="border-b border-border/50 hover:bg-muted/40 transition-colors cursor-pointer {isRowSelected(
                                row,
                            )
                                ? 'bg-accent'
                                : ''}"
                            onclick={() => onRowSelect(row)}
                        >
                            {#each result.columns as col}
                                {@const value = formatValue(row[col])}
                                {@const truncated = truncateText(value)}
                                <td
                                    class="px-3 py-1.5 whitespace-nowrap text-foreground/90"
                                >
                                    {#if row[col] === null || row[col] === undefined}
                                        <span
                                            class="text-muted-foreground italic text-xs"
                                            >NULL</span
                                        >
                                    {:else if truncated.truncated}
                                        <span
                                            title={truncated.full}
                                            class="cursor-default"
                                            >{truncated.display}</span
                                        >
                                    {:else}
                                        {truncated.display}
                                    {/if}
                                </td>
                            {/each}
                        </tr>
                    {/each}
                </tbody>
            </table>
        </ScrollArea>
        <div
            class="flex items-center justify-between px-3 py-1.5 text-xs text-muted-foreground border-t border-border shrink-0"
        >
            <span>
                {result.rows.length}
                {result.rows.length === 1 ? "row" : "rows"}
            </span>
            <Button
                variant="ghost"
                size="sm"
                class="h-6 gap-1.5 text-xs"
                onclick={exportToCSV}
            >
                <Download class="w-3 h-3" />
                Export CSV
            </Button>
        </div>
    </div>
{/if}
