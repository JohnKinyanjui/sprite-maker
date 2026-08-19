.DEFAULT_GOAL := help

BUN ?= bun
GH ?= gh
TAURI := $(BUN) run tauri
VERSION := $(shell node -p "require('./package.json').version")
TAG ?= v$(VERSION)
RELEASE_DIR ?= release-artifacts/$(VERSION)
BUNDLE_DIR := src-tauri/target/release/bundle
MACOS_TARGET := universal-apple-darwin
MACOS_BUNDLE_DIR := src-tauri/target/$(MACOS_TARGET)/release/bundle
HOST_OS := $(shell uname -s)
HOST_ARCH := $(shell uname -m)

ifeq ($(HOST_OS),Darwin)
  PLATFORM := macos
  ifeq ($(HOST_ARCH),arm64)
    ARCH_LABEL := aarch64
  else
    ARCH_LABEL := $(HOST_ARCH)
  endif
else ifeq ($(HOST_OS),Linux)
  PLATFORM := linux
  ifeq ($(HOST_ARCH),x86_64)
    ARCH_LABEL := amd64
  else
    ARCH_LABEL := $(HOST_ARCH)
  endif
else ifneq (,$(filter MINGW% MSYS% CYGWIN%,$(HOST_OS)))
  PLATFORM := windows
  ARCH_LABEL := x64
else
  $(error Unsupported host '$(HOST_OS)'. Build releases on macOS, Windows, or Linux.)
endif

.PHONY: help install check test native-test verify bundle collect release release-macos publish-macos macos linux windows

help:
	@printf '%s\n' \
	  'Sprite Studio local release commands:' \
	  '  make install       Install locked JavaScript dependencies.' \
	  '  make verify        Run frontend and Rust checks.' \
	  '  make bundle        Build native bundles for this machine.' \
	  '  make release       Verify, build, and collect this machine’s installers.' \
	  '  make release-macos Build one universal Intel + Apple Silicon macOS release.' \
	  '  make publish-macos Build macOS locally and upload it to TAG (default: v<version>).' \
	  '' \
	  'Artifacts are collected under release-artifacts/<version>/<platform>.' \
	  'Build each platform on its native host: macOS for DMG/app archive, Windows for EXE/MSI, Linux for AppImage/DEB/RPM.'

install:
	$(BUN) install --frozen-lockfile

check:
	$(BUN) run check

test:
	$(BUN) test

native-test:
	cargo test --manifest-path src-tauri/Cargo.toml

verify: check test native-test

bundle: verify
	$(TAURI) build

collect: bundle $(PLATFORM)

release: collect
	@printf 'Local release artifacts are ready in %s\n' '$(RELEASE_DIR)/$(PLATFORM)'

release-macos: verify
	@test "$(HOST_OS)" = Darwin || { printf '%s\n' 'Universal macOS bundles must be built on macOS.' >&2; exit 1; }
	rustup target add aarch64-apple-darwin x86_64-apple-darwin
	$(TAURI) build --target $(MACOS_TARGET)
	$(MAKE) macos BUNDLE_DIR='$(MACOS_BUNDLE_DIR)' ARCH_LABEL=universal
	@printf 'Universal macOS artifacts are ready in %s\n' '$(RELEASE_DIR)/macos'

publish-macos: release-macos
	@command -v $(GH) >/dev/null || { printf '%s\n' 'Install and authenticate GitHub CLI before publishing.' >&2; exit 1; }
	@$(GH) release view '$(TAG)' >/dev/null || { printf 'GitHub release %s does not exist yet. Wait for the Windows/Linux workflow to finish.\n' '$(TAG)' >&2; exit 1; }
	$(GH) release upload '$(TAG)' '$(RELEASE_DIR)/macos/'*.dmg '$(RELEASE_DIR)/macos/'*.app.tar.gz --clobber
	@printf 'Uploaded universal macOS artifacts to GitHub release %s\n' '$(TAG)'

macos:
	@test "$(HOST_OS)" = Darwin || { printf '%s\n' 'macOS bundles must be built on macOS.' >&2; exit 1; }
	@set -eu; \
	  destination='$(RELEASE_DIR)/macos'; \
	  mkdir -p "$$destination"; \
	  found=0; \
	  for artifact in $(BUNDLE_DIR)/dmg/*.dmg; do \
	    [ -f "$$artifact" ] || continue; \
	    cp "$$artifact" "$$destination/"; \
	    found=1; \
	  done; \
	  [ "$$found" -eq 1 ] || { printf '%s\n' 'No DMG was produced by Tauri.' >&2; exit 1; }; \
	  app='$(BUNDLE_DIR)/macos/Sprite Studio.app'; \
	  [ -d "$$app" ] || { printf '%s\n' 'No macOS .app bundle was produced by Tauri.' >&2; exit 1; }; \
	  tar -C '$(BUNDLE_DIR)/macos' -czf "$$destination/Sprite.Studio_$(VERSION)_$(ARCH_LABEL).app.tar.gz" 'Sprite Studio.app'; \
	  printf 'Collected macOS artifacts in %s\n' "$$destination"

linux:
	@test "$(HOST_OS)" = Linux || { printf '%s\n' 'Linux bundles must be built on Linux.' >&2; exit 1; }
	@set -eu; \
	  destination='$(RELEASE_DIR)/linux'; \
	  mkdir -p "$$destination"; \
	  appimage=$$(find '$(BUNDLE_DIR)/appimage' -maxdepth 1 -type f -name '*.AppImage' -print -quit 2>/dev/null || true); \
	  deb=$$(find '$(BUNDLE_DIR)/deb' -maxdepth 1 -type f -name '*.deb' -print -quit 2>/dev/null || true); \
	  rpm=$$(find '$(BUNDLE_DIR)/rpm' -maxdepth 1 -type f -name '*.rpm' -print -quit 2>/dev/null || true); \
	  [ -n "$$appimage" ] && [ -n "$$deb" ] && [ -n "$$rpm" ] || { printf '%s\n' 'Tauri must produce an AppImage, DEB, and RPM.' >&2; exit 1; }; \
	  cp "$$appimage" "$$deb" "$$rpm" "$$destination/"; \
	  printf 'Collected Linux artifacts in %s\n' "$$destination"

windows:
	@case "$(HOST_OS)" in MINGW*|MSYS*|CYGWIN*) ;; *) printf '%s\n' 'Windows bundles must be built on Windows.' >&2; exit 1;; esac
	@set -eu; \
	  destination='$(RELEASE_DIR)/windows'; \
	  mkdir -p "$$destination"; \
	  exe=$$(find '$(BUNDLE_DIR)/nsis' -maxdepth 1 -type f -name '*.exe' -print -quit 2>/dev/null || true); \
	  msi=$$(find '$(BUNDLE_DIR)/msi' -maxdepth 1 -type f -name '*.msi' -print -quit 2>/dev/null || true); \
	  [ -n "$$exe" ] && [ -n "$$msi" ] || { printf '%s\n' 'Tauri must produce both an NSIS EXE and an MSI.' >&2; exit 1; }; \
	  cp "$$exe" "$$msi" "$$destination/"; \
	  printf 'Collected Windows artifacts in %s\n' "$$destination"
