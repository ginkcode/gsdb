# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Development
npm run tauri dev          # Start dev build (Vite + Rust hot-reload)

# Type checking (frontend only)
npm run check              # svelte-check + tsc

# Rust checks
cd src-tauri && cargo check
cd src-tauri && cargo clippy

# Production build
npm run tauri:build        # Tauri desktop app (NO_STRIP=1 tauri build)
```

There are no automated tests. Validation is done via `cargo check`/`cargo clippy` for Rust and `npm run check` for the frontend.

## Architecture

GSDB is a Tauri 2 desktop app: a SvelteKit frontend communicates with a Rust backend via Tauri IPC commands. All database I/O happens in Rust — the frontend never connects to databases directly.

### Rust backend (`src-tauri/src/`)

**Driver abstraction** — `db/driver.rs` defines a `Driver` async trait and a `Dialect` enum (Postgres, Mysql, Sqlite, SqlServer). Each database has its own file implementing this trait:
- `postgres.rs`, `mysql.rs`, `sqlite.rs` — backed by sqlx
- `sqlserver.rs` — backed by tiberius (sqlx does not support MSSQL)

`DbPool` in `db/mod.rs` is a newtype `Arc<dyn Driver>` — cheap to clone, hides the variant. It is the only type callers interact with. Validation that applies to all drivers (e.g. the alphanumeric whitelist on `create_database`) lives here, not in the individual drivers.

**Connection** (`db/types.rs`) holds credentials. For postgres/mysql it builds `PgConnectOptions`/`MySqlConnectOptions` (never a URL with embedded password). SQLite still uses a URL string.

**SSH tunnel** (`db/ssh.rs`) uses `ssh2`. When `conn.ssh` is set, `DbPool::connect` spins up a tunnel first and extracts the local port, then passes `127.0.0.1:<local_port>` to the driver. The tunnel uses a temporary file for private keys because `libssh2`'s `userauth_pubkey_memory` does not support OpenSSH format.

**Tauri commands** (`commands/mod.rs`) hold `AppState` — a struct with two `Mutex<HashMap<String, _>>`: one for `Connection` configs and one for `DbPool` handles, keyed by connection ID.

### Svelte frontend (`src/`)

`src/lib/stores/connections.ts` is the central state. It:
- Persists connection metadata to disk via `@tauri-apps/plugin-store`
- Stores passwords separately in the OS keyring via `tauri-plugin-keyring-api`
- Manages query tabs (including "temporary preview" tabs that get replaced on next single-click)
- Calls `invoke()` for all database operations

The main page (`src/routes/+page.svelte`) composes `Sidebar` + `TabBar` + `QueryWorkspace`. `Sidebar` owns the table browser and connection tree. `QueryWorkspace` renders a `SqlEditor` (CodeMirror) per tab plus a `ResultTable`.

`src/lib/types/index.ts` is the source of truth for shared types (`Connection`, `QueryResult`, `DbDriver`, etc.) — keep this in sync with `db/types.rs`.

### Adding a new database driver

1. Add a struct in a new `src-tauri/src/db/<name>.rs` and implement the `Driver` trait
2. Register the module in `db/mod.rs` and add a match arm in `DbPool::connect`
3. Add the driver string to `DbDriver` in `src/lib/types/index.ts`
4. Add `defaultPorts` entry and select option in `ConnectionForm.svelte`
5. Add driver badge label in `ConnectionItem.svelte` and `Sidebar.svelte`

### Release

See `RELEASE-GUIDE.md`. The `Makefile` handles version bumping, tagging, and pushing. CI (`.github/workflows/release.yml`) builds for Linux/macOS/Windows and publishes apt/yum packages to `ginkcode/ginkcode.github.io`.
