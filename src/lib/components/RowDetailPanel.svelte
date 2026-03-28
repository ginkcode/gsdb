<script lang="ts">
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { X, Copy, Check, Save } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { invoke } from "@tauri-apps/api/core";
  import { untrack } from "svelte";
  import { toast } from "svelte-sonner";

  let {
    row,
    columns,
    connectionId,
    tableName,
    onClose,
    onUpdateSuccess,
  }: {
    row: Record<string, unknown> | null;
    columns: string[];
    connectionId?: string;
    tableName?: string;
    onClose: () => void;
    onUpdateSuccess?: () => void;
  } = $props();

  let copiedColumn = $state<string | null>(null);
  let baseRow = $state<Record<string, unknown> | null>(null);
  let editedValues = $state<Record<string, string>>({});
  let isUpdating = $state(false);

  // Reset only when the selected row changes (different row clicked).
  // columns is read inside untrack so a query refresh doesn't re-trigger this
  // and wipe the user's edits / post-save state.
  $effect(() => {
    const currentRow = row;
    untrack(() => {
      baseRow = currentRow ? { ...currentRow } : null;
      const initial: Record<string, string> = {};
      if (currentRow) {
        for (const col of columns) {
          initial[col] = formatValue(currentRow[col]);
        }
      }
      editedValues = initial;
    });
  });

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "object") return JSON.stringify(value, null, 2);
    return String(value);
  }

  function getValueType(value: unknown): string {
    if (value === null || value === undefined) return "null";
    if (typeof value === "boolean") return "boolean";
    if (typeof value === "number") return "number";
    if (typeof value === "string") return "string";
    if (typeof value === "object") return "object";
    return "unknown";
  }

  function isMultiline(value: unknown): boolean {
    const s = formatValue(value);
    return s.includes("\n") || s.length > 80;
  }

  function isChanged(col: string): boolean {
    if (!baseRow) return false;
    return editedValues[col] !== formatValue(baseRow[col]);
  }

  function validateField(col: string): string | null {
    if (!baseRow) return null;
    const edited = (editedValues[col] ?? "").trim();
    if (edited.toUpperCase() === "NULL") return null;

    const originalType = typeof baseRow[col];

    if (originalType === "boolean") {
      if (!["true", "false"].includes(edited.toLowerCase())) {
        return 'Must be "true", "false", or "NULL"';
      }
    }

    if (originalType === "number") {
      if (!Number.isFinite(Number(edited))) {
        return "Must be a valid number or NULL";
      }
    }

    if (originalType === "object" && baseRow[col] !== null) {
      try {
        JSON.parse(edited);
      } catch {
        return "Must be valid JSON or NULL";
      }
    }

    return null;
  }

  const fieldErrors = $derived(
    Object.fromEntries(columns.map((col) => [col, isChanged(col) ? validateField(col) : null])),
  );

  const hasValidationErrors = $derived(
    columns.some((col) => fieldErrors[col] !== null),
  );

  const hasChanges = $derived(
    row !== null && columns.some((col) => isChanged(col)),
  );

  const canUpdate = $derived(
    hasChanges && !hasValidationErrors && !!connectionId && !!tableName,
  );

  // Converts an edited string value to a SQL literal
  function toSqlLiteral(editedValue: string, originalValue: unknown): string {
    if (editedValue.trim().toUpperCase() === "NULL") return "NULL";
    if (typeof originalValue === "boolean") {
      return editedValue.toLowerCase() === "true" ? "TRUE" : "FALSE";
    }
    if (typeof originalValue === "number") {
      const n = Number(editedValue);
      return Number.isFinite(n) ? String(n) : `'${editedValue.replace(/'/g, "''")}'`;
    }
    return `'${editedValue.replace(/'/g, "''")}'`;
  }

  // Converts an original value to a SQL literal for the WHERE clause
  function toWhereLiteral(value: unknown): string {
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
    if (typeof value === "number") return String(value);
    if (typeof value === "object") return `'${JSON.stringify(value).replace(/'/g, "''")}'`;
    return `'${String(value).replace(/'/g, "''")}'`;
  }

  async function handleUpdate() {
    if (!row || !connectionId || !tableName) return;
    isUpdating = true;

    const changedCols = columns.filter((col) => isChanged(col));
    const setClauses = changedCols.map(
      (col) => `"${col}" = ${toSqlLiteral(editedValues[col], baseRow![col])}`,
    );
    const whereClauses = columns.map((col) => {
      const val = baseRow![col];
      if (val === null || val === undefined) return `"${col}" IS NULL`;
      return `"${col}" = ${toWhereLiteral(val)}`;
    });

    const sql = `UPDATE "${tableName}" SET ${setClauses.join(", ")} WHERE ${whereClauses.join(" AND ")}`;

    try {
      await invoke("run_query", { connectionId, sql });
      for (const col of changedCols) {
        baseRow![col] = editedValues[col];
      }
      toast.success("Row updated successfully");
      onUpdateSuccess?.();
    } catch (err) {
      toast.error(String(err));
    } finally {
      isUpdating = false;
    }
  }

  async function copyToClipboard(text: string, column: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedColumn = column;
      setTimeout(() => (copiedColumn = null), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  }
</script>

<div class="h-full flex flex-col overflow-hidden border-l border-border bg-background">
  <div class="flex items-center justify-between px-4 py-3 border-b border-border shrink-0">
    <h3 class="text-sm font-semibold">Row Details</h3>
    <Button variant="ghost" size="icon" class="h-7 w-7" onclick={onClose}>
      <X class="w-4 h-4" />
    </Button>
  </div>

  {#if row}
    <ScrollArea class="flex-1 h-0">
      <div class="p-4 space-y-4">
        {#each columns as col}
          {@const value = row[col]}
          {@const valueType = getValueType(value)}
          {@const changed = isChanged(col)}
          {@const error = fieldErrors[col]}
          {@const multiline = isMultiline(value)}
          {@const inputClass = `w-full text-sm font-mono bg-muted/50 rounded px-3 py-2 outline-none transition-colors border focus:border-border ${error ? 'border-destructive/70' : changed ? 'border-amber-500/60' : 'border-transparent'}`}

          <div class="space-y-1.5">
            <div class="flex items-center justify-between">
              <span class="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                {col}
                {#if error}
                  <span class="ml-1 text-destructive">•</span>
                {:else if changed}
                  <span class="ml-1 text-amber-500">•</span>
                {/if}
              </span>
              <div class="flex items-center gap-1">
                <span class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground">
                  {valueType}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  class="h-6 w-6"
                  onclick={() => copyToClipboard(editedValues[col] ?? "", col)}
                  title="Copy value"
                >
                  {#if copiedColumn === col}
                    <Check class="w-3 h-3 text-green-500" />
                  {:else}
                    <Copy class="w-3 h-3" />
                  {/if}
                </Button>
              </div>
            </div>

            {#if multiline}
              <textarea
                class="{inputClass} break-all resize-y min-h-16 max-h-48"
                value={editedValues[col] ?? ""}
                oninput={(e) => (editedValues[col] = e.currentTarget.value)}
                rows={Math.min(6, (editedValues[col] ?? "").split("\n").length + 1)}
              ></textarea>
            {:else}
              <input
                type="text"
                class={inputClass}
                value={editedValues[col] ?? ""}
                oninput={(e) => (editedValues[col] = e.currentTarget.value)}
              />
            {/if}

            {#if error}
              <p class="text-xs text-destructive">{error}</p>
            {/if}
          </div>
        {/each}
      </div>
    </ScrollArea>

    {#if canUpdate}
      <div class="px-4 py-3 border-t border-border shrink-0">
        <Button class="w-full" onclick={handleUpdate} disabled={isUpdating}>
          <Save class="w-4 h-4 mr-2" />
          {isUpdating ? "Updating…" : "Update Row"}
        </Button>
      </div>
    {/if}
  {:else}
    <div class="flex-1 flex items-center justify-center text-sm text-muted-foreground">
      <p>Select a row to view details</p>
    </div>
  {/if}
</div>
