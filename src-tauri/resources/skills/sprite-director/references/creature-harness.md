# Segmented creature harness

Use this harness for monsters, animals, insects, slimes, beasts, and other non-humanoid living game actors. It owns creature readability, anatomy, identity consistency, and deterministic animation.

## Lock one master

When no context asset exists, use the installed `imagegen` skill and call `image_gen__imagegen` once to create one original transparent master. Never ask ImageGen to invent an animation, pose sheet, or unposed separate frames. A focused context asset is the identity reference and remains unchanged. For an explicit animation request, one named motion-ready source-master revision is authorized when required limbs or joints are merged; create it automatically, guided by the focused reference, and continue without asking permission. AI Polish/Full redraw modes may additionally edit completed rough rig frames only under the frame-polish contract. In user-facing text, never describe the user's image as locked.

The master must show one creature in the requested gameplay view, at the requested logical canvas, with a clean silhouette and anatomy that can be segmented. Save the source under `.sprite-studio/imagegen-sources/<slug>/master.png`; write game-ready output under `assets/creatures/`. Use a distinct design with no copied franchise character, logo, text, or watermark.

For a centipede, require a readable head, mandibles or antennae, a consistent chain of armored body segments, paired legs, one tail segment, a fixed ground line, and clear front-to-back direction. Do not merge the legs into an unreadable texture.

Do not call every armored insect a centipede. Use the observed morphology tag to select mechanics:

- `hexapod`: three leg pairs; use two alternating tripod support groups, with planted feet remaining fixed while the opposite tripod lifts and swings;
- `segmented-many-leg`: several repeated leg pairs; propagate a metachronal support wave from front to rear instead of a tripod gait;
- `quadruped`: distinguish walk/trot/gallop contacts and keep the correct diagonal or lateral support sequence;
- `serpentine`: drive a continuous body wave without inventing legs;
- `winged`: separate grounded leg support from wing downstroke/upstroke timing.

For a small hexapod walking loop, prefer enough frames to show both tripod contacts, compression, passing/swing, and recovery without teleporting feet. Within the configured Auto range, recommend the smallest count that keeps all readable leg groups mechanically distinct; use higher counts only when the observed anatomy, pixel scale, and secondary motion justify them.

For quadrupeds, never reuse a standing pose as a fake run. A run or gallop needs independently described near/far fore and hind groups, hind-drive contact, gathered and extended suspension, fore-impact contact, spine compression/extension, head stabilization, and delayed tail counter-motion. Walking and trotting normally have continuous support; running, bounding, and galloping require distinct extended and gathered flight phases. Each upper fore/hind limb must sweep through a visibly useful arc rather than twitching around its standing angle, and the extended-flight paw spread must be materially wider than the gathered-flight paw spread. Inspect the loop at gameplay speed, not only as thumbnails.

For a cheetah or similarly flexible sprinting cat, use a rotary double-suspension gallop. The extended flight follows hind-leg drive: the spine lengthens and the fore and hind paws reach far apart. The gathered flight coils the spine and brings the paws back under the body before the next hind contact. If extended and gathered frames preserve nearly the same standing silhouette, the result is not a run and must be rejected.

## Build the deterministic rig

For more than one frame, inspect the master and write a rig under `.sprite-studio/rigs/<slug>.json`. Render only with:

```bash
python3 .sprite-studio/sprite_rig.py --validate .sprite-studio/rigs/<slug>.json
python3 .sprite-studio/sprite_rig.py .sprite-studio/rigs/<slug>.json
```

Define parts that match the creature's actual anatomy in a `rigVersion: 3` rig and select the matching anatomy-specific `rigProfile`. Typical centipede parts are `head`, `front_body`, individually grouped middle segments, `tail`, left/right antennae, and paired leg banks. Give them semantic roles, named anchors, parent/attachment relationships, bone envelopes, tight polygon masks, physical pivots, and explicit depth. Use `baseZ` and per-frame `zOverrides` when crossing appendages change occlusion. Every frame must reuse the same exclusively owned master pixels; reserve `overlapMode: "joint-cap"` for a small intentional joint seam.

