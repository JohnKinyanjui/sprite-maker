# How other tools solve AI sprite problems — and what Sprite Studio integrates

This document compares industry approaches and maps them to Sprite Studio features.

## Problem matrix

| Problem | Industry pattern | Sprite Studio |
| --- | --- | --- |
| Strip cells bleed / uneven gutters | **Profile / auto slice** (alpha projection valleys, DP cuts) — [PerfectPixel](https://pp.andrew-ai.app/), [Picasso](https://github.com/jacklenzotti/picasso) `--auto` | **`score_strip`** pre-QC; `split_strip` layout **`profile`** or **`auto`** (profile gutters + DP fallback); warns on **grid ink** |
| Character jitters left/right when limbs move | **Alpha-weighted centroid** alignment, not bbox center — PerfectPixel, Spriterrific | `normalize_animation` pins **centroid X** to anchor pivot |
| Feet slide / baseline drift | **Foot pivot + baseline** from lowest opaque pixel — game engines, LPC tooling | Anchor `baseline_y` from **alpha foot**; contract checks ±1px |
| Identity drift across frames | **dHash / histogram** vs reference — PerfectPixel `ScoreFrames` | Size contract: **`identity_drift`** (dHash distance vs anchor master) |
| “Animation” that does not move | **Motion presence** metric — PerfectPixel | Contract: **`motion_still`** when adjacent frames are nearly identical |
| AI draws visible cell borders | **Sheet QC grid check** — vibedgames `sheet-qc` | Strip warnings when **grid ink** detected; regenerate strip |
| Wrong equal-width split | Prefer auto over fixed grid — Picasso, Scenario video workflow | Default UI import uses **`profile`**; equal `horizontal` still available |
| Video → sprite frames | **ffmpeg extract** at fixed FPS — Scenario strip-first | MCP/UI **`extract_video_frames`** → `assets/imports/` then normalize |
| Volume / many animation states | **Custom model / reference lock** — Scenario, PixelLab | Chat references + **promoted anchor** + rig-native `/animate` |
| Non-destructive micro-fixes | **Metadata offsets** — Spriterrific frame-aligner, sprite-gen curation | **Align** panel (`offsetX/Y`) + Playground preview |
| Weak transition frames | **In-between from motion** — rig pose interpolation, optical flow (deferred in many tools) | **Motion-aware midpoint** (`optimize_animation_frames`); **rig bridge** when `rigId` is set; **rig-rendered in-betweens** (`interpolate_rig_animation_frames`) |
| Dense video import noise | **Keyframe picker / subsample** — Scenario, manual curation | **`extract_video_frames`** + **`subsample_video_frames`** + wizard step 4 filmstrip |
| Regional AI repair mask | **Brush / rect mask** — PixelLab-style inpainting regions | **`queue_region_regen`** with rect + **`brushStrokes`**; `FrameMaskOverlay` on the animation preview |
| Shared palette drift | **Palette lock / quantize** — PerfectPixel | **`quantize_worktree_palette`** (8/16/32) distinct from grid snap |
| Manual alpha cleanup | **Eraser with version history** | **`paint_frame_alpha`** (`erase` / `restore`) + `restore_asset_version` |
| Engine-ready handoff | **Manifest with rects + pivot** — sprite-gen, TexturePacker | Export JSON with `trimOffset`, `spriteSourceSize`; sprite-sheet job |
| Multi-direction sets (4/8-way) | **Facing check + mirror derive** — PerfectPixel (5 gen + 3 mirror), Spriterrific `facing-check.json` | **`check_anchor_facing`** / `orient_anchor`; **`mirror_animation`**; **`queue_direction_set`**; cross-facing **`check_character_contract`** |

## Gap inventory G18–G29

| ID | Priority | Capability | Status |
| --- | --- | --- | --- |
| G18 | P1 | Motion catalog UX — categories, expanded presets, generate all missing | Shipped (`MotionBatchPanel`, `queue_motion_batch` `onlyMissing`) |
| G19 | P1 | Animated GIF preview export (single animation + character pack) | Shipped (`export_gif_preview`, pack `includeAnimatedPreviews`) |
| G20 | P1 | Production score gate before pack export | Shipped (`get_production_score`, pack export guard) |
| G21 | P1 | MCP tool names aligned with pipeline orchestration | Shipped (`harden_animation`, `queue_motion_batch`, etc.) |
| G22 | P2 | Visual region mask (brush + rect) for regional regen | Shipped (`FrameMaskOverlay`, `queue_region_regen.brushStrokes`) |
| G23 | P2 | Dense video import curation (subsample + filmstrip picker) | Shipped (`subsample_video_frames`, `VideoImportWizard` step 4) |
| G24 | P2 | Game-view anchor presets (platformer side, top-down, RTS oblique) | Shipped (`PromoteAnchorDialog`, `normalize_view` aliases) |
| G25 | P2 | Worktree shared-palette quantization (8/16/32) | Shipped (`quantize_worktree_palette`, harden palette option) |
| G26 | P2 | Non-destructive alpha cleanup (eraser + version restore) | Shipped (`paint_frame_alpha`, `restore_asset_version`) |
| G27 | P3 | Rig-rendered pixel in-between frames in animation timeline | Shipped (`interpolate_rig_animation_frames`, Rig editor UI) |
| G28 | P3 | Production session gallery (generation archive per worktree) | Shipped (`ProductionSessionsGallery`, Media tab) |
| G29 | P3 | Optical flow Path B — permanent deferral | Deferred (no feature flag; use rig bridge + motion midpoint + G27) |

## Recommended Sprite Studio workflow (production)

1. Generate **one master** → **Promote anchor** (foot + centroid measured from pixels).
2. Generate **one horizontal strip** per action (not independent frames).
3. **`score_strip`** then **`split_strip`** with `layout: "profile"` (or `"auto"` / `frameCount: 0`), `recoverForeground: true`.
4. Save animation → **`normalize_animation`** (`lockFirstFrame`, `sharedScale`).
5. **`align_frames`** for 1–2 px nudges if needed.
6. **`check_size_contract`** (canvas, baseline, identity, centroid, motion).
7. On failure: **`queue_contract_retry`** (autonomous repair + AI regen + import) or manual **`retry_size_contract`** / **`finalize_contract_retry`**.
8. **`set_animation_review_status` accepted** → export.

## Multi-facing workflow (4/8 directions)

1. **Promote anchor** with `autoOrient: true` (side view canonical **W**).
2. **`check_anchor_facing`** before walk/run strips — fix mismatches with **`orient_anchor`** or UI “Flip to canonical (W)”.
3. Generate canonical facing only (e.g. walk-**w**), then **`mirror_animation`** or **`harden_animation`** with `options.deriveMirroredFacing: "e"`.
4. For full sets, **`queue_direction_set`** (`set: "4"` or `"8"`) mirrors derivable facings and runs **`check_character_contract`** (includes `facing_identity_drift` / `facing_baseline_mismatch`).
5. Export is blocked until the worktree direction family passes cross-facing contract when direction metadata is present.

## Transition repair (Phase 5)

`optimize_animation_frames` / `regenerate_transition` now:

1. **Rig bridge** — when `.sprite-studio/last-generation.json` carries a `rigId`, transition repairs render a midpoint rig pose instead of RGBA blending.
2. **Motion-aware midpoint** — otherwise offsets are aligned to the halfway metadata position before pixel blend.
3. **Rig-rendered in-betweens** — `interpolate_rig_animation_frames` renders midpoint poses into PNG frames and inserts them into an animation (complements pose-only `interpolate_rig_frames`).
4. **Optical flow** — **permanently deferred (G29 Path B)**. No feature flag; use rig bridge, motion-aware midpoint, or rig-rendered in-betweens instead.

## What we deliberately do not automate yet

| Technique | Why deferred |
| --- | --- |
| Cloud-only character training (Scenario / PixelLab) | Out of scope for local-first OSS; use references + anchor instead |
| Optical-flow in-between frames | Permanently deferred (G29 Path B); rig bridge + motion-aware midpoint + rig-rendered in-betweens |
| PerfectPixel-style perceptual frame scoring | `score_strip` covers count inference + grid ink; full perceptual hash scoring deferred |
| Skeleton inpainting UI (PixelLab) | Rig editor + native render covers deterministic path |

## References

- [PerfectPixel](https://pp.andrew-ai.app/) — centroid alignment, profile split, dHash scoring
- [Spriterrific](https://pypi.org/project/spriterrific/) — anchor, size contract, frame aligner
- [sprite-gen](https://github.com/bokjk/sprite-gen) — chroma recovery, manifest layout, curation offsets
- [Picasso](https://github.com/jacklenzotti/picasso) — auto slice vs grid, pipeline JSON
- [Scenario — AI sprite generator](https://www.scenario.com/blog/ai-sprite-generator) — strip-first vs frame-by-frame video
- [SEELE — frame consistency](https://www.seeles.ai/resources/blogs/how-we-create-ai-sprite-sheets) — reference-locking narrative
