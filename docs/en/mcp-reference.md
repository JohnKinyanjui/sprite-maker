# Sprite Studio MCP reference

Headless server binary: `sprite-studio-mcp` (stdio). Shares the same SQLite database and workspace folders as the desktop app.

Setup: [README — MCP server](../../README.md#mcp-server-headless).

This is **not** the per-workspace Python helper at `.sprite-studio/sprite_rig_mcp.py` (rig renderer only).

---

## Tool index

| Category | Tools |
| --- | --- |
| Workspace | `studio_status`, `open_workspace` |
| Chat / generation | `ensure_conversation`, `attach_references`, `generate`, `get_generation`, `cancel_generation` |
| Assets | `list_artifacts`, `list_assets`, `list_packs`, `export` |
| Pipeline (AI hardening) | `promote_anchor`, `orient_anchor`, `check_anchor_facing`, `get_anchor`, `list_anchors`, `list_facing_checks`, `score_strip`, `split_strip`, `extract_video_frames`, `clean_alpha`, `normalize_animation`, `snap_to_pixel_grid`, `harden_animation`, `align_frames`, `check_size_contract`, `check_character_contract`, `mirror_animation`, `queue_direction_set`, `queue_motion_batch`, `queue_region_regen`, `retry_size_contract`, `queue_contract_retry`, `finalize_contract_retry`, `set_animation_review_status`, `score_animation_frames`, `get_production_score`, `export_character_pack` |
| Rig | `save_rig`, `render_rig_animation`, `suggest_rig_points`, `analyze_rig_fit`, `interpolate_rig_frames` |
| Jobs | `queue_sprite_sheet`, `queue_procedural_vfx`, `get_job`, `quality_report` |

---

## Production pipeline (recommended order)

### Agent-first (two tools)

When frames already exist on an animation:

1. **`harden_animation`** — idempotent chain: optional strip split → `clean_alpha` → `normalize_animation` → optional `snap_to_pixel_grid` → `check_size_contract` → writes `last-generation.json`.
2. If `contractReport.passed` is false and a chat is open, call **`harden_animation`** again with `options: { cleanAlpha: false, normalize: false, queueContractRetry: true }` and poll **`get_job`** for the returned `jobId`.
3. **`set_animation_review_status`** with `status: "accepted"` — unlocks export.

### Manual / step-by-step

For AI-generated character animation:

1. **`promote_anchor`** — lock canvas size, foot baseline, and centroid pivot from the approved master.
2. **`score_strip`** (optional) — infer frame count, grid-ink QC, and recommended layout before import.
3. **`split_strip`** or **`extract_video_frames`** — import frames as workspace assets (`assetIds`).
4. Save an animation in the app (or via your agent) whose `frames` use those `assetIds`.
5. **`normalize_animation`** — `lockFirstFrame: true`, `sharedScale: true`, `anchorSlug` set.
6. **`align_frames`** — optional 1–2 px metadata nudges.
7. **`check_size_contract`** — diagnostics (canvas, baseline, identity, centroid, motion).
8. **`retry_size_contract`** — deterministic normalize/nudge passes; optional `regenerate: true` + `conversationId` for a single AI strip (manual finalize).
9. **`queue_contract_retry`** — **autonomous** loop: deterministic repair → AI regeneration (poll) → import strip → re-check, up to `maxAiAttempts` (default 2). Poll with **`get_job`** using returned `jobId`.
10. **`finalize_contract_retry`** — manual path after AI strip exists: profile split → normalize → re-check.
11. **`check_character_contract`** (optional) — validate every animation in a character worktree against the same anchor before shipping a character pack.
12. **`set_animation_review_status`** with `status: "accepted"` — unlocks export and sprite-sheet jobs.
13. **`export`** or **`queue_sprite_sheet`** with `exportFormat` / `metadataFormat` when you need Aseprite, TexturePacker, or Godot metadata.

---

## Pipeline tools (detail)

### `promote_anchor`

Promotes an asset to `.sprite-studio/anchors/<slug>.json`.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `assetId` | string | yes | Master sprite |
| `slug` | string | no | Defaults from asset name |
| `autoOrient` | boolean | no | Default `false` — detect facing and flip to canonical west (side) or north (top-down) |
| `view` | string | no | `side` or `top-down`; inferred from asset when omitted |

### `orient_anchor`

Flips a promoted anchor asset to its canonical facing (side view → west). Updates the anchor sidecar and writes a `facing-check.json` entry.

| Parameter | Type | Required |
| --- | --- | --- |
| `workspaceId` | string | yes |
| `slug` | string | yes |

### `check_anchor_facing`

Read-only facing report for a promoted anchor (mass-skew heuristics). Does not modify assets.

| Parameter | Type | Required |
| --- | --- | --- |
| `workspaceId` | string | yes |
| `slug` | string | yes |

Returns `FacingCheckReport`: `detectedFacing`, `canonicalFacing`, `confidence`, `status` (`ok` \| `mismatch` \| `uncertain`).

### `mirror_animation`

Derives a mirrored facing animation without AI (e.g. `walk-e` from `walk-w`). Uses rig re-render when a matching rig exists; otherwise flips frame PNGs.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | Source animation |
| `targetFacing` | string | yes | `w`, `e`, `n`, `s`, `nw`, `ne`, `sw`, `se` |
| `sourceFacing` | string | no | Inferred from direction meta or name suffix |
| `anchorSlug` | string | no | Used for normalize + contract check |
| `rigId` | string | no | Force rig-native mirror path |

### `queue_direction_set`

Queues a background job that mirrors derivable facings from a canonical source animation, optionally generates remaining facings with AI when `conversationId` is set, then runs `check_character_contract`.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `worktreeId` | string | yes | Character worktree |
| `sourceAnimationId` | string | yes | Canonical facing (usually west) |
| `anchorSlug` | string | yes | |
| `motion` | string | yes | e.g. `walk`, `run`, `idle` — direction family key |
| `set` | string | yes | `"4"` or `"8"` |
| `conversationId` | string | no | When set, AI-generates non-mirrorable facings |

Poll with **`get_job`**. Job metadata includes `mirroredAnimationIds`, `generatedAnimationIds`, `pendingFacings`, and `characterContract`. Status is `failed` when the character contract does not pass.

### `get_anchor` / `list_anchors`

Read promoted anchors. `list_anchors` returns summaries including `pivot` and `baselineY`.

### `score_strip`

Pre-import QC for a sprite strip: infers frame count from alpha valleys, scores grid ink, and recommends a layout. Does not write assets.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `sourcePath` | string | yes | Absolute path to strip image |
| `frameCount` | number | no | When set, scores that partition; when omitted, infers count |
| `layout` | string | no | Default `auto` — tries profile gutters, falls back to DP cuts |

Returns `StripScoreReport`:

```json
{
  "suggestedFrameCount": 6,
  "frameCountUsed": 6,
  "inferenceConfidence": 0.82,
  "gridInkDetected": false,
  "minCellWidth": 12,
  "meanMotionDelta": 0.14,
  "segmentWidths": [18, 17, 19, 18, 17, 18],
  "layoutRecommended": "profile",
  "warnings": []
}
```

### `split_strip`

Splits a horizontal strip, vertical strip, or grid into individual PNG assets.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `sourcePath` | string | yes | Absolute path to strip image |
| `layout` | string | yes | `profile` (alpha gutters, **preferred for AI**), `auto` (infer + profile/DP fallback), `horizontal`, `vertical`, `grid` |
| `frameCount` | number | yes | Expected cells; use **`0` with `layout: "auto"`** to infer count |
| `columns` | number | no | Grid layout only |
| `recoverForeground` | boolean | no | Default `true` — trims empty margins per cell |
| `category` | string | no | Asset folder, default `characters` |

Returns `SplitStripResult`:

```json
{
  "assetIds": ["..."],
  "relativePaths": ["assets/characters/..."],
  "framePaths": ["/abs/path/..."],
  "warnings": ["optional grid-ink or QC message"],
  "suggestedFrameCount": 6,
  "frameCountUsed": 6,
  "inferenceConfidence": 0.82,
  "layoutUsed": "profile"
}
```

### `extract_video_frames`

Extracts PNG frames from a video using **bundled ffmpeg** (installed builds) or a system `ffmpeg` on `PATH` (dev/MCP fallback).

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `videoPath` | string | yes | Absolute path to `.mp4`, `.webm`, `.mov`, etc. |
| `fps` | number | no | Default `10` — extraction rate |

Frames are written under `assets/imports/<video-name>-frames/` and indexed like other imports.

Returns the same `SplitStripResult` shape as `split_strip` (without strip QC warnings).

**Errors:** `ffmpeg_missing`, `ffmpeg_failed`, `ffmpeg_no_frames`, `video_not_found`.

**Typical use:** Scenario / video-to-sprite workflows — extract frames, then run `normalize_animation` on the saved animation.

### `normalize_animation`

Resizes and aligns every frame to the promoted anchor contract (foot baseline + centroid X).

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `anchorSlug` | string | no | Uses first promoted anchor if omitted |
| `lockFirstFrame` | boolean | no | Vertical foot lock from frame 1 |
| `sharedScale` | boolean | no | One scale factor for all frames |
| `padding` | number | no | Inset padding inside canvas |

### `clean_alpha`

Removes semi-transparent halos, defringes magenta spill, and prunes orphan pixels on every animation frame PNG. Non-destructive to offsets.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |

Returns `{ animationId, framesProcessed, framesChanged }`.

### `snap_to_pixel_grid`

Snaps per-frame `offsetX` / `offsetY` to a pixel grid. When `gridSize` > 1, also quantizes opaque RGB channels to the same grid step.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `gridSize` | number | no | Default `1` (offsets only). Use `2` or `4` for palette quantization |

### `harden_animation`

Idempotent orchestrator for post-gen hardening. Runs the production chain and returns an aggregated report.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `anchorSlug` | string | no | |
| `sourcePath` | string | no | When set, splits a strip first (requires `frameCount`) |
| `frameCount` | number | no | Required with `sourcePath` |
| `conversationId` | string | no | Required when `options.queueContractRetry` is true |
| `options.cleanAlpha` | boolean | no | Default `true` |
| `options.normalize` | boolean | no | Default `true` |
| `options.snapGrid` | boolean | no | Default `false` |
| `options.gridSize` | number | no | Passed to `snap_to_pixel_grid` |
| `options.splitLayout` | string | no | `profile`, `dp`, or `auto` when splitting |
| `options.queueContractRetry` | boolean | no | Default `false` — queues autonomous retry when contract fails |

Returns `HardenAnimationReport`: `{ animationId, steps[], contractReport, jobId? }`.

**Retry tip:** On a second call for contract recovery only, pass `{ cleanAlpha: false, normalize: false, queueContractRetry: true }` so frames are not re-normalized.

### `align_frames`

Non-destructive per-frame `offsetX` / `offsetY` metadata nudges.

| Parameter | Type | Required |
| --- | --- | --- |
| `animationId` | string | yes |
| `deltas` | array | yes — `{ frameIndex, offsetX, offsetY }` |
| `applyToAll` | boolean | no |

### `check_size_contract`

Validates animation against anchor (when applicable).

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `anchorSlug` | string | no | Explicit anchor; otherwise first anchor for character/creature/animation worktrees |

**Scope:** Size contract runs only for worktrees `character`, `creature`, or `animation`, or when `anchorSlug` is passed. VFX, props, and tilesets skip contract checks.

**Violation codes:** `canvas_size`, `baseline_drift`, `identity_drift`, `identity_warning`, `centroid_drift`, `motion_still`, `missing_anchor` (non-blocking warning).

### `check_character_contract`

Runs `check_size_contract` for **every animation** in a character/creature/animation worktree and aggregates the result.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `worktreeId` | string | yes | Must be `character`, `creature`, or `animation` |
| `anchorSlug` | string | no | Defaults to the first promoted anchor |

Returns `CharacterContractReport` with `passed`, `animationCount`, `anchorSlug`, and per-animation `animationReports`.

### `export`

Exports a horizontal strip PNG plus sidecar metadata for one animation.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Use `animation` |
| `id` | string | yes | Animation id |
| `destination` | string | no | Subfolder under workspace `exports/` |
| `exportFormat` | string | no | `sprite-studio` (default), `aseprite-json`, `texturepacker`, `godot-spriteframes` |

`sprite-studio` metadata includes fixed-cell fields: `anchorSlug`, `baselineY`, `pivot`, `sourceSize`, per-frame `trimOffset` and `spriteSourceSize`. Aseprite/TexturePacker/Godot carry pivot and baseline in `meta` (and Godot `metadata/fixed_cell`); trim rects are per-frame in all JSON formats.

### `queue_sprite_sheet`

Same `metadataFormat` values as `export`. Godot exports write `.tres` beside the PNG; other formats write `.json`.

### `set_animation_review_status`

| Parameter | Type | Values |
| --- | --- | --- |
| `animationId` | string | |
| `status` | string | `draft`, `accepted`, `rejected` |

`accepted` requires no **blocking** contract violations. Export and sprite-sheet jobs are gated until `accepted` or contract passes.

### `retry_size_contract`

Self-correcting loop inspired by PerfectPixel: deterministic repairs first, then optional AI regeneration.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `anchorSlug` | string | no | |
| `conversationId` | string | no | Required when `regenerate: true` |
| `regenerate` | boolean | no | Starts provider run with violation-specific corrective prompt |
| `maxDeterministicPasses` | number | no | Default `2` — normalize + baseline nudge |

Returns `ContractRetryResult` with `attempts`, `correctivePrompt`, `generationRequestId`, `finalReport`.

### `queue_contract_retry`

Fully autonomous PerfectPixel-style loop (background job `kind: contract_retry`).

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `animationId` | string | yes | |
| `conversationId` | string | yes | Chat used for corrective strip generation |
| `anchorSlug` | string | no | |
| `maxAiAttempts` | number | no | Default `2` |
| `maxDeterministicPasses` | number | no | Default `2` |

Returns immediately with `jobId` and a **pre-job** `finalReport` snapshot (`passed` is always `false` on enqueue). Poll **`get_job`** until `status` is `completed`, `failed`, or `cancelled`. Use `stage` for live progress (`Deterministic repair`, `Regenerating strip (1/2)`, `Waiting for generation`, `Importing regenerated strip`). Parse `metadataJson` (JSON string) for `{ "attempts": [...] }` — each attempt records `phase`, `actions`, and a `report` snapshot. Then call **`check_size_contract`** to read the post-job result. Frames are imported automatically when the job completes successfully.

### `finalize_contract_retry`

Run after AI produces a new horizontal strip.

| Parameter | Type | Required |
| --- | --- | --- |
| `animationId` | string | yes |
| `sourcePath` | string | yes — absolute path to strip PNG |
| `frameCount` | number | yes |
| `anchorSlug` | string | no |

### `queue_motion_batch`

Queue catalog motions on a character worktree (walk, run, idle, …). Poll **`get_job`** with the returned `jobId`.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `worktreeId` | string | yes | Character worktree |
| `anchorSlug` | string | yes | Promoted anchor |
| `motions` | string[] | no | Catalog ids; empty = all enabled presets |
| `set` | string | yes | `"4"` or `"8"` direction set |
| `conversationId` | string | no | Required for AI generation inside the batch |
| `hardenAfter` | boolean | no | Run harden after each motion |
| `seedAnimationId` | string | no | Canonical west-facing reference |
| `onlyMissing` | boolean | no | Skip motions that already pass contract on canonical facing |

### `queue_region_regen`

Queue inpainting for one frame region. Requires an open chat.

| Parameter | Type | Required |
| --- | --- | --- |
| `animationId` | string | yes |
| `frameIndex` | number | yes |
| `regions` | array | yes — `{ x, y, width, height }` |
| `conversationId` | string | yes |
| `prompt` | string | no |

### `export_character_pack`

Export every animation in a character worktree to a workspace-relative folder.

| Parameter | Type | Required | Notes |
| --- | --- | --- | --- |
| `workspaceId` | string | yes | |
| `worktreeId` | string | yes | |
| `destination` | string | yes | Subfolder under workspace `exports/` |
| `anchorSlug` | string | no | |
| `metadataFormat` | string | no | Same as `export` |
| `includeAnimatedPreviews` | boolean | no | Write GIF previews in `previews/` |

Typical agent flow after motions are accepted: **`queue_motion_batch`** → poll jobs → **`export_character_pack`**.

### `get_production_score`

Aggregate score for a character worktree (size contract, character contract, quality). CI gate threshold on the walk fixture is **60**.

| Parameter | Type | Required |
| --- | --- | --- |
| `workspaceId` | string | yes |
| `worktreeId` | string | yes |
| `anchorSlug` | string | no |

### `score_animation_frames`

Per-frame motion delta and stability metrics for one animation.

| Parameter | Type | Required |
| --- | --- | --- |
| `animationId` | string | yes |

### `interpolate_rig_frames`

Insert interpolated rig keyframes between two pose indices. Same `rig` body as **`save_rig`**.

| Parameter | Type | Required |
| --- | --- | --- |
| `rig` | object | yes |
| `fromIndex` | number | yes |
| `toIndex` | number | yes |
| `steps` | number | yes | 1–16 inserted poses |

### `list_facing_checks`

Read facing-check history from `.sprite-studio/facing-checks.json`.

| Parameter | Type | Required |
| --- | --- | --- |
| `workspaceId` | string | yes |
| `slug` | string | no | Filter to one anchor slug |

---

## Example: video strip to walk cycle

```json
// 1. Open workspace
{ "path": "C:/games/my-project" }

// 2. Extract frames from AI video
{
  "workspaceId": "<id>",
  "videoPath": "C:/games/my-project/refs/walk-preview.mp4",
  "fps": 12
}

// 3. Create animation with returned assetIds (app or agent), then:
{
  "animationId": "<animation-id>",
  "anchorSlug": "hero",
  "lockFirstFrame": true,
  "sharedScale": true
}

// 4. Accept and export
{ "animationId": "<animation-id>", "status": "accepted" }
```

---

## Prerequisites

| Requirement | Tools affected |
| --- | --- |
| Codex CLI (`codex login`) | `generate` (default provider) |
| Bundled ffmpeg (installers) or PATH (dev) | `extract_video_frames`, UI **Import video** |
| Promoted anchor | `normalize_animation`, `check_size_contract` for characters |

---

## Related docs

- [User guide](user-guide.md) — UI workflows
- [Competitive pipeline](competitive-pipeline.md) — why strip-first and profile split
- [Italian MCP reference](../it/riferimento-mcp.md)
