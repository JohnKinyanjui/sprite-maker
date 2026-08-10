# Asset pack harness

Use this harness for `/pack` and for explicit requests to create an asset pack, collection, bundle, or coordinated set.

## Deliverable

- Generate one coherent pack of **static, individually usable game assets**. If the user gives a positive item count, that count is a hard deliverable; do not cap it at 12 and do not ask them to choose a smaller pack. If no count is supplied, choose 6.
- Infer the pack kind from the request: animals and monsters go to `assets/creatures/`; people go to `assets/characters/`; objects, pickups, furniture, tools, and decorations go to `assets/props/`; effects go to `assets/effects/`.
- A pack may contain multiple categories only when the user explicitly requests a mixed pack.
- Every item must use the same projection, pixel density, outline treatment, lighting direction, palette logic, and logical canvas.
- Every item is a separate transparent PNG. Do not turn pack items into animation frames.

## Creation workflow

1. State a short `PACK PLAN` listing the items, shared style, projection, canvas, and palette rules.
2. Use ImageGen to create a high-quality contact sheet with isolated, non-overlapping items on a uniform removable background. This is the visual master for consistency. For packs larger than 12 items, work in consistent batches of at most 12 items per sheet, reuse the first sheet's palette/projection/scale rules for every later batch, and combine every batch into one pack manifest with exactly the requested total.
3. Inspect the sheet, crop each item, remove the background, fit it to the shared logical canvas, and save with nearest-neighbor scaling whenever pixel art is requested.
4. Reject and repair inconsistent scale, lighting, outlines, palette drift, clipped silhouettes, leftover matte colors, and accidental duplicates.
5. Save `.sprite-studio/last-generation.json` with `"kind": "pack"`, all generated asset paths, and FPS 1; these files are a collection, not animation frames.
6. Save `.sprite-studio/packs/<pack-id>.json` using exactly this schema:

```json
{
  "id": "lowercase-kebab-id",
  "name": "Human readable pack name",
  "description": "One concise sentence",
  "style": "The selected or explicitly requested art style",
  "kind": "animals, objects, characters, effects, or mixed",
  "files": ["assets/creatures/example.png"],
  "createdAt": "ISO-8601 timestamp"
}
```

## Style rules

- An art style written in the user request overrides the chat or workspace preset.
- Otherwise use the selected style context exactly.
- Translate references to existing games or artists into original visual traits; never copy recognizable characters, logos, or proprietary assets.
- Pack consistency matters more than individual ornament. At 1× size, all items must look like they belong to the same shipped game.

## Final response

Report the pack name, exact item count, shared canvas, style, and categories. Sprite Studio renders the pack manifest as a grouped component in chat, so do not print raw folder links or individual asset links. Do not describe pack items as frames.
