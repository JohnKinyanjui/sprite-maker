# Changelog

## Unreleased

## 0.3.2 — 2026-08-19

### Animation reliability

- Fixed deictic animation requests such as “animate this” so the selected asset's identity determines the correct creature or character harness, even when an older asset is stored in the wrong category folder.
- Prevented restored or unchanged generation manifests from attaching stale artwork to a new completed message.
- Treated provider-reported generation failures as failures instead of completed sprite cards, while preserving the best valid multi-frame result when only a visual warning remains.
- Rejected single-frame fallbacks as completed animations and used the renderer manifest's actual frame rate for accepted animation previews.
- Updated creature animation guidance to make one motion-ready source repair, then publish the best connected multi-frame attempt with a concise warning when needed.

## 0.3.1 — 2026-08-19

### Terrain and project recovery

- Added safe project backup, restore, and import workflows that preserve rigs and current project data.
- Added archived-chat recovery, asset-version restoration, Terrain Studio, canonical terrain-mask generation, and Godot 4 TileSet export.
- Removed terrain generation's dependency on legacy worktree types so the current request determines the correct asset category and output folder.
- Made Terrain and VFX tools available from every project section and added a production-ready desert terrain atlas.

### Distribution

- Restored tag-triggered GitHub release builds for Windows and Linux while keeping costly macOS builds off GitHub-hosted runners.
- Added local universal macOS build and upload commands covering both Apple Silicon and Intel Macs.
- Fixed the local universal macOS command so Tauri combines the two supported Rust architecture builds correctly.

## 0.3.0 — 2026-08-17

### Community providers

- Added polished provider, Custom API, and image-provider settings with actionable installation and authentication status.
- Added real streamed Claude Code, Gemini CLI, and Grok CLI adapters alongside the existing Codex integration, including cancellation and clear terminal errors.
- Added configurable OpenAI-compatible endpoints and Grok image credentials without exposing API keys in logs or the interface.
- Added official provider artwork and model switching directly in chat and recent-conversation rows.

### Faster, simpler projects

- Reworked project and chat loading so expanding a project no longer blocks on repeated database reads.
- Restored completed generated assets immediately without requiring an application restart.
- Made revisions version-aware so users can continue refining a sprite in the same project and chat; a fresh worktree is no longer required for each change.
- Removed provider discovery's dependence on the visible Codex application name, restoring compatibility with the current ChatGPT/Codex CLI installation.

### Rigging and animation quality

- Added a native point-and-bone rig editor with anatomy templates, AI point suggestions, deterministic rendering, and planted-contact IK.
- Added silent visual acceptance with one automatic repair attempt before a result is published.
- Added hard locomotion gates for rigid-body runs, repeated or near-static poses, missing flight phases, limb identity, wing attachment, quadruped limb range, and extended-versus-gathered gallop silhouettes.
- Changed the default generation canvas to 128×128 while preserving user-defined custom dimensions.

### Distribution

- Replaced the paid four-platform GitHub Actions release build with maintainer-built, locally verified release assets uploaded directly to GitHub.
- Releases now publish native installers and executables directly instead of ZIP wrappers.

## 0.2.3 — 2026-08-10

### Asset packs

- Removed the 12-item pack ceiling. Explicit pack counts are now treated as hard deliverables and larger packs are generated in visually consistent batches under one manifest.
- Added a grouped asset-pack component directly in chat instead of presenting pack items as animation frames or raw workspace links.
- Kept users in the active chat when pack generation completes; the pack library opens only through an intentional **View sprites** action.
- Made pack recovery reliable when multiple chats generate concurrently by associating completed messages with the pack created by that request.

### Browsing and animation handoff

- Added a dedicated per-pack sprite grid with search and independent asset selection, avoiding the grouped-animation route.
- Added zoomable inspection, individual sprite downloads, and an explicit **Animate in chat** handoff for every pack item.
- Fixed weapon names such as `battle_axe` being mistaken for bats and receiving flying motion suggestions.

### Quality

- Added regression coverage for grouped pack-message recovery.
- Verified the pack grid, sprite viewer, and animation handoff against a generated eight-item armour and weapons pack.

## 0.2.1 — 2026-08-10

### Fixed

- Restored playable inline sprite and animation cards when parallel generation changed the shared latest-generation manifest before chat metadata could be attached.
- Replaced unhelpful workspace folder and preview links with the existing in-chat sprite component whenever generated assets can be identified.
- Fixed inline animation playback stopping after a single frame; previews now loop continuously at the animation's configured FPS.
- Prevented relative workspace links in Markdown from navigating the desktop webview to a 404 page.
- Added safe handling for external, relative, absolute, encoded, macOS, Linux, and Windows Markdown links.

### Quality

- Added frontend regression tests for generation-card recovery and Markdown link routing.
- Added the frontend test suite to continuous integration.

## 0.2.0 — 2026-08-10

Sprite Studio 0.2 turns the original sprite experiment into a practical local-first game-art workbench.

### Create and organize

- Added dedicated creation harnesses for characters, creatures, game objects, effects, complete terrain atlases, and coordinated asset packs.
- Added `/pack`, a Packs tab, pack manifests, pack-aware sprite filtering, and pack preview cards.
- Added more built-in art directions, including limited-palette, one-bit, isometric pixel, cel-shaded, and painterly fantasy styles.
- Added workspace- and chat-level style choices with visual thumbnails.
- Terrain generation now creates one complete atlas PNG instead of registering every region as an unrelated sprite.

### Animate with better mechanics

- Rebuilt animation around AI-proposed rigs and masks plus deterministic frame rendering.
- Added anatomy-aware movement suggestions when choosing **Animate this**.
- Added explicit stable regions, moving body parts, pivots, overlap, z-order, support phases, and loop-closure requirements.
- Added physical motion envelopes using real-world scale, speed, displacement, height, gravity, and contact estimates unless the user supplies their own values.
- Added rig-only, recommended AI-polish, and experimental full-redraw finishing modes.
- Added regional AI repair for difficult joints and poses while retaining the planned motion and source identity.
- Increased automatic frame planning to a configurable 1–32 frame range. Auto remains the default.
- Enabled deterministic interpolation by default and replaced generation checkboxes with accessible switches.

### Improve the desktop workflow

- Added a full-size sprite viewer with zoom in, zoom out, actual-size reset, wheel zoom, metadata, and reveal-on-disk.
- Added playable sprite and animation cards directly inside Markdown chat messages.
- Grouped animation frames into one library item with a frame-count badge.
- Added clipboard paste, file upload, and drag-and-drop for reference images.
- Made focused references chat-local and removable; new chats no longer inherit a forced master image.
- Added concurrent per-chat generation with loading indicators in both the chat and sidebar.
- Added immediate worktree switching, simplified worktree creation, chat rename dialogs, and hover-to-archive actions.
- Added provider capability discovery for models, reasoning levels, image input, multi-reference input, structured output, and transparency.

### Fixes and polish

- Fixed pack mosaic images escaping their preview cells and overlapping titles or descriptions.
- Fixed cramped sprite cards and unreadable pack filters at narrower window sizes.
- Improved dark theme colors, typography, dialog sizing, empty states, and asset browsing.
- Added a help dialog that explains every generation control.
- Improved deterministic validation for alpha, dimensions, duplicates, alignment, palette, continuity, physical plausibility, and seamless loops.

### Platforms

- Native release builds for Apple Silicon macOS, Intel macOS, Windows, and Linux.
- Improved Codex executable discovery on macOS application launches.
