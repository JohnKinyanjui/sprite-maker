# Character harness

Use this harness whenever the routed asset kind is `character`. It owns character concept quality, identity consistency, animation preparation, and the final handoff.

## 1. Lock the character brief

Before generating, define these invariants in the ImageGen prompt:

- one original character and a clear gameplay role;
- view direction and body proportions;
- head, eye line, shoulder line, hip line, hands, and grounded foot line;
- hair shape, face landmarks, skin tone, and expression;
- three costume layers: primary silhouette, secondary garment, and one readable accessory;
- palette roles: outline, shadow, base, secondary color, accent, and highlight;
- the selected style preset and logical output canvas;
- no logo, text, watermark, copied franchise character, or unrequested prop.

Named games and supplied references are trait references only. Translate proportion, readability, shape language, and palette restraint into a distinct original design.

## 2. ImageGen is mandatory

Use the installed `imagegen` skill and call `image_gen__imagegen` only when a new master character is required. When an existing context asset is supplied, that asset is the master and no ImageGen call is needed. For animation, follow the deterministic character rig harness: segment the master into reusable pixel parts and render transforms with `.sprite-studio/sprite_rig.py`. Never use ImageGen for animation frames or pose sheets. Do not use primitive JSON rectangles, ellipses, or polygons as the primary character generator.

If `image_gen__imagegen` is not callable, stop and report that the ImageGen capability is unavailable. Never silently replace the requested ImageGen workflow with hand-drawn primitives.

Generate the master first. Save it under `.sprite-studio/imagegen-sources/<slug>/master.png`, inspect it, and reject it when the face, silhouette, hands, costume layers, or selected style are unclear.

## 3. Prepare animation frames

For multiple frames, follow the deterministic character rig harness. Inspect the approved master, define masks and pivots for movable parts, and transform those same source pixels for every frame. Change only the pose and minimum secondary motion needed for the action.

Animation rules:

- keep the same camera, scale, palette, costume, face, hair mass, and equipment;
- keep feet on one baseline and the body center on one pivot;
- use readable contact, passing, up, and down poses for walks;
- use restrained chest, hand, hair, or cloth motion for idles;
- avoid camera movement, generative redraws, changing outlines, or transformations that tear joints;
- inspect the frames as a loop before accepting them.

Use a perfectly flat removable chroma-key background for transparent output. Follow the installed ImageGen skill's chroma-key removal workflow. Do not leave the key color, a cast shadow, a floor plane, or a fringe in final frames.

## 4. Convert to game-ready output

Keep ImageGen source images in `.sprite-studio/imagegen-sources/`. Put only final frames in `assets/characters/`.

Workspace safety is mandatory:

- choose a unique slug for a new character;
- never move, delete, rename, archive, or overwrite unrelated files in `assets/`;
- never use a wildcard or `find assets ... -exec mv` to archive active assets;
- when revising an existing character, archive only the exact frame paths for that character after resolving them individually;
- before writing, list the exact source and destination paths and confirm every destination belongs to the current character slug.

- Pixel RPG: crop consistently, resize with nearest-neighbor sampling to the logical canvas, and reduce noisy colors while preserving the face and silhouette.
- Graphic adventure: preserve clean antialiased edges and the larger logical canvas.
- Cozy chibi: preserve the head-to-body ratio, clean outline, and face readability.

Use ImageMagick when available. The installed ImageGen transparency helper is also allowed. Never stretch frames to a different aspect ratio.

Write `.sprite-studio/last-generation.json` with the final frame paths, name, category `characters`, FPS, and the UTC generation time under the exact camelCase key `generatedAt`. Then run every quality gate.

## 5. Character quality gate

Reject and regenerate when any of these fail:

1. The silhouette is not recognizable at thumbnail size.
2. Eye level, shoulder width, head size, or total height changes between frames.
3. Face, hair, clothing, palette, or accessories drift.
4. The selected style thumbnail is not visibly reflected in shape language and rendering.
5. The feet slide, the pivot jumps, or the loop pops.
6. Transparency, crop, or edge quality is visibly broken.
7. A protected character or exact reference design was copied.
