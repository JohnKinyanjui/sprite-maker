# Bundled ffmpeg binaries

Sprite Studio installers ship a private **ffmpeg** sidecar for video frame extraction.

These files are **not** committed to git. They are downloaded before release builds:

```bash
# Windows (PowerShell)
./scripts/fetch-ffmpeg.ps1

# macOS / Linux
./scripts/fetch-ffmpeg.sh
```

Expected filenames (Tauri `externalBin`):

| Target | File |
| --- | --- |
| Windows x64 | `ffmpeg-x86_64-pc-windows-msvc.exe` |
| Linux x64 | `ffmpeg-x86_64-unknown-linux-gnu` |
| macOS Apple Silicon | `ffmpeg-aarch64-apple-darwin` |
| macOS Intel | `ffmpeg-x86_64-apple-darwin` |

For universal macOS bundles, fetch both macOS binaries before `make release-macos`.
