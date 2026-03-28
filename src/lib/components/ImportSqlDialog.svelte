<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Loader } from "@lucide/svelte";

  interface Props {
    open: boolean;
    fileName: string;
    preview: string;
    truncated: boolean;
    totalBytes: number;
    disableFkChecks: boolean;
    loading: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open = $bindable(false),
    fileName,
    preview,
    truncated,
    totalBytes,
    disableFkChecks = $bindable(false),
    loading,
    onConfirm,
    onCancel,
  }: Props = $props();

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="sm:max-w-3xl max-h-[85vh] overflow-hidden flex flex-col w-full"
  >
    <Dialog.Header>
      <Dialog.Title>Import SQL</Dialog.Title>
      <Dialog.Description class="text-xs text-muted-foreground truncate">
        {fileName}
        <span class="ml-2 text-muted-foreground/60"
          >({formatBytes(totalBytes)})</span
        >
      </Dialog.Description>
    </Dialog.Header>

    <!-- SQL preview -->
    <div class="flex-1 overflow-auto rounded border border-border bg-muted/30">
      <pre
        class="p-3 text-xs font-mono whitespace-pre-wrap break-all leading-relaxed text-foreground">{preview}</pre>
      {#if truncated}
        <div
          class="sticky bottom-0 px-3 py-2 text-xs text-amber-500 bg-muted/80 border-t border-border"
        >
          Preview truncated — showing first 16 KB of {formatBytes(totalBytes)}
          file. Full file will be imported.
        </div>
      {/if}
    </div>

    <!-- Options -->
    <div class="pt-2">
      <label class="flex items-center gap-2 cursor-pointer select-none">
        <input
          type="checkbox"
          class="rounded border-border"
          bind:checked={disableFkChecks}
        />
        <span class="text-sm">Disable foreign key checks during import</span>
      </label>
      <p class="mt-1 ml-6 text-xs text-muted-foreground">
        Useful when importing data with circular references or out-of-order
        inserts.
      </p>
    </div>

    <Dialog.Footer>
      <Button variant="outline" onclick={onCancel} disabled={loading}
        >Cancel</Button
      >
      <Button onclick={onConfirm} disabled={loading}>
        {#if loading}
          <Loader class="w-4 h-4 mr-2 animate-spin" />
          Importing...
        {:else}
          Confirm Import
        {/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
