# AI rig planning contract

For every non-effect animation, separate planning from rendering.

## Stage 0 — confirm the movement

Before proposing a rig, identify the requested **motion verb** and its physical meaning. A subject name is not a motion: “animate this rabbit” is incomplete, while “make this rabbit hop forward” is actionable.

- When the prompt contains a clear motion, restate it as `MOTION INTENT: <verb> — <one-sentence mechanics>` and continue.
- When the prompt does not say how the subject should move, ask one concise question and stop before rendering: `How should this <observed subject> move?` Offer 3–4 anatomy-appropriate examples.
- Infer suggestions from the visually observed subject, never only its filename: a rabbit can hop, bound, pounce, or perform an ear/nose idle; a centipede crawls with a leg wave; a bird flaps, takes off, glides, or lands; a tree sways, rustles, bends, or reacts to impact; a chest opens, closes, shakes, or bounces.
- Do not replace natural articulated motion with generic whole-image bobbing, sliding, scaling, or alternating frames. Root translation is allowed only as the consequence of a planned gait, jump arc, recoil, or other physical action.
- The motion intent becomes part of the rig proposal and every acceptance check. If the requested verb conflicts with visible anatomy, explain the conflict and ask the user to choose a feasible alternative.
- Apply the embedded real-world motion and scale contract next. Unless the user provided explicit physical or stylized values, establish `PHYSICAL ENVELOPE` in meters, meters per second, and seconds before selecting transforms or frame timing.

## Stage 0.5 — plan the loop closure

Treat every animation as a seamless loop unless the user explicitly requests a one-shot action or a non-looping final hold. Before choosing the frame count, the AI must propose the missing recovery, settle, reverse, or follow-through phases needed to return naturally to the opening state. Do this internally without making the user design the loop.

- State `LOOP INTENT: seamless — <last-to-first transition>` or `LOOP INTENT: one-shot — <explicit user reason>` in the proposal.
- The first frame is the loop's opening state. Do **not** copy it into the final frame; playback already returns from the last frame to the first, and a duplicate creates an unwanted pause.
- Write the frame table as a circular sequence. For every part, compare the final transform with the opening transform and ensure the last-to-first change is no larger or more abrupt than an ordinary adjacent-frame change.
- Locomotion cycles must end on the complementary contact/recovery pose that flows into the opening contact. Keep sprite-sheet locomotion in place unless the user explicitly requests baked root travel; report world displacement separately rather than accumulating root `dx` that snaps backward at the loop boundary.
- Idles and sways return through a damped reverse path. Attacks include recoil and recovery. Jumps and hops include landing and settle. Segmented creatures return through the continuing body/leg wave. Effects return to a compatible emission/opacity state or are explicitly marked one-shot.
- If the proposed frame budget cannot contain both the readable action and its closure, the AI should recommend more frames within Auto limits. With a fixed user frame count, simplify secondary motion before sacrificing loop closure.

## Stage 1 — AI rig proposal

## Required visual inspection

When a master/reference is attached, it is a real image input. Inspect it visually before writing or validating any rig. Do not derive the subject, anatomy, masks, or pivots from its filename, category, chat title, or the user's description. First record an observation summary covering the visible subject/object, facing direction, occupied pixel bounds, silhouette, articulated regions, physical joints, grounded/contact pixels, occlusions, and ambiguous areas. Assign one evidence-based `MORPHOLOGY TAG`: `biped`, `quadruped`, `hexapod`, `segmented-many-leg`, `serpentine`, `winged`, `amorphous`, or `rigid-object`. If a joint is hidden or unclear, use a conservative rigid region or ask for a clearer master instead of inventing anatomy.

## Motion-readiness and body-part gate

Before accepting a master for articulated movement, state `MOTION READY: yes|no — <reason>` and list the body parts that must change pose. The whole visible subject is never a valid single rigid base for locomotion.

