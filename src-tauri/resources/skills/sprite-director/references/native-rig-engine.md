# Native rig engine contract

Sprite Studio ships a deterministic rig engine written in Rust. The agent's job on a `/rig` request is to **place points and bones — not render pixels**. The app captures the returned `rig-suggestion` JSON block, opens it in the Rig editor for review, and renders every animation frame itself from the capsules.

## When to use this harness

- The request is a `/rig` command, or explicitly asks for rigging, joint points, a skeleton, capsule bones, or pose keyframes.
- The user wants deterministic, reproducible frames (identical rig + master ⇒ byte-identical PNGs) instead of sequential AI frames.
- A motion needs planted contacts (feet that do not slide) solved with two-bone IK.

Deterministic rigging is the default for `/animate`. Sequential AI frame generation is allowed only as explicit post-rig AI Polish or Full redraw, with the rough rig frames remaining the pose and timing authority.

## Output contract

Return exactly one fenced block tagged `rig-suggestion`:

```rig-suggestion
{
  "morphology": "biped",
  "points": [
    {"name": "neck", "kind": "joint", "x": 24, "y": 18, "confidence": 0.9, "note": "chin line"},
    {"name": "foot_r", "kind": "contact", "x": 27, "y": 58, "confidence": 0.85}
  ],
  "bones": [
    {"name": "torso", "start": "neck", "end": "hip", "radius": 6, "parent": null, "z": 5},
    {"name": "shin_r", "start": "knee_r", "end": "foot_r", "radius": 3, "parent": "thigh_r", "z": 9}
  ],
  "frames": [
    {"phase": "contact", "rootDx": 0, "rootDy": 0,
     "transforms": [{"bone": "thigh_r", "rotate": 18, "dx": 0, "dy": 0, "scaleX": 1, "scaleY": 1}],
     "contacts": [{"bone": "shin_r", "x": 27, "y": 58, "bend": 1}]}
  ],
  "reasoning": "observed anatomy, joint evidence, and pose logic in one short paragraph"
}
```

## Semantics the engine guarantees

- **Coordinates** are source pixels of the master, origin top-left, +y down. Out-of-range values are clamped, and bones referencing unknown points are dropped on import.
- **Points** carry `kind` (`joint`, `anchor`, `contact`, `pivot`), a 0–1 `confidence`, and a `source` tag (`auto` for template suggestions, `ai` for agent suggestions, `user` for hand-dragged).
- **Bones are capsules**: `start`/`end` name two points, `radius` is in pixels. The engine auto-claims every opaque pixel inside the closest capsule; leftover pixels fall to the nearest bone, so the silhouette always re-renders — no hand-drawn masks.
- **Hierarchy**: `parent` chains limb→torso/head. Cycles are cut on import. A parent rotation carries every descendant.
- **Layering** uses `z`: far-side limbs 1–3, torso/head 4–6, near-side limbs 7–10. Higher `z` paints over lower.
- **Frames** are per-playback-frame poses. `rotate` is degrees clockwise on screen about the bone's start point; `rootDx`/`rootDy` offset the whole body; `hold` duplicates the previous rendered frame.
- **Contacts** plant a bone's end point at a canvas position using analytic two-bone IK along the parent chain; `bend` (+1/−1) picks the knee or elbow direction.
- Rendering is nearest-neighbor inverse-mapped per owned region, so no scaling blur is ever introduced.

## Rules

1. Inspect the actual master image; read joint positions off the visible anatomy. Never guess or average blindly.
2. Capsules must tile the silhouette: thin limbs get small radii, torso/head get larger ones.
3. Include `frames` only when a motion intent was given. Keep adjacent pose changes small, keep planted points fixed through `contacts`, and end on a frame that leads smoothly back into the first.
4. After the block, write at most three sentences of summary. Nothing before the block.
5. Do not write rendered frame PNGs, masks, or rig-rendering scripts for this request — the Rust engine does the rendering.
