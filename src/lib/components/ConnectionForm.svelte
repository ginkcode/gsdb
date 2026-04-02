<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog";
  import * as Select from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import { readTextFile } from "@tauri-apps/plugin-fs";
  import type { Connection, DbDriver, SshTunnel } from "$lib/types";

  let {
    open = $bindable(false),
    editing = $bindable(null as Connection | null),
    onSave,
  }: {
    open: boolean;
    editing: Connection | null;
    onSave: (conn: Connection) => Promise<boolean>;
  } = $props();

  let connecting = $state(false);
  let driver = $state<DbDriver>("postgres");
  let name = $state("");
  let color = $state("blue");
  let host = $state("localhost");
  let port = $state(5432);
  let database = $state("");
  let username = $state("");
  let password = $state("");
  let filePath = $state("");

  // SSL mode (postgres + mysql)
  let sslMode = $state("prefer");

  const sslModeOptions: Record<string, { value: string; label: string }[]> = {
    postgres: [
      { value: "disable", label: "disable" },
      { value: "allow", label: "allow" },
      { value: "prefer", label: "prefer (default)" },
      { value: "require", label: "require" },
      { value: "verify-ca", label: "verify-ca" },
      { value: "verify-full", label: "verify-full" },
    ],
    mysql: [
      { value: "disabled", label: "disabled" },
      { value: "preferred", label: "preferred (default)" },
      { value: "required", label: "required" },
      { value: "verify_ca", label: "verify_ca" },
      { value: "verify_identity", label: "verify_identity" },
    ],
    sqlserver: [
      { value: "disable", label: "disable (no encryption)" },
      { value: "allow", label: "allow" },
      { value: "prefer", label: "prefer (default, trust cert)" },
      { value: "require", label: "require (trust cert)" },
      { value: "verify", label: "verify (validate cert)" },
    ],
  };

  const defaultSslMode: Record<string, string> = {
    postgres: "prefer",
    mysql: "preferred",
    sqlserver: "prefer",
  };

  // SSH tunnel fields
  let useSsh = $state(false);
  let sshHost = $state("");
  let sshPort = $state(22);
  let sshUsername = $state("");
  let sshPassword = $state("");
  let sshPrivateKey = $state("");
  let sshPrivateKeyPassphrase = $state("");
  // true when editing a connection that already has a saved key and the user
  // hasn't chosen to replace it yet
  let sshKeySaved = $state(false);

  async function pickKeyFile() {
    const path = await openFileDialog({ multiple: false, directory: false });
    if (!path) return;
    const content = await readTextFile(path as string);
    sshPrivateKey = content;
    sshKeySaved = false;
  }

  async function pickSqliteFile() {
    const path = await openFileDialog({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "SQLite Database",
          extensions: ["sqlite", "sqlite3", "db", "db3"],
        },
        { name: "All Files", extensions: ["*"] },
      ],
    });
    if (path) {
      filePath = path as string;
    }
  }

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
    {
      value: "indigo",
      label: "Indigo",
      class: "bg-indigo-500/15 text-indigo-400 border-indigo-500/20",
    },
  ];

  const defaultPorts: Record<DbDriver, number> = {
    postgres: 5432,
    mysql: 3306,
    sqlite: 0,
    sqlserver: 1433,
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
        sslMode = editing.sslMode ?? "prefer";

        // SSH fields
        useSsh = !!editing.ssh;
        if (editing.ssh) {
          sshHost = editing.ssh.host;
          sshPort = editing.ssh.port;
          sshUsername = editing.ssh.username;
          sshPassword = editing.ssh.password ?? "";
          sshPrivateKey = editing.ssh.privateKey ?? "";
          sshPrivateKeyPassphrase = editing.ssh.privateKeyPassphrase ?? "";
          // If a key is already saved, hide the textarea until user chooses to change it
          sshKeySaved = !!editing.ssh.privateKey;
        } else {
          sshHost = "";
          sshPort = 22;
          sshUsername = "";
          sshPassword = "";
          sshPrivateKey = "";
          sshPrivateKeyPassphrase = "";
          sshKeySaved = false;
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
        sslMode = "prefer";
        useSsh = false;
        sshHost = "";
        sshPort = 22;
        sshUsername = "";
        sshPassword = "";
        sshPrivateKey = "";
        sshPrivateKeyPassphrase = "";
        sshKeySaved = false;
      }
    }
  });

  function onDriverChange(v: string | undefined) {
    if (!v) return;
    driver = v as DbDriver;
    port = defaultPorts[driver];
    sslMode = defaultSslMode[driver] ?? "prefer";
  }

  async function submit() {
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

    // For SQLite, use the filename as the database name
    const dbDisplayName =
      driver === "sqlite"
        ? filePath
          ? filePath.split("/").pop()?.split("\\").pop() || "sqlite"
          : "sqlite"
        : database;

    const conn: Connection = {
      id: editing?.id ?? crypto.randomUUID(),
      name: name || `${driver}/${dbDisplayName}`,
      driver,
      color,
      // Preserve locked state when editing, or default to true for new connections
      locked: editing?.locked ?? true,
      database: driver === "sqlite" ? filePath || "sqlite" : database,
      ...(driver !== "sqlite" ? { host, port, username, password } : {}),
      ...(driver === "sqlite" ? { filePath } : {}),
      ...(driver === "postgres" || driver === "mysql" || driver === "sqlserver"
        ? { sslMode }
        : {}),
      ...(ssh ? { ssh } : {}),
    };
    connecting = true;
    const ok = await onSave(conn);
    connecting = false;
    if (ok) open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-2xl max-h-[90vh] overflow-y-auto">
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
        <span class="text-sm font-medium">Label Color</span>
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
                : driver === "sqlserver"
                  ? "SQL Server"
                  : "SQLite"}
          </Select.Trigger>
          <Select.Content>
            <Select.Item value="postgres">PostgreSQL</Select.Item>
            <Select.Item value="mysql">MySQL</Select.Item>
            <Select.Item value="sqlserver">SQL Server</Select.Item>
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

        <!-- SSL Mode (postgres + mysql + sqlserver) -->
        {#if driver === "postgres" || driver === "mysql" || driver === "sqlserver"}
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" for="conn-ssl">SSL Mode</label>
            <Select.Root
              type="single"
              value={sslMode}
              onValueChange={(v) => {
                if (v) sslMode = v;
              }}
            >
              <Select.Trigger id="conn-ssl">
                {sslMode}
              </Select.Trigger>
              <Select.Content>
                {#each sslModeOptions[driver] as opt}
                  <Select.Item value={opt.value}>{opt.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        {/if}

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
              <div class="flex items-center justify-between">
                <label class="text-sm font-medium" for="ssh-key"
                  >Private Key</label
                >
                <Button
                  variant="outline"
                  size="sm"
                  class="h-7 text-xs"
                  onclick={pickKeyFile}
                >
                  Select file…
                </Button>
              </div>
              {#if sshKeySaved}
                <div
                  class="flex items-center justify-between rounded-md border border-input bg-muted/40 px-3 py-2 text-xs text-muted-foreground"
                >
                  <span>Key saved — leave unchanged or replace below</span>
                  <button
                    type="button"
                    class="text-xs underline hover:text-foreground"
                    onclick={() => {
                      sshKeySaved = false;
                      sshPrivateKey = "";
                    }}
                  >
                    Clear
                  </button>
                </div>
              {:else}
                <textarea
                  id="ssh-key"
                  bind:value={sshPrivateKey}
                  placeholder="Paste private key or select a file…"
                  rows={6}
                  spellcheck={false}
                  autocomplete="off"
                  class="w-full rounded-md border border-input bg-background px-3 py-2 text-xs font-mono text-foreground shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
                ></textarea>
              {/if}
              <p class="text-xs text-muted-foreground">
                Leave empty to use SSH agent
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
          <div class="flex items-center justify-between">
            <label class="text-sm font-medium" for="conn-path">File Path</label>
            <Button
              variant="outline"
              size="sm"
              class="h-7 text-xs"
              onclick={pickSqliteFile}
            >
              Browse…
            </Button>
          </div>
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
      <Button onclick={submit} disabled={connecting}>
        {connecting
          ? editing
            ? "Updating…"
            : "Connecting…"
          : editing
            ? "Update"
            : "Connect"}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