- A quadruped hop requires separate masks for at least the hindlimb/haunch drive, forelimb/shoulder landing, torso or pelvis compression, and available secondary parts such as head/neck, ears, and tail. The hind and fore groups must both receive non-identity transforms.
- A biped walk/run requires separate left/right leg support and arm/upper-body opposition. A segmented creature requires multiple independently phased segment or leg-bank groups. A winged takeoff requires wing and grounded-leg groups.
- Record occluded or merged anatomy. If the required locomotor parts cannot be separated without inventing large hidden regions, mark the master `MOTION READY: no`; do not hide the failure by putting those parts into `stableBase` and moving `root`.
- When there is no focused reference, use ImageGen once to create a motion-ready master in the requested style: clean side/gameplay view, readable joints, separated limbs, sufficient transparent clearance, and no pose sheet.
- `Polish mode: AI polish.` and `Polish mode: Full redraw experimental.` explicitly authorize one automatic motion-ready master revision when the focused reference fails this gate. Create it with ImageGen, guided by the focused reference at maximum identity strength, save it as a named revision, validate it, and continue through rigging and polishing in the same request. Do not stop to ask permission. Preserve the original reference and record both source hashes in provenance.
- In Rig only mode, preserve the focused reference. If it fails the motion-readiness gate, explain the limitation and ask before creating a revised master.
- ImageGen still creates only the master during rig planning. Body-part masks, pivots, repair patches, poses, and rough animation frames remain AI-planned and deterministically rendered. Explicit post-render AI Polish/Full redraw follows the separate frame-polish contract.

When frame mode is Auto, recommend the count only after this visual inspection. State `AI FRAME RECOMMENDATION: N frames — <visual/mechanical reason>`, remain inside the user's minimum/maximum range, and use N consistently in the rig, rendered files, manifest, and final response. A named motion such as “walk” must not automatically mean eight frames.

After that inspection, write a structured proposal before rendering:

- name every movable part and the pixels it owns;
- choose a tight rect or polygon mask for each part;
- place the pivot on the physical joint, hinge, axle, or attachment point;
- assign explicit z order and identify any intentional overlap;
- define the stable base pixels that must never move;
- prove that `stableBase` excludes every limb, segment, wing, hinge, or other part required by the motion intent;
- write a frame table describing elapsed seconds, support/contact, physically scaled root position, and each part transform;
- include an explicit final-to-first row in the frame table describing the closure transition and any contact exchange;
- keep `source` fixed to the one approved master.

Save that proposal as the rig JSON under `.sprite-studio/rigs/`. Then validate it without producing frames:

```bash
python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<slug>.json
```

Treat validation warnings about overlapping masks as review work. Tighten masks or mark `allowOverlap: true` only for intentional joint coverage. Validation errors must be fixed before rendering.

## Stage 2 — deterministic render

After validation passes, run:

```bash
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json
```

The tool records a SHA-256 hash of the locked master in the saved rig and generation manifest. If that master changes, the rig must refuse to render; create a named rig revision instead. Every output frame must therefore be a deterministic transform of pixels from the same verified master.

The AI may revise the proposal—masks, pivots, z order, transforms, and tiny joint repair commands—but it must never independently invent an animation frame. Any explicit post-render polish must retain the rough rig frame as pose authority and pass the frame-polish contract.

## Automatic AI repair loop

Do not hand validation failures back to the user as unfinished work. When validation reports an error or overlap warning, inspect the named part and automatically revise only the rig proposal. Tighten masks, move pivots onto physical joints, correct z order, or adjust transforms, then validate again.

After rendering, inspect the contact sheet and cyclic playback for disconnected joints, holes, duplicate hashes, ground-line drift, unreadable motion, and loop pops. Evaluate the last-to-first transition with the same scrutiny as every in-sequence transition. If closure fails, have the AI revise recovery/settle poses, transforms, or the Auto frame recommendation, then rerender the same rig. Use at most three repair passes per request. Preserve the approved master hash throughout every pass. In explicit AI Polish or Full redraw mode, the one automatic motion-ready revision above is already authorized; only ask the user if another master revision would be required after that attempt.

Report completion only after validation passes and the deterministic rerender is reproducible. Briefly summarize any automatic repairs that were applied.
