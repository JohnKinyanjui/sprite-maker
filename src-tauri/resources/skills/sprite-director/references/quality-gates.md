# Sprite quality gates

Before reporting completion, verify:

1. **Files:** every PNG exists under `assets/<category>/` and decodes successfully.
2. **Canvas:** every animation frame has identical logical dimensions.
3. **Transparency:** the background is transparent and corners have alpha zero.
4. **Silhouette:** the subject reads at 1× and does not touch unintended canvas edges.
5. **Pixel discipline:** no accidental antialiasing, blurry scaling, compression noise, or isolated pixels.
6. **Palette:** outline, shadow, base, and highlight roles stay consistent across frames.
7. **Rig provenance:** animation poses come from the saved deterministic rig tied to the exact focused source. Independently generated AI poses are rejected.
8. **Rig reproducibility:** re-rendering the saved rig produces the same frame order, dimensions, pivot, timing, and visible layer order.
9. **Animation loop:** preview at least three cycles. Feet and pivot do not drift, motion arcs are intentional, and the final-to-first transition is no worse than an ordinary adjacent transition.
10. **Character consistency:** eye line, head size, anatomy, markings, costume, palette, equipment, ground line, and facing direction remain stable.
11. **Motion mechanics:** contacts do not slide, visible joints remain connected, root motion matches the stated intent, and no accidental duplicate endpoint is accepted.
12. **Paired-limb identity:** near/far limbs keep their depth and occlusion roles throughout crossings; a limb identity swap fails validation.
13. **Manifest:** `.sprite-studio/last-generation.json` lists accepted files in playback order with the correct FPS, category, source provenance, and generation mode.
14. **Category contract:** the rig category, `assets/<category>/` folder, manifest category, and indexed assets agree.
15. **AI finishing provenance:** AI polish or Full redraw is allowed only when explicitly selected and every result traces to a corresponding rough rig frame. The rough pose remains authoritative.
16. **Clean handoff:** only final accepted manifest frames remain in `assets/`; superseded polish attempts stay archived outside the published asset folder.
17. **Originality:** style references are translated into general traits without copying a known character or exact sprite.
18. **Attachment sanity:** no limb, wing, tail, head, weapon, or articulated layer reads as a floating island. A mathematically coincident anchor still fails when the visible pixels do not maintain a believable overlap around the joint.
19. **Wing mechanics:** wing roots stay visibly seated in the shoulder/chest throughout the cycle; membranes fold and lag around the wing joints instead of the entire wing orbiting as one rigid cutout. Wing count, near/far identity, and depth stay constant.

If the first candidate fails, silently repair its master, rig, mask, layer order, anchor, weighting, timing, or optional polish and render exactly one replacement. Apply the same gates to the replacement and publish the better structurally valid candidate. Treat remaining visual-quality issues as warnings and simplify blocked motion until the deterministic validator accepts it. An animation handoff requires at least two distinct frames; never label a one-frame fallback as completed animation. Stop with `GENERATION_FAILED` only when no valid workspace-confined result of the requested kind can be produced at all.
