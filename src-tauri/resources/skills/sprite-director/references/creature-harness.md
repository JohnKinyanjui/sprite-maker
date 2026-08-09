# Segmented creature harness

Use this harness for monsters, animals, insects, slimes, beasts, and other non-humanoid living game actors. It owns creature readability, anatomy, identity consistency, and deterministic animation.

## Lock one master

When no context asset exists, use the installed `imagegen` skill and call `image_gen__imagegen` once to create one original transparent master. Never ask ImageGen for an animation, pose sheet, or separate frames. A context asset is already the locked master and must not be regenerated.

The master must show one creature in the requested gameplay view, at the requested logical canvas, with a clean silhouette and anatomy that can be segmented. Save the source under `.sprite-studio/imagegen-sources/<slug>/master.png`; write game-ready output under `assets/creatures/`. Use a distinct design with no copied franchise character, logo, text, or watermark.

For a centipede, require a readable head, mandibles or antennae, a consistent chain of armored body segments, paired legs, one tail segment, a fixed ground line, and clear front-to-back direction. Do not merge the legs into an unreadable texture.

## Build the deterministic rig

For more than one frame, inspect the master and write a rig under `.sprite-studio/rigs/<slug>.json`. Render only with:

```bash
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json
```

Define parts that match the creature's actual anatomy. Typical centipede parts are `head`, `front_body`, individually grouped middle segments, `tail`, left/right antennae, and paired leg banks. Use tight polygon masks, physical pivots, explicit z order, and a stable base layer. Every frame must reuse the same master pixels.

## Motion mechanics

Plan the action before writing transforms.

- crawl: pass a restrained lateral compression wave from head to tail while paired legs execute a phase-shifted leg wave;
- idle: use small antenna, mandible, breathing, or tail motion without sliding the grounded body;
- attack: anticipation, head/mandible strike, impact hold, recoil, and recovery;
- hit: short recoil through the body chain followed by a damped settle;
- death: loss of support progressing along the body, then a readable final hold.

Adjacent segments must lag by one or more frames instead of moving identically. Opposite leg banks should alternate support. Keep integer-friendly movement at small resolutions and preserve one stable ground line unless the requested action deliberately leaves it.

## Acceptance gates

Reject and revise when any of these fail:

1. Segment count, head direction, palette, markings, or appendages drift.
2. Adjacent segments disconnect, overlap implausibly, or expose transparent holes.
3. All legs move together, frames merely blink, or PNG hashes repeat without a documented hold.
4. The creature slides without coordinated foot support or purposeful root travel.
5. The loop pops instead of returning through a continuous body wave.
6. Any animation frame was generated independently by ImageGen.
7. The action is unreadable at 1× playback.

Rerendering the same rig against the same master must produce identical PNG bytes.
