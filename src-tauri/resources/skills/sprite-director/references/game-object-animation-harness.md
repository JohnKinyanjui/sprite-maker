# Deterministic game-object rig harness

Use this harness for animated props, environmental objects, machinery, pickups, weapons, doors, chests, vehicles, plants, and other non-character game objects.

ImageGen or the deterministic sprite renderer may create one transparent master. Animation frames must then come only from `.sprite-studio/sprite_rig.py`; never generate a pose sheet or redraw the object per frame.

## Category is semantic, not inherited

The routed harness brief is authoritative for output category. Set the rig JSON `category` to that exact plural category and write every generated frame beneath `assets/<category>/`. Never copy the source master's folder merely because an older or misfiled asset lives there.

- trees, bushes, plants, rocks, ground pieces, and tiles use `terrain`;
- chests, doors, machines, vehicles, torches, turrets, weapons, pickups, and other movable objects use `props`;
- transient smoke, sparks, flashes, and explosions use `effects`.

An input such as `assets/characters/tree_01.png` can therefore be the locked master for a new `terrain` animation. The renderer may read that source in place, but its outputs and manifest category must still follow the routed semantic category.

## Object decomposition

Inspect the master and define only the parts that actually move. Examples:

- chest: base, lid, latch;
- door: frame, door slab, handle;
- vehicle: body, wheels, suspension, lights;
- turret: base, rotating head, barrel, muzzle flash attachment;
- plant: trunk/stem, branch or leaf clusters;
- weapon: grip/base, blade or moving mechanism;
- machine: housing, gears, piston, indicator.

Each part gets a precise rect or polygon mask, a physical pivot, and a z order. Keep the immobile pixels in the base layer. Use rotations around hinges/axles and integer translations along rails or recoil directions. Use root motion only when the complete object intentionally moves.

Write the rig under `.sprite-studio/rigs/<slug>.json` and run:

```bash
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json
```

Use the same JSON shape documented by the deterministic character rig harness. The renderer supports characters, terrain, props, and effects categories.

## Object acceptance gates

- the object silhouette and material pixels remain sourced from one master;
- hinges, axles, sockets, and attachment points remain connected;
- moving parts do not expose unexplained transparent holes;
- the motion has believable anticipation, travel, impact, and settle poses when relevant;
- hash every rendered PNG and reject accidental duplicate frames; a repeated hash is allowed only for an explicitly documented hold or loop closure;
- at very small resolutions, if different rotations quantize to identical pixels, increase the arc or add a purposeful integer-pixel translation while keeping the fixed base locked;
- the loop returns cleanly to its first transform;
- rerendering the rig produces identical PNG bytes.
