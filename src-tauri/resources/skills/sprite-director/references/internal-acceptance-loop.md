# Internal visual acceptance loop

Run this acceptance loop silently before publishing a sprite or animation. It is an implementation detail: do not expose the review transcript, rejected attempt, or retry count in chat or the UI.

1. Produce the first candidate without updating `.sprite-studio/last-generation.json`.
2. Inspect every static sprite at both 1× and 4×. For animation, build a contact sheet in playback order and validate the cycle headlessly as described in **Headless validation** below.
3. Mark a candidate for repair when its anatomy or motion does not make physical and visual sense: a limb, wing, tail, weapon, or joint floats or disconnects; an attachment stretches away from its body; paired appendages swap identity; a planted contact slides; the subject changes proportions or markings; a part clips; motion is merely whole-body translation; or the loop visibly pops.
4. For winged actors specifically, keep each wing root continuously anchored and overlapping its shoulder/chest region. The root drives the stroke while the membrane folds or lags around its wrist. A whole wing must never orbit, translate, or float as a rigid island. Downstroke/upstroke timing must produce a believable opposing chest reaction without changing wing count, side identity, or depth order.
5. If the first candidate fails, revise the responsible master, rig, masks, pivots, weights, transforms, contacts, or optional polish and render exactly one replacement attempt. Do not stack a second retry. Preserve a focused user reference; regenerate an unfocused generated master only when the master itself is the cause.
6. Inspect the replacement with the same gates. Choose the better of the original and replacement, move only that candidate into `assets/<category>/`, and write a fresh generation manifest. Keep the unused attempt outside `assets/`.
7. Remaining visual imperfections are soft failures. Publish the best structurally valid candidate and end with `GENERATION_WARNING: <concise remaining limitation>`. If the requested motion cannot pass the deterministic rig validator, reduce pose amplitude, simplify the motion, or reduce the frame plan until it validates. An explicit animation request must still publish at least two distinct frames; never report a one-frame fallback as an animation.
8. Use `GENERATION_FAILED` only when no decodable, correctly categorized, workspace-confined asset can be produced at all. Never restore a stale manifest as the result of the current turn.

## Headless validation

Playback validation is internal, deterministic, and headless. Never open a browser, Chrome, Safari, a local HTML/GIF preview page, a dev server, or an `open`/`xdg-open`/`start` command, and never request computer-use, screen-control, or other interactive approval to watch an animation. A "cycle preview" means these non-interactive checks:

- the strict rig validator (`sprite_rig.py --check`, the `sprite_rig_validate`/`sprite_rig_analyze_motion` MCP tools, or `analyze_rig_fit` for native rigs), which already rejects loop-seam pops, final-to-first discontinuities, duplicate endpoint frames, sliding planted anchors, and clipping;
- a direct image inspection of the rendered frames or contact sheet in playback order, reading the last→first pair as one more adjacent transition;
- the frame hashes and dimensions reported by the renderer.

Sprite Studio runs native quality analysis automatically after it registers the manifest; do not poll or wait for it. If any tool, command, or permission request is denied, do not retry it or seek an alternative interactive route: continue with the checks above. A denied or unavailable optional preview is never a reason to withhold a structurally valid result.

## Repair budget

Keep a normal request fast and bounded:

- at most three validate→fix cycles on the rig before the first render; if the validator still rejects the motion, simplify the motion or reduce amplitude instead of continuing to iterate;
- at most one post-render repair rerender (the single replacement attempt above);
- never iterate pixel by pixel. One-pixel seams, small outline gaps, minor joint cosmetics, and slight shading flicker are soft warnings, not blockers.

When the budget is spent, stop repairing, publish the best structurally valid candidate, and report the remaining limitation with `GENERATION_WARNING`. Sprite Studio enforces these limits at runtime from the tool calls themselves: starting one validation or rerender beyond them stops the run, and Sprite Studio renders and publishes the latest structurally valid rig with the warning on the agent's behalf.

## Failure reporting

Hard failures still block publishing: missing or undecodable frames, mismatched canvas sizes, files outside the workspace or routed category, broken provenance, or fewer than two distinct frames for an explicit animation. If you withhold the result for any reason, begin the reply with `GENERATION_FAILED: <reason>`. Never describe a withheld result only as "unpublished" or "not published", and never point the user at an older asset as if it were this turn's output.

This loop supplements deterministic validators. Hard safety, file-integrity, workspace-boundary, category, and provenance checks remain mandatory; subjective visual quality should degrade gracefully instead of discarding usable work.
