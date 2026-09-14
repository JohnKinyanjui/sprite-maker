---
name: sprite-director
description: Infer, create, animate, and validate original pixel-art sprites for Sprite Studio. Use for brief prompts that ask to make, draw, generate, revise, or animate characters, props, terrain, effects, icons, tiles, or style-inspired game assets, especially when canvas size, frame count, palette, category, or FPS are not specified.
---

# Sprite Director router

Turn the user's intent into real, game-ready PNG files. The Rust harness routes the request by asset kind. Do not stop at a description or plan when creation is requested.

## Direct the asset

1. Preserve every explicit user constraint.
2. Infer only missing production details from `references/style-presets.md`.
3. Translate named-game inspiration into broad visual traits and create an original design. Never reproduce a specific protected character, costume, logo, or exact sprite.
4. Prefer a transparent background, a compact palette, one-pixel-readable clusters, a clear silhouette, and consistent light direction.
5. Use the harness brief supplied above the request as the default. Override it only when the user's explicit words conflict.

## Route

- `character`: follow `references/character-harness.md`. ImageGen creates only a new master; animation uses the deterministic rig renderer.
- `creature`: follow `references/creature-harness.md`. Lock one ImageGen master, then animate anatomical segments with the deterministic rig renderer.
- `effect`: follow `references/effect-harness.md`. ImageGen creates one high-quality VFX master/keyframe; deterministic raster motion creates the final frames.
- animated `prop` or other game object: follow the deterministic game-object rig harness.
- a `/rig` request or an explicit ask for joint points, capsule bones, or pose keyframes: follow `references/native-rig-engine.md`. Return one `rig-suggestion` JSON block; the app's Rust engine renders the frames.
- full `terrain tileset`: follow `references/terrain-tileset-harness.md` and return one atlas PNG, never one file per tile.
- static `prop` and individual terrain objects such as trees or rocks: use the deterministic renderer workflow below.

The routed harness name appears above the request. Do not substitute another harness.

## Deterministic renderer for non-character jobs

Write one JSON spec under `.sprite-studio/`, then run:

```bash
python3 .sprite-studio/sprite_tool.py .sprite-studio/<spec>.json
```

The spec supports `name`, `category`, `size`, `fps`, `palette`, and `frames`. A frame supports `pixels` or commands:

- `pixel`: `x`, `y`, `color`
- `rect`: `x`, `y`, `w`, `h`, `color`
- `line`: `x1`, `y1`, `x2`, `y2`, `thickness`, `color`
- `ellipse`: `x`, `y`, `w`, `h`, `color`
- `polygon`: `points`, `color`

Use `terrain`, `props`, or `effects` as the category. Character jobs must use the character harness and ImageGen instead.

## Native Rust rig renderer (preferred)

For any raster master that needs animation, use the **Sprite Studio MCP native rig path** — do not call `python3 .sprite-studio/sprite_rig.py` unless the user explicitly requests the legacy Python renderer for debugging.

1. **`suggest_rig_points`** or **`analyze_rig_fit`** on the master `assetId` to seed joints and morphology.
2. **`save_rig`** with points, bones, and motion frames bound to the master asset.
3. **`render_rig_animation`** to emit frame PNGs and a draft animation in the workspace.

The bundled Python `sprite_rig.py` remains for offline compatibility only; agents and chat flows should use MCP `save_rig` + `render_rig_animation`. ImageGen must never invent animation timing, poses, or pose sheets. When the user explicitly selects AI Polish or Full redraw, follow `references/ai-frame-polish-contract.md`: edit only completed rough rig frames before acceptance.

After **`promote_anchor`**, read **`get_character_profile`** for palette, dHash, linked `rigId`, and known `directionFamilies` before generating follow-up motions.

## Validate

Follow `references/quality-gates.md`. Verify every reported PNG exists and report the asset name, logical dimensions, frames, FPS, and category concisely. Do not expose internal spec paths unless troubleshooting.

## Post-generation pipeline (Sprite Studio MCP)

When animation frames come from AI strips or need production hardening, prefer the **single orchestrator** instead of ad-hoc Python crops:

1. **Promote anchor** — after the user approves a character master, call `promote_anchor` with `autoOrient: true` (side view) so `.sprite-studio/anchors/<slug>.json` becomes the size and foot-baseline authority. Run **`check_anchor_facing`** before locomotion strips; use **`orient_anchor`** if facing is not canonical **W**.
2. **Generate strip-first** — prefer one horizontal strip per action with a shared anchor silhouette; never independent frame generations for locomotion.
3. **`harden_animation`** — idempotent chain: optional `sourcePath` + `frameCount` split → `clean_alpha` → `normalize_animation` (lock-first + shared scale) → optional `snap_to_pixel_grid` → `check_size_contract` → optional `options.deriveMirroredFacing: "e"` after pass → optional `queue_contract_retry` when `options.queueContractRetry: true` and `conversationId` is set. Writes `.sprite-studio/last-generation.json` from the animation frames.
4. **Multi-facing RPG (8-dir)** — generate only **N, E, S, NE, NW**; derive **SE←NE, SW←NW, W←E** with **`mirror_animation`** or **`queue_direction_set`** (`set: "8"`). Finish with **`check_character_contract`** on the character worktree.
5. **Manual steps (only when debugging)** — `score_strip` before import; `split_strip` / `extract_video_frames`; `align_frames` for 1–2 px nudges; `retry_size_contract` + `finalize_contract_retry` instead of autonomous retry.
6. **Accept** — `check_size_contract` must pass, then `set_animation_review_status` with `accepted` before export or sprite-sheet jobs.
7. **Batch motions** — after the canonical west-facing walk passes contract, call **`queue_motion_batch`** (`onlyMissing: true` to skip motions already satisfied) and poll **`get_job`** per motion.
8. **Ship pack** — when the character worktree passes **`get_production_score`** and **`check_character_contract`**, call **`export_character_pack`** with a workspace-relative `destination` (set `includeAnimatedPreviews: true` for GIF previews).

Typical agent flow after frames exist: **`harden_animation`** → if `contractReport.passed` is false and a chat is available, call again with `options: { cleanAlpha: false, normalize: false, queueContractRetry: true }` (skip re-normalizing) → poll `get_job` → **`set_animation_review_status` accepted`** → optional **`queue_motion_batch`** → **`export_character_pack`**.

Other MCP tools: **`score_animation_frames`** (per-frame motion QC), **`list_facing_checks`** (anchor facing history), **`queue_region_regen`** (masked frame repair with rect regions and optional `brushStrokes`). Pose-only in-betweens: MCP **`interpolate_rig_frames`**. Rendered PNG in-betweens inserted into an animation timeline: **Tauri/UI command `interpolate_rig_animation_frames`** (not exposed on MCP — use the Rig editor or invoke the Rust command from the app). Optical flow is permanently deferred — use rig bridge, motion-aware midpoint, or rendered in-betweens instead.

Always write `.sprite-studio/last-generation.json` for static and animated outputs so the workbench can recover assets without a manual rescan.
