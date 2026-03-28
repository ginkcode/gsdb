<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";
  import { Loader } from "@lucide/svelte";

  type TableActionType = "delete" | "truncate" | "drop";

  interface Props {
    open: boolean;
    actionType: TableActionType;
    tableName: string;
    sql: string;
    loading: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    open = $bindable(false),
    actionType,
    tableName,
    sql,
    loading,
    onConfirm,
    onCancel,
  }: Props = $props();

  const actionMeta: Record<
    TableActionType,
    { title: string; description: string; confirm: string }
  > = {
    delete: {
      title: "Delete all rows",
      description:
        "All rows will be permanently deleted. The table structure will remain.",
      confirm: "Delete Rows",
    },
    truncate: {
      title: "Truncate table",
      description:
        "All rows will be removed from the table. This cannot be undone.",
      confirm: "Truncate",
    },
    drop: {
      title: "Drop table",
      description: "The table and all its data will be permanently deleted.",
      confirm: "Drop Table",
    },
  };
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title
        >{actionMeta[actionType].title}:
        <strong>{tableName}</strong></Dialog.Title
      >
      <Dialog.Description>
        {actionMeta[actionType].description}
      </Dialog.Description>
    </Dialog.Header>
    <div
      class="rounded border border-border bg-muted/40 px-3 py-2 font-mono text-xs text-muted-foreground"
    >
      {sql}
    </div>
    <Dialog.Footer>
      <Button variant="outline" onclick={onCancel} disabled={loading}
        >Cancel</Button
      >
      <Button variant="destructive" onclick={onConfirm} disabled={loading}>
        {#if loading}
          <Loader class="w-4 h-4 mr-2 animate-spin" />
          Processing...
        {:else}
          {actionMeta[actionType].confirm}
        {/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
