#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
mkdir -p "$BIN_DIR"

host_triple="$(rustc -vV | sed -n 's/^host: //p')"
universal_macos=0
if [[ "${1:-}" == "--universal-macos" ]]; then
  universal_macos=1
fi

fetch_linux() {
  local dest="$BIN_DIR/ffmpeg-x86_64-unknown-linux-gnu"
  if [[ -f "$dest" ]]; then
    echo "ffmpeg already present at $dest"
    return 0
  fi
  local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz"
  local tmp
  tmp="$(mktemp -d)"
  echo "Downloading ffmpeg for Linux..."
  curl -fsSL "$url" | tar -xJ -C "$tmp" --strip-components=1
  cp "$tmp/bin/ffmpeg" "$dest"
  chmod +x "$dest"
  rm -rf "$tmp"
  echo "Installed bundled ffmpeg to $dest"
}

fetch_macos_brew() {
  local arch="$1"
  local dest="$BIN_DIR/ffmpeg-${arch}-apple-darwin"
  if [[ -f "$dest" ]]; then
    echo "ffmpeg already present at $dest"
    return 0
  fi
  if ! command -v brew >/dev/null 2>&1; then
    echo "Homebrew is required to bundle ffmpeg on macOS. Install brew, then run: brew install ffmpeg" >&2
    exit 1
  fi
  brew list ffmpeg >/dev/null 2>&1 || brew install ffmpeg
  local brew_prefix
  brew_prefix="$(brew --prefix ffmpeg)"
  cp "$brew_prefix/bin/ffmpeg" "$dest"
  chmod +x "$dest"
  echo "Installed bundled ffmpeg to $dest"
}

fetch_macos_evermeet() {
  local arch="$1"
  local dest="$BIN_DIR/ffmpeg-${arch}-apple-darwin"
  if [[ -f "$dest" ]]; then
    echo "ffmpeg already present at $dest"
    return 0
  fi
  local tmp
  tmp="$(mktemp -d)"
  echo "Downloading ffmpeg ${arch} for macOS..."
  curl -fsSL "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip" -o "$tmp/ffmpeg.zip"
  unzip -oq "$tmp/ffmpeg.zip" -d "$tmp"
  cp "$tmp/ffmpeg" "$dest"
  chmod +x "$dest"
  rm -rf "$tmp"
  echo "Installed bundled ffmpeg to $dest"
}

fetch_macos_evermeet_x86() {
  fetch_macos_evermeet x86_64
}

fetch_macos_evermeet_arm64() {
  fetch_macos_evermeet aarch64
}

if [[ "$universal_macos" -eq 1 ]]; then
  fetch_macos_evermeet_arm64
  fetch_macos_evermeet_x86
  exit 0
fi

case "$host_triple" in
  x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu)
    fetch_linux
    ;;
  x86_64-apple-darwin)
    fetch_macos_evermeet_x86
    ;;
  aarch64-apple-darwin)
    fetch_macos_evermeet_arm64
    ;;
  *)
    echo "Unsupported host triple for bundled ffmpeg: $host_triple" >&2
    exit 1
    ;;
esac
