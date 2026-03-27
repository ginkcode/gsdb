<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Select from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import type { Connection, DbDriver } from "$lib/types";

  let { open = $bindable(false), onSave }: {
    open: boolean;
    onSave: (conn: Connection) => void;
  } = $props();

  let driver = $state<DbDriver>("postgres");
  let name = $state("");
  let host = $state("localhost");
  let port = $state(5432);
  let database = $state("");
  let username = $state("");
  let password = $state("");
  let filePath = $state("");

  const defaultPorts: Record<DbDriver, number> = {
    postgres: 5432,
    mysql: 3306,
    sqlite: 0,
  };

  function onDriverChange(v: string | undefined) {
    if (!v) return;
    driver = v as DbDriver;
    port = defaultPorts[driver];
  }

  function submit() {
    const conn: Connection = {
      id: crypto.randomUUID(),
      name: name || `${driver}/${database}`,
      driver,
      database,
      ...(driver !== "sqlite" ? { host, port, username, password } : {}),
      ...(driver === "sqlite" ? { filePath } : {}),
    };
    onSave(conn);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title>New Connection</Dialog.Title>
      <Dialog.Description>Connect to a database</Dialog.Description>
    </Dialog.Header>

    <div class="grid gap-4 py-2">
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="conn-name">Name</label>
        <Input id="conn-name" bind:value={name} placeholder="My Database" />
      </div>

      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="conn-driver">Driver</label>
        <Select.Root type="single" value={driver} onValueChange={onDriverChange}>
          <Select.Trigger id="conn-driver">
            {driver === "postgres" ? "PostgreSQL" : driver === "mysql" ? "MySQL" : "SQLite"}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="postgres">PostgreSQL</Select.Item>
            <Select.Item value="mysql">MySQL</Select.Item>
            <Select.Item value="sqlite">SQLite</Select.Item>
          </Select.Content>
        </Select.Root>
      </div>

      {#if driver !== "sqlite"}
        <div class="grid grid-cols-3 gap-2">
          <div class="col-span-2 grid gap-1.5">
            <label class="text-sm font-medium" for="conn-host">Host</label>
            <Input id="conn-host" bind:value={host} />
          </div>
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" for="conn-port">Port</label>
            <Input id="conn-port" type="number" bind:value={port} />
          </div>
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="conn-db">Database</label>
          <Input id="conn-db" bind:value={database} />
        </div>
        <div class="grid grid-cols-2 gap-2">
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" for="conn-user">Username</label>
            <Input id="conn-user" bind:value={username} />
          </div>
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" for="conn-pass">Password</label>
            <Input id="conn-pass" type="password" bind:value={password} />
          </div>
        </div>
      {:else}
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="conn-path">File Path</label>
          <Input id="conn-path" bind:value={filePath} placeholder="/path/to/db.sqlite" />
        </div>
      {/if}
    </div>

    <Dialog.Footer>
      <Dialog.Close>
        <Button variant="outline">Cancel</Button>
      </Dialog.Close>
      <Button onclick={submit}>Connect</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
