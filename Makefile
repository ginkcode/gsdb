.PHONY: help build release clean version tag push-tag push aur aur-deploy icons

# Current version from package.json
VERSION := $(shell node -p "require('./package.json').version")
# Explicitly set cargo target dir to avoid issues with CARGO_TARGET_DIR env var
CARGO_TARGET_DIR := $(HOME)/.cargo_cache
# AUR repository paths
AUR_GSDB_DIR := /home/gink/Workspaces/learning/tools/gsdb
AUR_GSDB_BIN_DIR := /home/gink/Workspaces/learning/tools/gsdb-bin

help:
	@echo "GSDB Makefile"
	@echo ""
	@echo "Usage:"
	@echo "  make icons          - Regenerate all app icons from SVG sources"
	@echo "  make build          - Build the application (binary, deb, rpm, appimage)"
	@echo "  make release        - Push commits, tag, and create a GitHub release with AppImage"
	@echo "  make aur-deploy     - Copy AUR files to local gsdb/gsdb-bin repos"
	@echo "  make version        - Show current version"
	@echo "  make tag            - Create a git tag for current version"
	@echo "  make push-tag       - Push tag to remote"
	@echo "  make push           - Push commits to remote"
	@echo ""
	@echo "Version management:"
	@echo "  make set-version V=x.x.x  - Set a new version"
	@echo ""
	@echo "Current version: $(VERSION)"

icons:
	@echo "Generating icons..."
	@# Generate macOS icns from Apple Icon Composer PNG
	npx tauri icon src-tauri/icons/mac-icon-macOS-Default-1024x1024@1x.png -o /tmp/gsdb-mac-icons
	@# Generate Linux/Windows icons from gsdb.svg (transparent background)
	npx tauri icon src-tauri/icons/gsdb.svg
	@# Restore the macOS-specific icns
	cp /tmp/gsdb-mac-icons/icon.icns src-tauri/icons/icon.icns
	@echo "Icons generated."
	@echo "  macOS:   src-tauri/icons/icon.icns  (from mac-icon-macOS-Default-1024x1024@1x.png)"
	@echo "  Windows: src-tauri/icons/icon.ico   (from gsdb.svg)"
	@echo "  Linux:   src-tauri/icons/*.png      (from gsdb.svg)"

version:
	@echo "Current version: $(VERSION)"

build:
	@echo "Building GSDB v$(VERSION)..."
	npm ci
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) npm run tauri:build
	@echo ""
	@echo "Build complete!"
	@echo "  Binary:   $(CARGO_TARGET_DIR)/release/gsdb"
	@echo "  DEB:      $(CARGO_TARGET_DIR)/release/bundle/deb/GSDB_$(VERSION)_amd64.deb"
	@echo "  RPM:      $(CARGO_TARGET_DIR)/release/bundle/rpm/GSDB-$(VERSION)-1.x86_64.rpm"
	@echo "  AppImage: $(CARGO_TARGET_DIR)/release/bundle/appimage/GSDB_$(VERSION)_amd64.AppImage"

tag:
	@echo "Creating git tag v$(VERSION)..."
	git tag -a v$(VERSION) -m "Release v$(VERSION)"

push-tag:
	@echo "Pushing tag v$(VERSION) to remote..."
	git push origin v$(VERSION)

push:
	@echo "Pushing commits to remote..."
	git push origin main

release: build push tag push-tag
	@echo "Creating GitHub release v$(VERSION)..."
	gh release create v$(VERSION) \
		$(CARGO_TARGET_DIR)/release/bundle/appimage/GSDB_$(VERSION)_amd64.AppImage \
		--title "GSDB v$(VERSION)" \
		--notes "Release v$(VERSION) of GSDB - Database Management Tool"
	@echo ""
	@echo "Release created at: https://github.com/ginkcode/gsdb/releases/tag/v$(VERSION)"

