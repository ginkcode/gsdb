<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Select from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import type { Connection, DbDriver, SshTunnel } from "$lib/types";

  let {
    open = $bindable(false),
    editing = $bindable(null as Connection | null),
    onSave,
  }: {
    open: boolean;
    editing: Connection | null;
    onSave: (conn: Connection) => void;
  } = $props();

  let driver = $state<DbDriver>("postgres");
  let name = $state("");
  let color = $state("blue");
  let host = $state("localhost");
  let port = $state(5432);
  let database = $state("");
  let username = $state("");
  let password = $state("");
  let filePath = $state("");

  // SSH tunnel fields
  let useSsh = $state(false);
  let sshHost = $state("");
  let sshPort = $state(22);
  let sshUsername = $state("");
  let sshPassword = $state("");
  let sshPrivateKey = $state("");
  let sshPrivateKeyPassphrase = $state("");

  const colorOptions = [
    {
      value: "blue",
      label: "Blue",
      class: "bg-blue-500/15 text-blue-400 border-blue-500/20",
    },
    {
      value: "green",
      label: "Green",
      class: "bg-green-500/15 text-green-400 border-green-500/20",
    },
    {
      value: "orange",
      label: "Orange",
      class: "bg-orange-500/15 text-orange-400 border-orange-500/20",
    },
    {
      value: "purple",
      label: "Purple",
      class: "bg-purple-500/15 text-purple-400 border-purple-500/20",
    },
    {
      value: "red",
      label: "Red",
      class: "bg-red-500/15 text-red-400 border-red-500/20",
    },
    {
      value: "yellow",
      label: "Yellow",
      class: "bg-yellow-500/15 text-yellow-400 border-yellow-500/20",
    },
    {
      value: "pink",
      label: "Pink",
      class: "bg-pink-500/15 text-pink-400 border-pink-500/20",
    },
    {
      value: "cyan",
      label: "Cyan",
      class: "bg-cyan-500/15 text-cyan-400 border-cyan-500/20",
    },
  ];

  const defaultPorts: Record<DbDriver, number> = {
    postgres: 5432,
    mysql: 3306,
    sqlite: 0,
  };

  // Reset form when dialog opens/closes or editing changes
  $effect(() => {
    if (open) {
      if (editing) {
        // Populate form with existing connection data
        driver = editing.driver;
        name = editing.name;
        color = editing.color ?? "blue";
        host = editing.host ?? "localhost";
        port = editing.port ?? defaultPorts[editing.driver];
        database = editing.database;
        username = editing.username ?? "";
        password = editing.password ?? "";
        filePath = editing.filePath ?? "";

        // SSH fields
        useSsh = !!editing.ssh;
        if (editing.ssh) {
          sshHost = editing.ssh.host;
          sshPort = editing.ssh.port;
          sshUsername = editing.ssh.username;
          sshPassword = editing.ssh.password ?? "";
          sshPrivateKey = editing.ssh.privateKey ?? "";
          sshPrivateKeyPassphrase = editing.ssh.privateKeyPassphrase ?? "";
        } else {
          sshHost = "";
          sshPort = 22;
          sshUsername = "";
          sshPassword = "";
          sshPrivateKey = "";
          sshPrivateKeyPassphrase = "";
        }
      } else {
        // Reset to defaults for new connection
        driver = "postgres";
        name = "";
        color = "blue";
        host = "localhost";
        port = 5432;
        database = "";
        username = "";
        password = "";
        filePath = "";
        useSsh = false;
        sshHost = "";
        sshPort = 22;
        sshUsername = "";
        sshPassword = "";
        sshPrivateKey = "";
        sshPrivateKeyPassphrase = "";
      }
    }
  });

  function onDriverChange(v: string | undefined) {
    if (!v) return;
    driver = v as DbDriver;
    port = defaultPorts[driver];
  }

  function submit() {
    const ssh: SshTunnel | undefined =
      useSsh && driver !== "sqlite"
        ? {
            host: sshHost,
            port: sshPort,
            username: sshUsername,
            password: sshPassword || undefined,
            privateKey: sshPrivateKey || undefined,
            privateKeyPassphrase: sshPrivateKeyPassphrase || undefined,
          }
        : undefined;

    const conn: Connection = {
      id: editing?.id ?? crypto.randomUUID(),
      name: name || `${driver}/${database}`,
      driver,
      color,
      database,
      ...(driver !== "sqlite" ? { host, port, username, password } : {}),
      ...(driver === "sqlite" ? { filePath } : {}),
      ...(ssh ? { ssh } : {}),
    };
    onSave(conn);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg max-h-[90vh] overflow-y-auto">
    <Dialog.Header>
      <Dialog.Title
        >{editing ? "Edit Connection" : "New Connection"}</Dialog.Title
      >
      <Dialog.Description
        >{editing
          ? "Update connection settings"
          : "Connect to a database"}</Dialog.Description
      >
    </Dialog.Header>

    <div class="grid gap-4 py-2">
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="conn-name">Name</label>
        <Input id="conn-name" bind:value={name} placeholder="My Database" />
      </div>

      <div class="grid gap-1.5">
        <label class="text-sm font-medium">Label Color</label>
        <div class="flex flex-wrap gap-2">
          {#each colorOptions as opt}
            <button
              type="button"
              class="px-3 py-1.5 text-xs font-bold rounded border transition-all {opt.class} {color ===
              opt.value
                ? 'ring-2 ring-offset-1 ring-offset-background'
                : 'opacity-60 hover:opacity-100'}"
              onclick={() => (color = opt.value)}
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="conn-driver">Driver</label>
        <Select.Root
          type="single"
          value={driver}
          onValueChange={onDriverChange}
        >
          <Select.Trigger id="conn-driver">
            {driver === "postgres"
              ? "PostgreSQL"
              : driver === "mysql"
                ? "MySQL"
                : "SQLite"}
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

        <!-- SSH Tunnel Section -->
        <div class="border-t border-border pt-4 mt-2">
          <label
            class="flex items-center gap-2 text-sm font-medium cursor-pointer"
          >
            <input
              type="checkbox"
              checked={useSsh}
              onchange={() => (useSsh = !useSsh)}
              class="h-4 w-4 rounded border-border"
            />
            Connect via SSH Tunnel
          </label>
        </div>

        {#if useSsh}
          <div class="grid gap-3 pl-6 border-l-2 border-muted ml-1">
            <div class="grid grid-cols-3 gap-2">
              <div class="col-span-2 grid gap-1.5">
                <label class="text-sm font-medium" for="ssh-host"
                  >SSH Host</label
                >
                <Input
                  id="ssh-host"
                  bind:value={sshHost}
                  placeholder="ssh.example.com"
                />
              </div>
              <div class="grid gap-1.5">
                <label class="text-sm font-medium" for="ssh-port">Port</label>
                <Input id="ssh-port" type="number" bind:value={sshPort} />
              </div>
            </div>
            <div class="grid gap-1.5">
              <label class="text-sm font-medium" for="ssh-user"
                >SSH Username</label
              >
              <Input id="ssh-user" bind:value={sshUsername} />
            </div>
            <div class="grid gap-1.5">
              <label class="text-sm font-medium" for="ssh-pass"
                >SSH Password</label
              >
              <Input
                id="ssh-pass"
                type="password"
                bind:value={sshPassword}
                placeholder="Leave empty if using key"
              />
            </div>
            <div class="grid gap-1.5">
              <label class="text-sm font-medium" for="ssh-key"
                >Private Key</label
              >
              <Input
                id="ssh-key"
                type="password"
                bind:value={sshPrivateKey}
                placeholder="Paste private key or leave empty for agent"
              />
              <p class="text-xs text-muted-foreground">
                Paste your private key or leave empty to use SSH agent
              </p>
            </div>
            <div class="grid gap-1.5">
              <label class="text-sm font-medium" for="ssh-key-pass"
                >Key Passphrase</label
              >
              <Input
                id="ssh-key-pass"
                type="password"
                bind:value={sshPrivateKeyPassphrase}
                placeholder="If your key has a passphrase"
              />
            </div>
          </div>
        {/if}
      {:else}
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="conn-path">File Path</label>
          <Input
            id="conn-path"
            bind:value={filePath}
            placeholder="/path/to/db.sqlite"
          />
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
