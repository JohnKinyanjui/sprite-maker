# ImageGen visual-effects harness

Use this harness for fire, smoke, magic, impacts, explosions, energy slashes, particles, and other transient game VFX.

## Create one high-quality master

When the request needs a new effect design, use the installed `imagegen` skill and call `image_gen__imagegen` exactly once to create one original transparent master/keyframe. Do not ask ImageGen for a sprite sheet, animation strip, multiple frames, text, a logo, or a watermark.

If ACTIVE REFERENCE IMAGES are supplied, inspect them first and pass their local paths through `referenced_image_paths`. Use them only for the requested palette, material, shape language, lighting, or composition. A reference image guides the new effect; it does not replace the new master unless the user explicitly asks to reuse that exact effect.

The ImageGen prompt must specify:

- one isolated game VFX element centered on a fully transparent background;
- a readable silhouette at the requested logical size;
- crisp internal energy, smoke, flame, debris, or particle structure appropriate to the effect;
- the selected style preset and intended blend mode;
- no environment, character, UI, border, text, watermark, or sprite sheet.

### Gameplay composition is non-negotiable

Do not produce a small finished illustration floating on a mostly empty canvas.
At the requested logical size, the visible peak silhouette must occupy roughly 55–80% of the usable canvas dimension while retaining a 2–4 pixel safety margin for particles. Design the master at the **peak frame**, not at an arbitrary travelling frame.

Use the request to choose one of these compositions before generating:

- **Impact / burst / hit / end effect:** lock the impact centre to canvas centre. Start compact, reach a single large readable peak, then separate into a few intentional fragments. Never use a long projectile tail as the primary silhouette.
- **Projectile / lance / bolt:** put the stable gameplay origin at the rear third of the canvas and reserve the leading third for the head. Motion may travel only a small, deliberate distance; do not slide the whole effect across the canvas.
- **Beam / slash:** keep the attachment point stable and make direction readable with a tapered core and a separate halo. Do not turn it into a circular explosion.
- **Zone / trap:** centre the footprint and use a clear rim plus a distinct interior event; do not fill the whole frame with noisy particles.

Every one-shot needs four visible beats: compact ignition, decisive expansion or release, one held peak frame, then structured breakup. The peak must be recognisable when viewed at 1×, and the final fragments must fade only after the peak has had time to read.

Save the accepted source under `.sprite-studio/imagegen-sources/<slug>/master.png`. Inspect it and reject outputs with an opaque background, cropped particles, muddy edges, accidental objects, or unreadable structure.

## Build animation deterministically

ImageGen establishes the art direction once. It must not redraw animation frames independently; explicit AI Polish/Full redraw may edit completed rough frames only under the frame-polish contract.

For multiple frames, derive every frame from the locked master with deterministic raster operations and `.sprite-studio/sprite_rig.py`: integer translation, rotation, scaling, alpha fades, masked reveals, duplicated particle layers, and controlled distortion. For topology-changing effects such as explosions or dissipating smoke, deterministic masks and particles may supplement the locked master, but must preserve its palette, texture, lighting, and shape language.

Stage the requested motion phases clearly: anticipation/ignition, expansion or travel, a held impact/peak, dissipation/recovery, and a clean loop closure when looping. For a one-shot impact effect, use at least eight frames and reserve 1–2 frames for the readable peak before breakup. Reject accidental duplicate frames unless they are an intentional hold.

For looping VFX, the AI must propose a compatible final emission, opacity, shape, and particle-flow state that leads into the opening frame. Preview the last-to-first transition directly; never end at peak impact and snap to ignition. If a naturally terminating effect cannot close without looking artificial, mark it one-shot instead of pretending it is seamless.

Normalize each final frame to the requested transparent logical canvas with nearest-neighbour sampling and write it under `assets/effects/`. Record the requested FPS in the generation manifest.

## Acceptance gates

- the first frame, peak frame, and final frame are visibly distinct;
- the master is authored around the requested gameplay composition (impact, projectile, beam, or zone), with no unrelated empty-canvas padding;
- a contact sheet makes the peak and phase progression obvious; reject a sequence that only shrinks or disintegrates without a visible expansion/contact event;
- the effect stays centered around a stable gameplay origin unless travel is requested;
- alpha edges are clean and no opaque rectangle remains;
- brightness and hue stay coherent across the sequence;
- no frame is independently redrawn by ImageGen without a rough-frame pose guide and valid frame-polish report;
- every PNG has identical dimensions and the exported loop plays smoothly.
