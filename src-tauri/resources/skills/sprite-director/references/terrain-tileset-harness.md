# Terrain tileset atlas harness

Use this harness for a complete terrain, tilemap, or tileset request. The deliverable is one coherent atlas image, not a batch of independent sprite files.

## 1. Inspect the visual reference

When a reference image is attached, inspect the actual pixels before planning. Treat it as guidance for atlas organization, tile scale, edge language, palette restraint, texture density, and top-down readability. Do not copy its exact artwork or protected pack contents.

State the observed:

- base tile unit and atlas aspect ratio;
- ground-fill texture and edge thickness;
- outer and inner corner construction;
- cliff or elevation treatment;
- transparent gutters and occupied regions;
- palette roles and light direction.

Do not infer these details from the filename.

## 2. Generate one atlas master

For a new tileset, use the installed `imagegen` skill and call `image_gen__imagegen` once. Supply every attached visual reference through `referenced_image_paths`. Ask for one original, orthographic top-down pixel-art terrain atlas on a transparent background with no labels, UI, grid lines, watermark, mockup, perspective view, or repeated preview panels.

The generated master must be one image containing a coordinated tile family. Never call ImageGen once per tile and never emit several terrain PNGs. Save the source under `.sprite-studio/imagegen-sources/<slug>/tileset-master.png`, inspect it, then normalize it to the routed logical canvas with nearest-neighbour sampling.

Do not use magenta, fuchsia, hot pink, or purple as a chroma-key background. Prefer direct alpha. Unless the user explicitly requested those hues in the terrain palette, the final atlas must contain no opaque magenta/pink boundary fringe.

If `image_gen__imagegen` is unavailable, stop and report that the required capability is unavailable. Do not replace the atlas with unrelated primitive tiles.

## 3. Required atlas coverage

Use a 32×32 logical base grid unless the user explicitly requests another unit. Compose the atlas as a few large, readable macro-regions with transparent gutters, following the presentation logic of a production terrain sheet. The viewer must immediately understand how the ground, edges, corners, and elevation look together without mentally assembling dozens of tiny cells.

Do not pack the canvas as a dense grid of small separated tile thumbnails. Single-cell variants are secondary. At least half of the occupied pixels must belong to large assembled examples that span several tile units, such as a broad ground platform, long edge strips, a connected corner section, and a full cliff face.

Include at minimum:

1. a large repeatable center-fill platform at least 4×3 base tiles;
2. isolated single-tile and compact block variants;
3. north, south, east, and west edge strips;
4. all four outer corners;
5. all four inner or concave corners;
6. narrow horizontal and vertical strips;
7. a visibly connected top-to-cliff region at least three tiles wide, plus cliff wall segments when elevation is part of the request;
8. at least two restrained texture variants that remain palette-compatible.

Every shared boundary must use the same edge thickness, color order, and lighting. Adjacent cells must meet without a gap, doubled outline, brightness jump, or mismatched texture seam. Keep decoration away from join boundaries unless it intentionally continues into the neighboring tile.

## 4. Single-file output contract

Write exactly one final PNG to `assets/terrain/<slug>_tileset.png`. Do not cut the atlas into separate tile assets and do not create an animation for it.

After writing the final PNG, run `python3 .sprite-studio/terrain_cleanup.py assets/terrain/<slug>_tileset.png` unless the user explicitly requested pink, magenta, fuchsia, or purple terrain. The cleanup removes only strongly magenta pixels within two pixels of existing transparency. Run it a second time and require `removedFringePixels` to be `0` before accepting the atlas.

Write `.sprite-studio/last-generation.json` with:

- the atlas name;
- category `terrain`;
- FPS `1`;
- a one-element `files` array containing only the atlas PNG;
- the UTC timestamp under `generatedAt`.

The final response must link the single atlas, report its dimensions and base tile unit, and briefly list the included terrain transitions. Never present the atlas cells as separate generated sprites.

## 5. Tileset quality gate

Reject and repair the atlas when any of these fail:

1. More than one final terrain PNG was produced.
2. The atlas is a dense gallery of tiny disconnected cells instead of a few readable assembled terrain regions.
3. Required edges or corners are missing.
4. Corners do not match the straight edge thickness.
5. Fill, edge, and cliff tiles change palette or light direction.
6. The output contains opaque background pixels in intended gutters.
7. A 2×2 test map reveals seams, gaps, or doubled borders.
8. The attached reference was copied rather than translated into an original tileset.
9. Less than half of the occupied atlas area demonstrates assembled terrain larger than one base tile.
10. Any unrequested opaque magenta/pink boundary-fringe pixels remain after deterministic cleanup.
