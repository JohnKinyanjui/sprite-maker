# AI frame-polish contract

Read the user request for one exact mode line:

- `Polish mode: Rig only.` — do not call ImageGen for animation frames.
- `Polish mode: AI polish.` — recommended; polish only rough frames that fail visual or quality gates.
- `Polish mode: Full redraw experimental.` — redraw every rough frame, but keep the deterministic rig as the pose, timing, and silhouette authority.

If no mode is present, default to **Rig only**. Never silently spend ImageGen calls.

Selecting **AI Polish** or **Full redraw** is also explicit consent for one automatic motion-ready source-master revision if visual inspection finds that required locomotor parts are fused or hidden. Keep the focused source unchanged as the identity reference, create a named revision at maximum reference strength, record both hashes, and continue in the same request without asking permission. This revision fixes separability only; it must not invent animation timing or a pose sheet.

## Shared required order

1. Approve one source master and complete the full body-part rig.
2. Validate and deterministically render every rough frame with `sprite_rig.py`.
3. Preview the rough animation for at least three cycles and identify exact failing frame numbers and regions.
4. Only then perform the selected polish mode. ImageGen must not choose frame count, timing, root motion, contacts, or a different pose.
5. Run every ImageGen result through `.sprite-studio/sprite_polish.py`; never copy a raw ImageGen output directly into `assets/`.
6. Rescan and run the native quality report. A rejected first attempt is not the end of AI Polish: automatically retry the named frame up to three total attempts with a tighter defect region and stricter identity wording. Keep the better rough frame only after all three attempts fail.

## AI Polish — recommended

Polish only frames with a named defect such as an exposed joint, collapsed hand/foot, broken foreshortening, missing compression, or unreadable overlapping limbs. For each repair call, supply these images together:

1. the approved master as identity and palette reference;
2. the deterministic rough frame as the edit target and exact pose guide;
3. the previous rough/polished frame;
4. the next rough frame.

Tell ImageGen to change only the named defective regions, preserve the rough frame's silhouette, contact points, facing direction, proportions, canvas composition, and neighbor continuity, and return one frame on a perfectly flat chroma-key background. Do not request a pose sheet or multiple frames in one image. Record a tight `x,y,width,height` repair box around each named defect on the rough frame; it may not cover more than half the logical canvas.

Save raw results under `.sprite-studio/polish-sources/<animation>/<frame>.png`, then normalize each one:

```bash
python3 .sprite-studio/sprite_polish.py \
  --master <approved-master.png> \
  --rough <rough-frame.png> \
  --input .sprite-studio/polish-sources/<animation>/<frame>.png \
  --region <x,y,width,height> \
  --output <rough-frame.png> \
  --report .sprite-studio/polish-sources/<animation>/<frame>.json
```

The tool archives the replaced rough frame, restores its logical canvas and alpha, anchors the result to the rough silhouette bounds, maps colors back to the master palette, and keeps every pixel outside the repair box byte-identical to the rough frame. Treat a tool rejection as a failed polish attempt, not permission to bypass validation. On retry, shrink or correct the region and tell ImageGen exactly which joint/contact pixels failed; do not redraw the whole subject again.

When a regional report is valid, `outsideRegionUnchanged` is true, and cyclic inspection shows the named joint/contact improved without a boundary seam, accept the regional frame. Do not reject it for whole-body identity drift: whole-body identity cannot drift because the tool preserved all pixels outside the named box. If a seam remains, retry with a smaller or better-aligned box rather than restoring the rough frame immediately.

## Full redraw — experimental

Use the same per-frame inputs and postprocessor, but redraw every rough frame. The rough frame remains the pose guide. Process frames in playback order so the prior accepted result can guide the next call. After the last frame, re-check it against frame one; reject identity or silhouette drift even if an individual frame looks attractive.

## Acceptance gates

- final dimensions, category, FPS, order, and frame count match the rig and manifest;
- every polished frame has transparent corners and uses the approved master palette;
- eyes, markings, costume, equipment, outline weight, and body proportions do not drift;
- support feet, landing points, hinges, and the root arc remain at their rigged coordinates;
- adjacent-frame and final-to-first changes remain mechanically continuous;
- raw ImageGen files, normalization reports, rough-frame archives, and hashes remain recoverable.

Report which frames were polished and why. If none failed the rough-frame gates, say so and keep the deterministic frames unchanged.