Rigid cutout rotation is not an acceptable shortcut for flexible animal anatomy. Before masking, zoom in and mark the visible shoulder, elbow, wrist, hip, knee/stifle, and ankle/hock positions in the rig's `joints` evidence. A quadruped limb must be an upper/lower/paw chain with IK. A weighted mesh may soften pixels around that proven chain, but it cannot replace missing joints. Use torso, limb-chain, tail, and spine bones as mesh influences. Keep weights normalized and use enough triangles to bend around joints without shearing the whole limb. The renderer samples the source with nearest-neighbour texture lookup, preserving pixel colors while the weighted mesh changes the silhouette continuously.

After masks are applied, inspect the residual base at high zoom. There must be no paw, shin, tail, fur strip, or anti-aliased limb pixel left behind inside a declared bone envelope. Do not conceal remnants with overlay paint or repair pixels; expand the correct anatomical mask and render again.

## Motion mechanics

Plan the action before writing transforms.

First use the real-world physical envelope: estimate body length/height, plausible speed, stride or hop distance, vertical rise, and seconds per cycle for the observed species or closest analogue. Convert these to body lengths and pixels. An explicit user speed, height, gravity, or exaggerated/magical request overrides the estimate.

- crawl: pass a restrained lateral compression wave from head to tail while paired legs execute a phase-shifted leg wave;
- idle: use small antenna, mandible, breathing, or tail motion without sliding the grounded body;
- attack: anticipation, head/mandible strike, impact hold, recoil, and recovery;
- hit: short recoil through the body chain followed by a damped settle;
- death: loss of support progressing along the body, then a readable final hold.

For rabbits, hares, and similar saltatorial quadrupeds, “move forward” should default to suggesting a hop or bound, not a human-like alternating walk. A hop cycle needs readable crouch/compression, hind-leg extension, airborne stretch, forefoot contact, hind-foot recovery, and settle. The root follows a forward-and-up arc while contact feet remain planted during compression and landing. Ear and tail motion lag the body rather than driving it. Use a walk only when the user explicitly requests a cautious quadruped walk.

A rabbit hop may not be implemented by translating a seated rabbit as one rigid layer. Segment at minimum the near hindlimb/haunch drive, near forelimb/shoulder landing, torso/pelvis, and one secondary group; include far-side limbs when visible. The silhouette must change from compressed to extended in the airborne frames. Hind feet push from the ground, the spine lengthens, forefeet reach and accept landing, then hind feet recover underneath. If those joints are fused or hidden in the master, automatically create the single authorized motion-ready revision with ImageGen instead of faking the hop with root `dy`, returning a static result, or stopping for confirmation.

The AI must complete the circular gait, not stop at the most dramatic pose. A rabbit hop returns from landing compression through settle into the next crouch; a quadruped gait ends on the complementary contact that leads into its opening contact; a centipede or serpent continues its phase wave across the last-to-first boundary. When needed, recommend extra Auto frames for recovery rather than cutting the animation off.

Adjacent segments must lag by one or more frames instead of moving identically. Opposite leg banks should alternate support. Every locomotion frame names its gait `phase` and lists only planted contact anchors; attached segment anchors remain connected and planted anchors remain fixed until lift. Default to `rootMotion: "in-place"`; use baked displacement only when explicitly requested. Keep integer-friendly movement at small resolutions and preserve one stable ground line unless the requested action deliberately leaves it.

## Acceptance gates

Reject and revise when any of these fail:

1. Segment count, head direction, palette, markings, or appendages drift.
2. Adjacent segments disconnect, overlap implausibly, or expose transparent holes.
3. All legs move together, frames merely blink, or PNG hashes repeat without a documented hold.
4. The creature slides without coordinated foot support or purposeful root travel.
5. The loop pops instead of returning through a continuous body wave.
6. Outside explicit `Polish mode: AI frames experimental.`, any animation frame was generated independently by ImageGen instead of tracing to a rough rig frame and a valid frame-polish report. In AI-frames mode, reject frames that lack master/prior-frame provenance or continuity validation.
7. The action is unreadable at 1× playback.
8. A locomotion rig moves fewer than two anatomical groups, or a hop lacks distinct hindlimb drive, forelimb landing, and body compression/extension.
9. A parent attachment separates, a planted anchor slides, limb depth is wrong, visible pixels clip, or the final frame duplicates the opening frame.

Rerendering the same rig against the same master must produce identical PNG bytes.