set-version:
ifndef V
	@echo "Error: Please specify version with V=x.x.x"
	@exit 1
endif
	@echo "Setting version to $(V)..."
	@# Update package.json
	sed -i 's/"version": ".*"/"version": "$(V)"/' package.json
	@# Update src-tauri/Cargo.toml
	sed -i 's/^version = ".*"/version = "$(V)"/' src-tauri/Cargo.toml
	@# Update src-tauri/tauri.conf.json
	sed -i 's/"version": ".*"/"version": "$(V)"/' src-tauri/tauri.conf.json
	@# Update PKGBUILD
	sed -i 's/pkgver=.*/pkgver=$(V)/' pkg/aur/PKGBUILD
	sed -i 's/pkgver=.*/pkgver=$(V)/' pkg/aur/PKGBUILD-bin
	sed -i 's/pkgrel=.*/pkgrel=1/' pkg/aur/PKGBUILD
	sed -i 's/pkgrel=.*/pkgrel=1/' pkg/aur/PKGBUILD-bin
	@# Regenerate .SRCINFO files from updated PKGBUILDs
	$(MAKE) aur
	@echo "Version updated to $(V)"
	@echo "Don't forget to:"
	@echo "  1. git add ."
	@echo "  2. git commit -m 'chore: bump version to $(V)'"
	@echo "  3. make release"

clean:
	@echo "Cleaning build artifacts..."
	rm -rf build/
	rm -rf .svelte-kit/
	rm -rf src-tauri/target/
	rm -rf $(CARGO_TARGET_DIR)/release/
	rm -rf node_modules/
	@echo "Clean complete!"

aur:
	@echo "Updating AUR .SRCINFO files..."
	cd pkg/aur && makepkg --printsrcinfo > .SRCINFO
	cd pkg/aur && cp PKGBUILD PKGBUILD.bak && cp PKGBUILD-bin PKGBUILD && makepkg --printsrcinfo > .SRCINFO-bin && mv PKGBUILD.bak PKGBUILD
	@echo "AUR files updated."

aur-deploy: aur
	@echo "Deploying to AUR repos..."
	cp pkg/aur/PKGBUILD $(AUR_GSDB_DIR)/PKGBUILD
	cp pkg/aur/.SRCINFO $(AUR_GSDB_DIR)/.SRCINFO
	cp pkg/aur/gsdb.desktop $(AUR_GSDB_DIR)/gsdb.desktop
	cp pkg/aur/PKGBUILD-bin $(AUR_GSDB_BIN_DIR)/PKGBUILD
	cp pkg/aur/.SRCINFO-bin $(AUR_GSDB_BIN_DIR)/.SRCINFO
	@echo "Deployed v$(VERSION) to AUR repos."
	@echo "  gsdb:     $(AUR_GSDB_DIR)"
	@echo "  gsdb-bin: $(AUR_GSDB_BIN_DIR)"
	@echo "Next: cd into each repo, git add -A, git commit, git push"

install:
	@echo "Installing the binary to /usr/bin..."
	sudo install -Dm755 $(CARGO_TARGET_DIR)/release/gsdb /usr/bin/gsdb
	sudo install -Dm644 pkg/aur/gsdb.desktop /usr/share/applications/gsdb.desktop
	@echo "Installing icons..."
	sudo install -Dm644 src-tauri/icons/32x32.png /usr/share/icons/hicolor/32x32/apps/gsdb.png
	sudo install -Dm644 src-tauri/icons/128x128.png /usr/share/icons/hicolor/128x128/apps/gsdb.png
	sudo install -Dm644 src-tauri/icons/128x128@2x.png /usr/share/icons/hicolor/256x256/apps/gsdb.png
	sudo install -Dm644 src-tauri/icons/icon.png /usr/share/icons/hicolor/512x512/apps/gsdb.png
	@echo "Updating icon cache..."
	sudo gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
	@echo "Installation complete! Run 'gsdb' to start the application."