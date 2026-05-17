# AGENTS.md

## Commands

```bash
npm run tauri dev              # Start dev (Vite + Rust hot-reload)
npm run check                  # Frontend typecheck (svelte-check + tsc)
cd src-tauri && cargo check    # Rust type check
cd src-tauri && cargo clippy   # Rust lint
npm run tauri:build            # Production build (NO_STRIP=1 tauri build)
```

There are no automated tests. Validate changes with `cargo check`/`cargo clippy` for Rust and `npm run check` for frontend.

## Architecture

Tauri 2 desktop app: SvelteKit frontend ↔ Rust backend via Tauri IPC. All database I/O is Rust-only; the frontend never connects to databases directly.

### Rust (`src-tauri/src/`)

- **`db/driver.rs`** — `Driver` async trait + `Dialect` enum (Postgres, Mysql, Sqlite, SqlServer). Each driver has its own file; `sqlserver.rs` uses tiberius (not sqlx).
- **`db/mod.rs`** — `DbPool` newtype wrapping `Arc<dyn Driver>`. Cross-driver validation (e.g. alphanumeric whitelist on `create_database`) lives here, not in individual drivers.
- **`db/types.rs`** — Connection credentials. Postgres/mysql use `PgConnectOptions`/`MySqlConnectOptions` objects, never URLs with embedded passwords.
- **`db/ssh.rs`** — Uses `russh` (not `ssh2`). Private keys are written to a temp file because `russh`/libssh2 cannot handle OpenSSH format in-memory.
- **`commands/mod.rs`** — `AppState` holds two `Mutex<HashMap<String, _>>`: connection configs and `DbPool` handles, keyed by connection ID.

### Frontend (`src/`)

- **`src/lib/stores/connections.ts`** — Central state. Persists connections via `@tauri-apps/plugin-store`, passwords via OS keyring (`tauri-plugin-keyring-api`). Manages query tabs including "temporary preview" tabs replaced on single-click.
- **`src/lib/types/index.ts`** — Source of truth for shared types. Must stay in sync with `db/types.rs`.
- **`src/routes/+page.svelte`** — Composes `Sidebar` + `TabBar` + `QueryWorkspace`.
- SvelteKit uses `adapter-static` with SPA fallback (`index.html`). No SSR.

### UI components

- Tailwind CSS v4 with `@source "../"` directive in `layout.css` — required for AUR builds that lack `.git` (Tailwind uses `.git` to find the project root).
- UI primitives from `bits-ui` + `paneforge`. Icons from `@lucide/svelte`. SQL editor via CodeMirror.
- Dark mode via `mode-watcher` + `@custom-variant dark` in `layout.css`.

## Adding a database driver

1. Create `src-tauri/src/db/<name>.rs` implementing `Driver` trait
2. Register module in `db/mod.rs`, add match arm in `DbPool::connect`
3. Add driver string to `DbDriver` in `src/lib/types/index.ts`
4. Add `defaultPorts` entry + select option in `ConnectionForm.svelte`
5. Add driver badge label in `ConnectionItem.svelte` and `Sidebar.svelte`

## Key pitfalls

- **SSH tunnel private keys**: Must be written to a temp file; `russh` cannot authenticate with OpenSSH-format keys from memory.
- **Type sync**: Adding a field to `Connection` or `QueryResult` in Rust requires the matching change in `src/lib/types/index.ts` and vice versa.
- **`db/mod.rs` vs driver files**: Cross-driver validation goes in `DbPool` methods, not in individual driver implementations.
- **Tailwind v4 source detection**: The `@source "../"` directive in `layout.css` is needed because AUR tarballs lack `.git`. Do not remove it.
- **Production build**: Uses `NO_STRIP=1` to prevent debug symbol stripping issues.
- **`Cargo.toml` lib name**: Uses `gsdb_lib` (not `gsdb`) to avoid Windows conflict between lib and bin names.

## Release

- `make set-version V=x.y.z` — bumps version in `package.json`, `Cargo.toml`, `tauri.conf.json`, both PKGBUILDs, regenerates `.SRCINFO`
- `make release` — builds, pushes, tags, creates GitHub release with AppImage
- CI builds for Linux (amd64+arm64), macOS (universal), Windows and publishes apt/yum packages to GitHub Pages
- See `RELEASE-GUIDE.md` for full details including AUR deployment