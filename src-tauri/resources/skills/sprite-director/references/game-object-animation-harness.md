# Deterministic game-object rig harness

Use this harness for animated props, environmental objects, machinery, pickups, weapons, doors, chests, vehicles, plants, and other non-character game objects.

ImageGen or the deterministic sprite renderer may create one transparent master. Rough animation frames must then come from `.sprite-studio/sprite_rig.py`; never generate a pose sheet or invent unrigged poses. Explicit AI Polish/Full redraw may edit completed rough frames only under the frame-polish contract.

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

Before decomposition, require a movement description. If none is present, ask how the object should move and offer mechanics appropriate to what is visibly present: a hinged object opens or closes around its hinge, wheels rotate while a vehicle translates, a plant bends and its foliage lags, machinery cycles through connected axles or pistons, and a pickup may hover or pulse. Never choose generic bobbing merely because it is easy to render.

Apply the real-world physical envelope before timing the rig. Estimate scale, material/mass class, travel speed or angular velocity, acceleration, impact, damping, and cycle duration. Convert meters and seconds through the observed subject scale; explicit user values or clearly magical/cartoon motion override these estimates.

Write new articulated work as a backward-compatible `rigVersion: 2` rig under `.sprite-studio/rigs/<slug>.json`. Use semantic roles, named source-space anchors, parent/attachment relationships, exclusive pixel ownership, `baseZ`, and per-frame `zOverrides` where the object's occlusion changes. Default to `rootMotion: "in-place"`; use `"baked"` only for explicitly requested displacement. Then run:

```bash
python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<slug>.json
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json
```

Use the same JSON shape documented by the deterministic character rig harness. The renderer supports characters, terrain, props, and effects categories.

## Object acceptance gates

- the object silhouette and material pixels remain sourced from one master;
- hinges, axles, sockets, and attachment points remain connected;
- moving parts do not expose unexplained transparent holes;
- the motion has believable anticipation, travel, impact, and settle poses when relevant;
- hash every rendered PNG and reject accidental duplicate frames; a repeated hash is allowed only for an explicitly documented internal `hold: true`, and the final frame may not duplicate the first;
- at very small resolutions, if different rotations quantize to identical pixels, increase the arc or add a purposeful integer-pixel translation while keeping the fixed base locked;
- the loop returns cleanly to its first transform;
- the AI proposes recovery or damped settle poses when the requested motion would otherwise stop abruptly, and the last frame leads into rather than duplicates the first frame;
- rerendering the rig produces identical PNG bytes.
