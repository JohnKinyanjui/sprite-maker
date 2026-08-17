# Deterministic character rig harness

Use this harness whenever a character request has more than one frame. ImageGen may create **one master character** before rigging. It must never invent animation frames or a pose sheet; explicit AI Polish/Full redraw may edit completed rough frames only under the frame-polish contract.

## Required pipeline

1. Lock one transparent master.
   - When `Context asset:` is present, use that exact PNG and do not call ImageGen.
   - Otherwise, create one master with ImageGen, convert it to the requested logical canvas, and approve it before animation.
2. Inspect the master at 1× and enlarged nearest-neighbour scale. Identify reusable pixel parts: base/head/torso, left and right limbs, hair or cloth, held equipment, and accessories.
3. Write one `rigVersion: 3` rig JSON with `rigProfile: "human_sprite_rig"` under `.sprite-studio/rigs/`. First record the observed hip, knee, ankle, shoulder, and elbow joints and their visibility. Then use precise polygon masks, bone envelopes, anatomical roles, pivots, anchors, parent attachments, named key poses, and explicit depth. Do not redraw or regenerate a part.
4. Write an exact circular motion table, including the final-to-first transition, then express every frame as transforms of those same pixels.
5. Run `python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<slug>.json` and fix every error before rendering.
6. Run `python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json`.
7. Inspect the rendered loop. In Rig-only mode revise masks, pivots, transforms, and small joint patches. In explicit AI Polish/Full redraw mode, use the frame-polish contract only after this rough-loop inspection.

## Rig specification

This abridged fragment shows the version 2 hierarchy shape. A runnable biped walk must include matching left and right leg chains and 2–32 circular phases.

```json
{
  "rigVersion": 3,
  "rigProfile": "human_sprite_rig",
  "name": "courier_walk",
  "category": "characters",
  "source": "assets/characters/courier_01.png",
  "fps": 10,
  "rootMotion": "in-place",
  "baseZ": 0,
  "proposal": {"morphologyTag": "biped", "motionIntent": "walking loop"},
  "parts": [
    {
      "name": "left_thigh",
      "role": "left_upper_leg",
      "mask": {"polygon": [[20,38],[29,38],[28,52],[20,52]]},
      "pivot": [25,39],
      "anchors": {"hip": [25,39], "knee": [24,51]},
      "z": 1
    },
    {
      "name": "left_shin",
      "role": "left_lower_leg",
      "parent": "left_thigh",
      "attach": {"parentAnchor": "knee", "selfAnchor": "knee"},
      "mask": {"polygon": [[20,50],[28,50],[27,63],[19,63]]},
      "pivot": [24,51],
      "anchors": {"knee": [24,51], "foot": [22,63]},
      "z": 1
    }
  ],
  "frames": [
    {
      "phase": "left_contact",
      "contacts": [{"part": "left_shin", "anchor": "foot", "state": "planted"}],
      "root": {"dy": 0},
      "zOverrides": {"left_shin": 2},
      "transforms": {
        "left_thigh": {"rotate": -8},
        "left_shin": {"rotate": 12}
      },
      "ik": [{
        "chain": ["left_thigh", "left_shin"],
        "endAnchor": "foot",
        "target": [22,63],
        "bend": 1
      }]
    }
  ]
}
```

Masks support `rect: [x,y,width,height]` or a polygon. Named anchors and pivots use source-space coordinates. Child transforms are local and inherit the parent transform; `attach.parentAnchor` and `attach.selfAnchor` must stay coincident. Transforms support integer-friendly `dx`, `dy`, `rotate`, `scaleX`, and `scaleY`. `root.dx` and `root.dy` move the complete source asset. Version 1 rigs remain valid as flat rigs, but new articulated animation must use version 2. Use `underlay` or `overlay` renderer commands only for tiny joint/occlusion repairs. Never use them to redraw the character.

For leg and arm chains, prefer the deterministic two-bone IK solver over hand-compensated rotations. `ik.chain` contains the directly parented upper and lower parts plus an optional foot/hand child; `endAnchor` belongs to the last part, `target` is a locked-canvas point, and `bend` is `1` or `-1`. With a third part, `endRotation` keeps the sole, hand, or tool at a stable world angle. The renderer writes the solved local rotations into `transforms`, and validation rejects unreachable targets. A direct transform may also use `worldRotate` to counter inherited rotation, but never combine it with `rotate`.

Every visible source pixel has one owner: stable base or one movable part. A small intentional joint seam may use `overlapMode: "joint-cap"`; broad overlap, a transformed part retained in the base, and duplicated limb pixels are invalid. `baseZ`, part `z`, and per-frame `zOverrides` share one depth order so far and near limbs can cross correctly.

## Motion mechanics

Before rendering, write a frame table naming elapsed seconds, `phase`, planted contact anchors, leading/trailing foot, hip height, arm opposition, and permitted secondary motion. Derive cadence, stride distance, and world speed from the real-world physical envelope instead of applying one generic walk speed to every body size. For `rootMotion: "in-place"`, planted anchors remain fixed on the canvas while the body passes over them and horizontal root displacement does not accumulate. Use `"baked"` only for explicitly requested travel.

- 4-frame walk: left contact, left passing, right contact, right passing.
- 6-frame walk: left contact, left down, left passing, right contact, right down, right passing.
- 8-frame walk: left contact, down, passing, up, right contact, down, passing, up.

Rotate limbs around their actual shoulder or hip pivots. Arms oppose legs. Use a restrained one-pixel root arc for down/up poses. Hair, cloth, and accessories may lag by one frame, but the face, head, torso, costume construction, palette, and equipment pixels remain byte-for-byte sourced from the master.

For every looping action, let the AI propose enough recovery poses to close the cycle. Walks and runs end on the complementary support phase that leads into the opening contact. Idles reverse their breathing/secondary arc. Attacks include recoil and return-to-ready. Jumps and hops include landing compression and settle before the opening anticipation resumes. Do not duplicate the first frame as the last frame.

## Acceptance gates

Reject and revise the rig when:

1. A part mask cuts unrelated pixels or leaves a visible transparent hole.
2. A joint disconnects, doubles, or changes thickness unexpectedly.
3. Feet do not exchange support or visibly slide without body travel.
4. The pivot or ground line drifts outside the planned root arc.
5. The action cannot be read while playing at 1×.
6. Any generatively edited frame lacks its deterministic rough pose, raw repair, normalization report, or drift validation.
7. Two poses produce identical PNG hashes without an explicitly documented internal hold.
8. The final-to-first transition changes support, root position, silhouette, or secondary motion more abruptly than an ordinary adjacent transition.
9. A declared planted anchor slides, an attached joint separates, a visible pixel clips at the canvas edge, or limb depth contradicts the pose.

At tiny target sizes, subpixel rotations can collapse to identical raster frames. In that case, increase the arc or use a purposeful integer-pixel translation instead of counting the duplicate as extra motion. `hold: true` permits only a deliberate internal hold; the final frame must never duplicate the first.

The renderer writes ordered PNGs and `.sprite-studio/last-generation.json` itself. A valid rig animation is reproducible: running the same JSON against the same master must produce identical frames.
