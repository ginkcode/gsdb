# AUR Package Management Guide

This guide explains how to manage the GSDB AUR packages (`gsdb` and `gsdb-bin`).

## Package Overview

| Package    | Description       | Build Dependencies | Install Time |
| ---------- | ----------------- | ------------------ | ------------ |
| `gsdb`     | Build from source | rust, nodejs, npm  | ~5-10 min    |
| `gsdb-bin` | Pre-built binary  | none               | ~30 sec      |

Users should prefer `gsdb-bin` for faster installation. Use `gsdb` only if they want to compile from source.

## Initial Setup (One-time)

### Create AUR Repositories

```bash
# Create gsdb AUR repo
cd /home/gink/Workspaces/learning/tools
git clone ssh://aur@aur.archlinux.org/gsdb.git
cd gsdb
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/PKGBUILD .
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/.SRCINFO .
git init
git add PKGBUILD .SRCINFO
git commit -m "Initial upload: gsdb 0.2.0"
git remote add origin ssh://aur@aur.archlinux.org/gsdb.git
git push -f origin master

# Create gsdb-bin AUR repo
cd /home/gink/Workspaces/learning/tools
git clone ssh://aur@aur.archlinux.org/gsdb-bin.git
cd gsdb-bin
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/PKGBUILD-bin ./PKGBUILD
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/.SRCINFO-bin ./.SRCINFO
git init
git add PKGBUILD .SRCINFO
git commit -m "Initial upload: gsdb-bin 0.2.0"
git remote add origin ssh://aur@aur.archlinux.org/gsdb-bin.git
git push -f origin master
```

## Release Workflow

When releasing a new version, follow these steps:

### 1. Update Version and Build

```bash
# In main repo
cd /home/gink/Workspaces/learning/tools/gs-data-tool
make set-version V=0.2.1
git add . && git commit -m "chore: bump version to 0.2.1"
make release
```

This will:

- Update version in package.json, Cargo.toml, tauri.conf.json, PKGBUILD files
- Build the AppImage
- Create and push git tag
- Create GitHub release with AppImage

### 2. Update AUR Packages

```bash
# Update gsdb (build from source)
cd /home/gink/Workspaces/learning/tools/gs-data-aur
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/PKGBUILD .
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/.SRCINFO .
makepkg --printsrcinfo > .SRCINFO
git add . && git commit -m "Update to 0.2.1"
git push

# Update gsdb-bin (pre-built binary)
cd /home/gink/Workspaces/learning/tools/gsdb-bin
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/PKGBUILD-bin ./PKGBUILD
cp /home/gink/Workspaces/learning/tools/gs-data-tool/pkg/aur/.SRCINFO-bin ./.SRCINFO
git add . && git commit -m "Update to 0.2.1"
git push
```

### 3. Verify

```bash
# Check packages are updated
yay -Ss gsdb
yay -Ss gsdb-bin
```

## File Locations

| File                   | Purpose                   |
| ---------------------- | ------------------------- |
| `pkg/aur/PKGBUILD`     | Build from source package |
| `pkg/aur/PKGBUILD-bin` | Pre-built binary package  |
| `pkg/aur/.SRCINFO`     | Source info for gsdb      |
| `pkg/aur/.SRCINFO-bin` | Source info for gsdb-bin  |
| `pkg/aur/gsdb.desktop` | Desktop entry file        |

## Important Notes

### CARGO_TARGET_DIR

The PKGBUILD explicitly sets `--target-dir target` to ensure consistent build output location regardless of user's `CARGO_TARGET_DIR` environment variable.

### AppImage Naming

The AppImage filename follows the pattern: `GSDB_{version}_amd64.AppImage`

Example: `GSDB_0.2.0_amd64.AppImage`

### Dependencies

**Runtime (both packages):**

- webkit2gtk-4.1
- gtk3
- libsoup3
- openssl
- libssh2

**Build (gsdb only):**

- rust
- nodejs
- npm

## Troubleshooting

### Package not found in search

AUR search indexing can take a few minutes after pushing. Direct installation still works:

```bash
yay -S gsdb-bin
```

### Build fails with CARGO_TARGET_DIR error

Fixed in PKGBUILD by explicitly setting `--target-dir target` in the cargo build command.

### AppImage download fails

Ensure the GitHub release exists and the version in PKGBUILD matches the release tag.
