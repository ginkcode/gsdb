# Release Guide

This guide explains how to release GSDB across all distribution channels.

## Distribution Channels

| Channel | Target users | Format |
|---|---|---|
| GitHub Releases | Direct download | AppImage, deb, rpm, dmg, exe |
| apt repository | Debian/Ubuntu | `.deb` via GitHub Pages |
| yum/dnf repository | RHEL/Fedora | `.rpm` via GitHub Pages |
| AUR (`gsdb-bin`) | Arch Linux | Pre-built binary |
| AUR (`gsdb`) | Arch Linux | Build from source |

---

## Release Workflow

### 1. Set the new version

```bash
cd /home/gink/Workspaces/learning/tools/gs-data-tool
make set-version V=0.3.0
```

This updates the version in `package.json`, `package-lock.json`, `Cargo.toml`, `tauri.conf.json`, both PKGBUILDs, and regenerates `.SRCINFO` files automatically.

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
- Push commits to `main`
- Create and push the git tag
- Trigger the CI workflow which builds and uploads:
  - AppImage, `.deb`, `.rpm` (Linux amd64 + arm64)
  - `.dmg` (macOS x86_64 + aarch64)
  - `.exe` (Windows)
- Creates the GitHub release with all artifacts attached
- Publishes `.deb` and `.rpm` to the apt/yum repositories on GitHub Pages

### 4. Deploy to AUR repos

```bash
make aur-deploy
```

Then push to AUR:

```bash
cd /home/gink/Workspaces/learning/tools/gsdb
git add -A && git commit -m "Update to 0.3.0" && git push

cd /home/gink/Workspaces/learning/tools/gsdb-bin
git add -A && git commit -m "Update to 0.3.0" && git push
```

### 5. Verify

```bash
# Arch
yay -Ss gsdb

# Debian/Ubuntu
apt-cache show gsdb

# RHEL/Fedora
dnf info gsdb
```

> AUR search indexing can take a few minutes after pushing. Direct install works immediately: `yay -S gsdb-bin`

---

## apt/yum Repository (GitHub Pages)

Packages are automatically published to `https://ginkcode.github.io/gsdb/` by CI after each release tag. The CI pushes to the [`ginkcode/ginkcode.github.io`](https://github.com/ginkcode/ginkcode.github.io) repo, which GitHub Pages serves at `https://ginkcode.github.io/`.

### One-time setup (required before first release)

**1. Generate a GPG signing key:**

```bash
gpg --full-generate-key
# Choose: RSA and RSA, 4096 bits, 0 = no expiry
```

**2. Get your key ID:**

```bash
gpg --list-secret-keys --keyid-format=long
# Note the 16-char hex ID after "rsa4096/"
```

**3. Add GitHub Secrets to the `gsdb` repo** (`Settings → Secrets and variables → Actions`):

```bash
# GPG_PRIVATE_KEY — paste the full armor output
gpg --armor --export-secret-keys YOUR_KEY_ID

# GPG_KEY_ID — the 16-char hex ID
# GPG_PASSPHRASE — your key's passphrase (leave empty if none)
```

**4. Create a Personal Access Token for cross-repo push:**
- GitHub → **Settings** → **Developer settings** → **Personal access tokens** → **Fine-grained tokens**
- Repository access: `ginkcode/ginkcode.github.io` only
- Permissions: **Contents → Read and write**
- Add it as `RELEASE_PAGES_TOKEN` secret in the `gsdb` repo

**5. Enable GitHub Pages on `ginkcode/ginkcode.github.io`:**
`Settings → Pages → Source: Deploy from branch → main / root`

### User install instructions

**Debian/Ubuntu:**
```bash
curl -fsSL https://ginkcode.github.io/gsdb/gpg.key \
  | sudo gpg --dearmor -o /usr/share/keyrings/gsdb.gpg
echo "deb [signed-by=/usr/share/keyrings/gsdb.gpg] https://ginkcode.github.io/gsdb/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/gsdb.list
sudo apt update
sudo apt install gsdb
```

**RHEL/Fedora:**
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

---

## AUR Packages

| Package | Path |
|---|---|
| `gsdb` | `/home/gink/Workspaces/learning/tools/gsdb` |
| `gsdb-bin` | `/home/gink/Workspaces/learning/tools/gsdb-bin` |

| Package | Description | Build Dependencies | Install Time |
|---|---|---|---|
| `gsdb` | Build from source | rust, nodejs, npm | ~5-10 min |
| `gsdb-bin` | Pre-built binary | none | ~30 sec |

Users should prefer `gsdb-bin` for faster installation.

### Initial AUR setup (one-time)

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

### AUR file locations

| File | Purpose |
|---|---|
| `pkg/aur/PKGBUILD` | Build-from-source package |
| `pkg/aur/PKGBUILD-bin` | Pre-built binary package |
| `pkg/aur/.SRCINFO` | Generated metadata for `gsdb` |
| `pkg/aur/.SRCINFO-bin` | Generated metadata for `gsdb-bin` |
| `pkg/aur/gsdb.desktop` | Desktop entry file |

---

## Runtime Dependencies

- webkit2gtk-4.1
- gtk3
- libsoup3
- openssl
- libssh2
- libsecret

**Build (`gsdb` AUR package only):** rust, nodejs, npm

---

## Troubleshooting

### Build fails with CARGO_TARGET_DIR error

The PKGBUILD explicitly sets `--target-dir target` so the binary is always at `src-tauri/target/release/gsdb` regardless of the user's `CARGO_TARGET_DIR` environment variable.

### UI appears unstyled / missing CSS (AUR source build only)

**Symptom:** The app launches but shows plain unstyled HTML.

**Root cause:** Tailwind CSS v4 uses `.git` to locate the project root for source detection. AUR tarballs have no `.git`, so Tailwind falls back to a minimal scan.

**Fix:** `src/routes/layout.css` contains an explicit `@source "../";` directive. If CSS looks too small in an AUR build, verify this directive is still present.

### AppImage download fails in gsdb-bin

Ensure the GitHub release exists and that the version in `PKGBUILD-bin` matches the release tag. Run `make release` before `make aur-deploy`.
