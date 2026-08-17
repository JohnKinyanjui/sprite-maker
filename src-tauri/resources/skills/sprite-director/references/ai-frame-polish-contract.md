# AI frame-polish contract

Read the request for an explicit finishing mode:

- **Rig only (default):** render every frame deterministically from the saved rig. Do not call an image provider for animation frames.
- **AI polish:** build and validate the deterministic rig first, render rough frames, then use each rough frame as the exact pose and layout authority. Repair rendering detail only.
- **Full redraw (experimental):** build and validate the deterministic rig first, then redraw each rough frame while preserving its pose, timing, silhouette, scale, ground line, camera, and canvas placement.

If no finishing mode is stated, use **Rig only**. Never silently spend image-provider calls or replace deterministic motion with independently invented poses.

## Shared order of operations

1. Approve or create one transparent source master.
2. Build, save, and validate a deterministic rig tied to that exact source.
3. Render the complete rough animation from the rig and preview at least three cycles.
4. If and only if the user selected AI polish or Full redraw, process frames in playback order with the corresponding rough frame as the strongest reference.
5. Preserve frame count, timing, contact states, limb identity, pivot, and final-to-first continuity.
6. Normalize every accepted result with the same crop, scale, anchor, palette, alpha, and padding treatment before it enters `assets/`.
7. Update `.sprite-studio/last-generation.json` and run native quality analysis.

## AI polish

Make bounded regional repairs to outlines, joints, seams, or small texture defects. Do not change the pose, anatomy, proportions, equipment, silhouette, camera, lighting direction, canvas position, or timing. A polished frame must remain traceable to its rough rig frame.

## Full redraw

Treat the rough rig frame as pose-canonical. Reject any redraw that changes limb count, pose, body scale, contact state, ground anchor, facing direction, or framing. This mode is experimental and must never be selected implicitly.

## Acceptance gates

Reject output with identity drift, malformed anatomy, dirty alpha, blur, palette flicker, per-frame recentering, canvas-edge clipping, missing provenance, or a loop seam worse than an ordinary adjacent transition. When a polished result fails, keep the deterministic rough frame rather than publishing a worse replacement.
