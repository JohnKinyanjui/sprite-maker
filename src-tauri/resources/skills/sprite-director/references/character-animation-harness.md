# Deterministic character rig harness

Use this harness whenever a character request has more than one frame. ImageGen may create **one master character only**. ImageGen must never create animation frames or a pose sheet.

## Required pipeline

1. Lock one transparent master.
   - When `Context asset:` is present, use that exact PNG and do not call ImageGen.
   - Otherwise, create one master with ImageGen, convert it to the requested logical canvas, and approve it before animation.
2. Inspect the master at 1× and enlarged nearest-neighbour scale. Identify reusable pixel parts: base/head/torso, left and right limbs, hair or cloth, held equipment, and accessories.
3. Write one rig JSON under `.sprite-studio/rigs/`. Use precise polygon masks, anatomical pivots, and explicit z order. Do not redraw or regenerate a part.
4. Write an exact motion table, then express every frame as transforms of those same pixels.
5. Run `python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json`.
6. Inspect the rendered loop. Revise only masks, pivots, transforms, and small joint patches; never ask ImageGen for frame corrections.

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

Before rendering, write a frame table naming the support foot, leading/trailing foot, hip height, arm opposition, and permitted secondary motion.

- 4-frame walk: left contact, left passing, right contact, right passing.
- 6-frame walk: left contact, left down, left passing, right contact, right down, right passing.
- 8-frame walk: left contact, down, passing, up, right contact, down, passing, up.

Rotate limbs around their actual shoulder or hip pivots. Arms oppose legs. Use a restrained one-pixel root arc for down/up poses. Hair, cloth, and accessories may lag by one frame, but the face, head, torso, costume construction, palette, and equipment pixels remain byte-for-byte sourced from the master.

## Acceptance gates

Reject and revise the rig when:

1. A part mask cuts unrelated pixels or leaves a visible transparent hole.
2. A joint disconnects, doubles, or changes thickness unexpectedly.
3. Feet do not exchange support or visibly slide without body travel.
4. The pivot or ground line drifts outside the planned root arc.
5. The action cannot be read while playing at 1×.
6. Any animation frame came from ImageGen or another generative redraw.
7. Two poses produce identical PNG hashes without an explicitly documented hold or loop closure.

At tiny target sizes, subpixel rotations can collapse to identical raster frames. In that case, increase the arc or use a purposeful integer-pixel translation instead of counting the duplicate as extra motion.

The renderer writes ordered PNGs and `.sprite-studio/last-generation.json` itself. A valid rig animation is reproducible: running the same JSON against the same master must produce identical frames.
