# Paired-limb identity contract (biped walk and run cycles)

The hardest failure mode in humanoid animation is leg identity: when the model cannot tell the two legs apart, limbs swap roles mid-cycle, flicker, or merge into one blob. This contract prevents that.

## Naming

- Name limbs by **camera depth, never by screen side**: the `NEAR` limb is closer to the camera; the `FAR` limb is behind it. Screen-left/right is never a valid limb name because a side-view silhouette crosses itself every step.
- Assign the convention from the identity reference in Frame 1 — whichever leg reads as closer to the camera is `NEAR` — and keep it for the entire cycle. If the reference is ambiguous, choose one and stay consistent.
- Every per-frame pose specification names each limb's state explicitly: "NEAR leg: planted at heel under the hip. FAR leg: trailing, toe-off, heel lifted." Never write "the legs swap", "the other leg", or "opposite leg".

## Shading lock

- The `FAR` leg and `FAR` arm render in a visibly darker shade of the same hue — roughly 20–30% darker — in **every** frame. This shading difference is the primary identity signal at 1× scale.
- The `FAR` limb never occludes a `NEAR` limb. In passing poses where the limbs overlap, the `NEAR` limb always draws in front.
- The shading roles are frozen for the whole animation: a limb may never change from dark to light between frames.

## Gait ordering

- Run cycle, one full loop: `NEAR` contact → `NEAR` stance while `FAR` swings through → flight (both airborne) → `FAR` contact → `FAR` stance while `NEAR` swings through → flight → loop. The two legs are never in the same phase.
- Walk cycle: contact, down, passing, up — repeated once per leg. In each passing pose the swinging leg is the one whose name is *not* on the phase.
- Each leg continues its own arc through every crossing. After limbs separate, the `NEAR` leg is still the `NEAR` leg; an identity swap is a defect, not a pose.

## Run-cycle structure (positional)

Image models collapse runs into hops — both legs synchronized — unless the structure is pinned to frame slots. For an 8-frame run cycle, anchor the poses to slots:

| Slot | Pose |
| --- | --- |
| 1 | `NEAR` contact — wide split stance: NEAR leg forward and planted, FAR leg extended behind |
| 2 | `NEAR` compression — weight over the NEAR leg, FAR heel lifting |
| 3 | `NEAR` drive — NEAR toe pushes off, FAR knee driving through |
| 4 | Flight — gathered airborne pose; BOTH legs still visible, FAR leg darker behind the NEAR |
| 5 | `FAR` contact — the mirrored wide split stance: FAR leg forward and planted, NEAR leg extended behind |
| 6 | `FAR` compression |
| 7 | `FAR` drive |
| 8 | Flight return — gathered, both legs visible, leading back to slot 1 |

Hard rules:

- Exactly **two wide split stances per cycle, half a cycle apart** (slots 1 and 5). They are the anchors; every other pose transitions between them. A cycle with only one split stance is a hop — regenerate it.
- At each contact extreme, the two foot endpoints must be separated horizontally by at least 20% of the observed character height (and never fewer than four logical pixels). Tiny angle changes that leave the feet clustered under the hips are a standing twitch, not a run.
- Include two true flight frames with no planted contact, one after each leg drives. A continuous-support sequence is a walk even if its frame names say “run.”
- Gathered and flight poses still show two legs: the `FAR` leg remains visible as the darker overlapping shape behind the `NEAR` leg. The legs never fuse into a single mass.
- Keep the widest split stance inside the canvas with safe padding on every edge — a clipped paw at the boundary fails the sheet.
- For other frame counts keep the same shape: contact → absorb → drive → gathered, twice, evenly spaced around the cycle.

## Rejection

Reject and regenerate any frame where:

- a limb changes its shading role (dark becomes light or vice versa),
- occlusion order flips (`FAR` limb drawn over `NEAR`),
- both legs merge into one indistinguishable silhouette blob for more than one consecutive frame,
- a leg teleports or reverses its arc direction without a contact to justify it.

Reject and regenerate the whole cycle when a run shows fewer than two wide split contact stances — native quality analysis flags this as lost leg alternation.

## Rig-assisted alternative

When limb identity keeps failing, switch to the rig path (`references/native-rig-engine.md`): place NEAR/FAR hip-knee-foot points with distinct z-layers, let the Rust engine render the poses deterministically, then optionally polish the rendered frames with AI while keeping their pose and limb layering canonical.
