# Internal visual acceptance loop

Run this acceptance loop silently before publishing a sprite or animation. It is an implementation detail: do not expose the review transcript, rejected attempt, or retry count in chat or the UI.

1. Produce the first candidate without updating `.sprite-studio/last-generation.json`.
2. Inspect every static sprite at both 1× and 4×. For animation, also build a contact sheet in playback order and preview at least three complete cycles at the requested FPS.
3. Mark a candidate for repair when its anatomy or motion does not make physical and visual sense: a limb, wing, tail, weapon, or joint floats or disconnects; an attachment stretches away from its body; paired appendages swap identity; a planted contact slides; the subject changes proportions or markings; a part clips; motion is merely whole-body translation; or the loop visibly pops.
4. For winged actors specifically, keep each wing root continuously anchored and overlapping its shoulder/chest region. The root drives the stroke while the membrane folds or lags around its wrist. A whole wing must never orbit, translate, or float as a rigid island. Downstroke/upstroke timing must produce a believable opposing chest reaction without changing wing count, side identity, or depth order.
5. If the first candidate fails, revise the responsible master, rig, masks, pivots, weights, transforms, contacts, or optional polish and render exactly one replacement attempt. Do not stack a second retry. Preserve a focused user reference; regenerate an unfocused generated master only when the master itself is the cause.
6. Inspect the replacement with the same gates. Choose the better of the original and replacement, move only that candidate into `assets/<category>/`, and write a fresh generation manifest. Keep the unused attempt outside `assets/`.
7. Remaining visual imperfections are soft failures. Publish the best structurally valid candidate and end with `GENERATION_WARNING: <concise remaining limitation>`. If the requested motion cannot pass the deterministic rig validator, reduce pose amplitude, simplify the motion, or reduce the frame plan until it validates. An explicit animation request must still publish at least two distinct frames; never report a one-frame fallback as an animation.
8. Use `GENERATION_FAILED` only when no decodable, correctly categorized, workspace-confined asset can be produced at all. Never restore a stale manifest as the result of the current turn.

This loop supplements deterministic validators. Hard safety, file-integrity, workspace-boundary, category, and provenance checks remain mandatory; subjective visual quality should degrade gracefully instead of discarding usable work.
