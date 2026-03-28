# GSDB

A database management tool built with Tauri, SvelteKit, and TypeScript.

## Features

- Connect to PostgreSQL, MySQL, and SQLite databases
- SSH tunnel support for secure connections
- SQL editor with syntax highlighting
- Query results with export to CSV
- Connection persistence with secure password storage
- Dark/Light theme support

## Installation

### Arch Linux (AUR)

```bash
# Build from source
yay -S gsdb

# Or use the pre-built binary
yay -S gsdb-bin
```

### From Source

```bash
git clone https://github.com/ginkcode/gsdb.git
cd gsdb
npm install
npm run tauri build
```

## Development

```bash
npm install
npm run tauri dev
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
