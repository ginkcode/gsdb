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

### Debian / Ubuntu (apt)

```bash
curl -fsSL https://ginkcode.github.io/gsdb/gpg.key \
  | sudo gpg --dearmor -o /usr/share/keyrings/gsdb.gpg
echo "deb [signed-by=/usr/share/keyrings/gsdb.gpg] https://ginkcode.github.io/gsdb/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/gsdb.list
sudo apt update
sudo apt install gsdb
```

### RHEL / Fedora (dnf)

```bash
sudo rpm --import https://ginkcode.github.io/gsdb/gpg.key
sudo tee /etc/yum.repos.d/gsdb.repo << 'EOF'
[gsdb]
name=GSDB Repository
baseurl=https://ginkcode.github.io/gsdb/rpm
enabled=1
gpgcheck=1
gpgkey=https://ginkcode.github.io/gsdb/gpg.key
EOF
sudo dnf install gsdb
```

### Direct download

Download the latest AppImage, `.deb`, `.rpm`, `.dmg`, or `.exe` from the [GitHub Releases](https://github.com/ginkcode/gsdb/releases/latest) page.

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
