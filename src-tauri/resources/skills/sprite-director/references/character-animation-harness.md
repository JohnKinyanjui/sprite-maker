# Deterministic character rig harness

Use this harness whenever a character request has more than one frame. ImageGen may create **one master character** before rigging. It must never invent animation frames or a pose sheet; explicit AI Polish/Full redraw may edit completed rough frames only under the frame-polish contract.

## Required pipeline

1. Lock one transparent master.
   - When `Context asset:` is present, use that exact PNG and do not call ImageGen.
   - Otherwise, create one master with ImageGen, convert it to the requested logical canvas, and approve it before animation.
2. Inspect the master at 1× and enlarged nearest-neighbour scale. Identify reusable pixel parts: base/head/torso, left and right limbs, hair or cloth, held equipment, and accessories.
3. Write one rig JSON under `.sprite-studio/rigs/`. Use precise polygon masks, anatomical pivots, and explicit z order. Do not redraw or regenerate a part.
4. Write an exact circular motion table, including the final-to-first transition, then express every frame as transforms of those same pixels.
5. Run `python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<slug>.json` and fix every error before rendering.
6. Run `python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json`.
7. Inspect the rendered loop. In Rig-only mode revise masks, pivots, transforms, and small joint patches. In explicit AI Polish/Full redraw mode, use the frame-polish contract only after this rough-loop inspection.

## Rig specification

```json
{
  "name": "courier_walk",
  "category": "characters",
  "source": "assets/characters/courier_01.png",
  "fps": 10,
  "parts": [
    {
      "name": "left_leg",
      "mask": {"polygon": [[20,38],[29,38],[30,63],[19,63]]},
      "pivot": [25,39],
      "z": 1
    }
  ],
  "frames": [
    {
      "root": {"dy": 0},
      "transforms": {
        "left_leg": {"rotate": -8, "dx": -1, "dy": 0}
      }
    }
  ]
}
```

Masks support `rect: [x,y,width,height]` or a polygon. Transforms support integer-friendly `dx`, `dy`, `rotate`, `scaleX`, and `scaleY`. `root.dx` and `root.dy` move the complete locked asset. Use `underlay` or `overlay` renderer commands only for tiny joint/occlusion repairs. Never use them to redraw the character.

## Motion mechanics

Before rendering, write a frame table naming elapsed seconds, support foot, leading/trailing foot, hip height, arm opposition, and permitted secondary motion. Derive cadence, stride distance, and world speed from the real-world physical envelope instead of applying one generic walk speed to every body size.

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
7. Two poses produce identical PNG hashes without an explicitly documented hold or loop closure.
8. The final-to-first transition changes support, root position, silhouette, or secondary motion more abruptly than an ordinary adjacent transition.

At tiny target sizes, subpixel rotations can collapse to identical raster frames. In that case, increase the arc or use a purposeful integer-pixel translation instead of counting the duplicate as extra motion.

The renderer writes ordered PNGs and `.sprite-studio/last-generation.json` itself. A valid rig animation is reproducible: running the same JSON against the same master must produce identical frames.
