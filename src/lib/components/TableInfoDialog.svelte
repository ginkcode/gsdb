<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Copy, Loader } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { EditorView, lineNumbers } from "@codemirror/view";
  import { EditorState } from "@codemirror/state";
  import { sql, StandardSQL } from "@codemirror/lang-sql";
  import { oneDark } from "@codemirror/theme-one-dark";

  interface Props {
    open: boolean;
    tableName: string;
    definition: string;
    loading: boolean;
    onClose: () => void;
  }

  let {
    open = $bindable(false),
    tableName,
    definition,
    loading,
    onClose,
  }: Props = $props();

  let editorEl: HTMLDivElement | undefined = $state();
  let editorView: EditorView | undefined = $state();

  onMount(() => {
    return () => {
      editorView?.destroy();
    };
  });

  $effect(() => {
    if (editorEl && definition && open) {
      editorView?.destroy();
      const state = EditorState.create({
        doc: definition,
        extensions: [
          lineNumbers(),
          sql({
            dialect: StandardSQL,
          }),
          oneDark,
          EditorView.editable.of(false),
          EditorView.lineWrapping,
        ],
      });
      editorView = new EditorView({
        state,
        parent: editorEl,
      });
    }
  });

  async function copyToClipboard() {
    if (definition) {
      await navigator.clipboard.writeText(definition);
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="sm:max-w-3xl max-h-[85vh] overflow-hidden flex flex-col w-full"
  >
    <Dialog.Header>
      <Dialog.Title>Table: <strong>{tableName}</strong></Dialog.Title>
    </Dialog.Header>
    <div class="flex-1 overflow-auto">
      {#if loading}
        <div class="flex items-center justify-center py-8">
          <Loader class="w-6 h-6 animate-spin text-muted-foreground" />
        </div>
      {:else if definition}
        <div bind:this={editorEl} class="cm-editor-wrapper"></div>
      {/if}
    </div>
    <Dialog.Footer>
      <Button
        variant="ghost"
        size="icon"
        onclick={copyToClipboard}
        disabled={!definition || loading}
        title="Copy to clipboard"
      >
        <Copy class="w-4 h-4" />
      </Button>
      <Button variant="outline" onclick={onClose}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .cm-editor-wrapper :global(.cm-editor) {
    background: transparent;
    font-size: 0.875rem;
    height: auto;
    min-height: 200px;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-scroller) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
      "Liberation Mono", "Courier New", monospace;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-content) {
    padding: 0;
  }
  .cm-editor-wrapper :global(.cm-editor .cm-line) {
    padding: 0;
  }
</style>
