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

## Deterministic layered rig renderer

For any raster master that needs animation, write a rig JSON and run:

```bash
python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<rig>.json
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<rig>.json
```

The AI proposes the parts, masks, physical pivots, z order, and motion table. The validation pass checks that proposal and locks the master hash. Only then does the deterministic renderer apply nearest-neighbour translations, rotations, and scales. It supports characters and game objects. ImageGen must never invent animation timing, poses, or pose sheets. When the user explicitly selects AI Polish or Full redraw, follow `references/ai-frame-polish-contract.md`: edit only completed rough rig frames and pass every result through `sprite_polish.py` before acceptance.

## Validate

Follow `references/quality-gates.md`. Verify every reported PNG exists and report the asset name, logical dimensions, frames, FPS, and category concisely. Do not expose internal spec paths unless troubleshooting.
