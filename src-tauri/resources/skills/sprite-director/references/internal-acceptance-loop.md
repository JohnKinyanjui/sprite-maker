# Internal visual acceptance loop

Run this acceptance loop silently before publishing a sprite or animation. It is an implementation detail: do not expose the review transcript, rejected attempt, or retry count in chat or the UI.

1. Produce the first candidate without updating `.sprite-studio/last-generation.json`.
2. Inspect every static sprite at both 1× and 4×. For animation, also build a contact sheet in playback order and preview at least three complete cycles at the requested FPS.
3. Reject a candidate when its anatomy or motion does not make physical and visual sense: a limb, wing, tail, weapon, or joint floats or disconnects; an attachment stretches away from its body; paired appendages swap identity; a planted contact slides; the subject changes proportions or markings; a part clips; motion is merely whole-body translation; or the loop visibly pops.
4. For winged actors specifically, keep each wing root continuously anchored and overlapping its shoulder/chest region. The root drives the stroke while the membrane folds or lags around its wrist. A whole wing must never orbit, translate, or float as a rigid island. Downstroke/upstroke timing must produce a believable opposing chest reaction without changing wing count, side identity, or depth order.
5. If the first candidate fails, revise the responsible master, rig, masks, pivots, weights, transforms, contacts, or optional polish and render exactly one replacement attempt. Do not stack a second retry. Preserve a focused user reference; regenerate an unfocused generated master only when the master itself is the cause.
6. Inspect the replacement with the same gates. Publish only the accepted candidate by moving its final files into `assets/<category>/` and then writing the generation manifest. Keep rejected files outside `assets/` so the app cannot index or display them.
7. If the replacement still fails, return a clear terminal failure instead of publishing broken art. Do not claim success and do not leave a stale manifest pointing at the rejected attempt.

This loop supplements deterministic validators. Passing hashes, dimensions, anchor math, or file checks never overrides an obvious visual or anatomical failure.
