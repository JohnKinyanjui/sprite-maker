.DEFAULT_GOAL := help

BUN ?= bun
TAURI := $(BUN) run tauri
VERSION := $(shell node -p "require('./package.json').version")
RELEASE_DIR ?= release-artifacts/$(VERSION)
BUNDLE_DIR := src-tauri/target/release/bundle
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

.PHONY: help install check test native-test verify bundle collect release macos linux windows

help:
	@printf '%s\n' \
	  'Sprite Studio local release commands:' \
	  '  make install       Install locked JavaScript dependencies.' \
	  '  make verify        Run frontend and Rust checks.' \
	  '  make bundle        Build native bundles for this machine.' \
	  '  make release       Verify, build, and collect this machine’s installers.' \
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
	  found=0; \
	  for artifact in $(BUNDLE_DIR)/appimage/*.AppImage $(BUNDLE_DIR)/deb/*.deb $(BUNDLE_DIR)/rpm/*.rpm; do \
	    [ -f "$$artifact" ] || continue; \
	    cp "$$artifact" "$$destination/"; \
	    found=1; \
	  done; \
	  [ "$$found" -eq 1 ] || { printf '%s\n' 'No Linux installers were produced by Tauri.' >&2; exit 1; }; \
	  printf 'Collected Linux artifacts in %s\n' "$$destination"

windows:
	@case "$(HOST_OS)" in MINGW*|MSYS*|CYGWIN*) ;; *) printf '%s\n' 'Windows bundles must be built on Windows.' >&2; exit 1;; esac
	@set -eu; \
	  destination='$(RELEASE_DIR)/windows'; \
	  mkdir -p "$$destination"; \
	  found=0; \
	  for artifact in $(BUNDLE_DIR)/nsis/*.exe $(BUNDLE_DIR)/msi/*.msi; do \
	    [ -f "$$artifact" ] || continue; \
	    cp "$$artifact" "$$destination/"; \
	    found=1; \
	  done; \
	  [ "$$found" -eq 1 ] || { printf '%s\n' 'No Windows installers were produced by Tauri.' >&2; exit 1; }; \
	  printf 'Collected Windows artifacts in %s\n' "$$destination"
