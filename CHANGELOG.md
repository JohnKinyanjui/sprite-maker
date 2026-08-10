# Changelog

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
