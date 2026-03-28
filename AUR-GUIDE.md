# AUR Package Management Guide

This guide explains how to manage the GSDB AUR packages (`gsdb` and `gsdb-bin`).

## Package Overview

| Package    | Description       | Build Dependencies | Install Time |
| ---------- | ----------------- | ------------------ | ------------ |
| `gsdb`     | Build from source | rust, nodejs, npm  | ~5-10 min    |
| `gsdb-bin` | Pre-built binary  | none               | ~30 sec      |

Users should prefer `gsdb-bin` for faster installation. Use `gsdb` only if they want to compile from source.

## Local AUR Repo Paths

| Repo       | Path                                                  |
| ---------- | ----------------------------------------------------- |
| `gsdb`     | `/home/gink/Workspaces/learning/tools/gsdb`     |
| `gsdb-bin` | `/home/gink/Workspaces/learning/tools/gsdb-bin` |

## Initial Setup (One-time)

Clone the AUR repos and do the first upload:

```bash
cd /home/gink/Workspaces/learning/tools

git clone ssh://aur@aur.archlinux.org/gsdb.git
git clone ssh://aur@aur.archlinux.org/gsdb-bin.git
```

Then deploy and push the initial files:

```bash
cd /home/gink/Workspaces/learning/tools/gs-data-tool
make aur-deploy

cd /home/gink/Workspaces/learning/tools/gsdb
git add -A && git commit -m "Initial upload: gsdb $(version)" && git push

cd /home/gink/Workspaces/learning/tools/gsdb-bin
git add -A && git commit -m "Initial upload: gsdb-bin $(version)" && git push
```

## Release Workflow

### 1. Set the new version

```bash
cd /home/gink/Workspaces/learning/tools/gs-data-tool
make set-version V=0.3.0
```

This updates the version in `package.json`, `Cargo.toml`, `tauri.conf.json`, both PKGBUILDs, and regenerates `.SRCINFO` files automatically.

### 2. Commit the version bump

```bash
git add .
git commit -m "chore: bump version to 0.3.0"
```

### 3. Build and publish the GitHub release

```bash
make release
```

This will:
- Build the app and AppImage
- Push commits to `main`
- Create and push the git tag
- Create the GitHub release with the AppImage attached

### 4. Deploy to AUR repos

```bash
make aur-deploy
```

This copies the correct files into both local AUR repos:
- `pkg/aur/PKGBUILD` + `.SRCINFO` + `gsdb.desktop` → `gsdb/`
- `pkg/aur/PKGBUILD-bin` + `.SRCINFO-bin` → `gsdb-bin/` (renamed to `PKGBUILD` / `.SRCINFO`)

### 5. Push to AUR

```bash
cd /home/gink/Workspaces/learning/tools/gsdb
git add -A && git commit -m "Update to 0.3.0" && git push

cd /home/gink/Workspaces/learning/tools/gsdb-bin
git add -A && git commit -m "Update to 0.3.0" && git push
```

### 6. Verify

```bash
yay -Ss gsdb
```

> AUR search indexing can take a few minutes after pushing. Direct install works immediately: `yay -S gsdb-bin`

## File Locations

| File                   | Purpose                        |
| ---------------------- | ------------------------------ |
| `pkg/aur/PKGBUILD`     | Build-from-source package      |
| `pkg/aur/PKGBUILD-bin` | Pre-built binary package       |
| `pkg/aur/.SRCINFO`     | Generated metadata for `gsdb`     |
| `pkg/aur/.SRCINFO-bin` | Generated metadata for `gsdb-bin` |
| `pkg/aur/gsdb.desktop` | Desktop entry file             |

## Dependencies

**Runtime (both packages):**
- webkit2gtk-4.1
- gtk3
- libsoup3
- openssl
- libssh2
- libsecret

**Build (`gsdb` only):**
- rust
- nodejs
- npm

## Troubleshooting

### Build fails with CARGO_TARGET_DIR error

The PKGBUILD explicitly sets `--target-dir target` so the binary is always at `src-tauri/target/release/gsdb` regardless of the user's `CARGO_TARGET_DIR` environment variable.

### AppImage download fails in gsdb-bin

Ensure the GitHub release exists and that the version in `PKGBUILD-bin` matches the release tag. Run `make release` before `make aur-deploy`.
