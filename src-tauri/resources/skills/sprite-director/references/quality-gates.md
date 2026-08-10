# Sprite quality gates

Before reporting completion, verify:

1. **Files:** every PNG exists under `assets/<category>/` and decodes successfully.
2. **Canvas:** every animation frame has identical logical dimensions.
3. **Transparency:** the background is transparent and corners have alpha zero.
4. **Silhouette:** the subject reads at 1× and does not touch unintended canvas edges.
5. **Pixel discipline:** no accidental antialiasing, blurry scaling, or isolated noise pixels.
6. **Palette:** outline, shadow, base, and highlight roles stay consistent across frames.
7. **Animation loop:** preview at least three consecutive cycles. Feet/pivot do not drift, motion arcs are intentional, and the final-to-first transition is as smooth as any adjacent pair. The final frame must lead into the first rather than duplicate it. Reject a snap in pose, root position, contact state, silhouette, palette, or opacity.
8. **Originality:** style references are translated into general traits without copying a known character or exact sprite.
9. **Manifest:** `.sprite-studio/last-generation.json` lists files in playback order with the correct FPS and category.
10. **Clean handoff:** only the final manifest frames remain in `assets/`; the renderer archives superseded iterations under `.sprite-studio/versions/`.
11. **Master provenance:** character jobs and illustrated game objects have one approved master. Animation poses come from a saved rig under `.sprite-studio/rigs/`; ImageGen pose sheets and independently generated frames are rejected. Explicit polished frames additionally require the complete frame-polish provenance chain.
12. **Character consistency:** eye line, shoulders, head size, total height, face, hair, costume, palette, equipment, foot line, and pivot remain consistent across frames.
13. **Motion mechanics:** adjacent frames change intentional limb mechanics, walks contain opposing contact and passing poses, arms oppose legs, and the action reads during playback rather than only as still images.
14. **Rig reproducibility:** rerunning the same rig JSON against the same master produces identical frames, with connected joints/hinges and no exposed mask holes.
15. **Rig contract:** the AI-authored rig passes `sprite_rig.py --validate`, its saved master hash matches the approved master, and mask overlap is either removed or explicitly documented as intentional joint coverage.
16. **Category contract:** the rig category, `assets/<category>/` output folder, manifest category, indexed assets, and routed harness category are identical. A creature may not be accepted under `props`, and an older rig from another category may not be reused.
17. **Focused-reference provenance:** when the user supplies a focused reference, the rig source must be that exact file or a documented normalized derivative of it. A visually similar older workspace master is not a substitute.
18. **Loop intent:** the proposal states `LOOP INTENT`. Seamless loops contain the recovery/settle phase needed to close; non-looping output is allowed only when the user explicitly requested a one-shot action or final hold.
19. **AI-polish provenance:** when frame polishing is selected, every final frame traces to a deterministic rough frame, raw ImageGen repair, `sprite_polish.py` normalization report, and archived rough-frame hash. Raw ImageGen pixels may never bypass canvas, alpha, palette, silhouette, contact, identity, and loop checks.
20. **Regional polish containment:** AI Polish names a tight repair region, preserves every rough-frame pixel outside it byte-for-byte, and retries a rejected repair up to three total attempts before falling back. Full redraw is the only mode allowed to replace the complete subject.
21. **Physical envelope:** unless explicitly overridden, the proposal records plausible scale in meters, speed in m/s, vertical rise or travel in meters, seconds per cycle, and contact/gravity assumptions. Pixel transforms, frame count, FPS, body-length displacement, and final-to-first momentum agree with those values.

If a check fails, revise the spec and render again before responding.
For rigged animations this repair is automatic and AI-directed: revise only masks, pivots, z order, transforms, tiny joint patches, or a contract-compliant regional AI polish, then revalidate and deterministically rerender. Never generate an independent unrigged pose.
