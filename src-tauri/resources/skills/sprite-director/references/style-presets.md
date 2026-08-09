# Sprite style presets

Use these only when the user did not specify production dimensions. Treat named games as high-level inspiration and keep the output original.

| User language | Logical canvas | Default deliverable | FPS | Direction |
|---|---:|---:|---:|---|
| Pixel RPG, cozy farming RPG, 16-bit | 48×64 character | 4-frame gentle idle/walk-ready set | 8 | front or user-specified |
| Graphic adventure, angular concept art | 192×256 character | 4-frame idle set | 8 | three-quarter/front |
| Cozy chibi, rounded cartoon | 128×160 character | 4-frame idle set | 8 | front or three-quarter |
| classic top-down adventure | 16×24 character | 4-frame idle/walk-ready set | 8 | down/front |
| SNES-era action RPG | 24×32 character | 4-frame idle | 10 | three-quarter/front |
| compact roguelike | 16×16 character | 2-frame idle | 6 | top-down/front hybrid |
| pixel platformer | 32×32 character | 6-frame run-ready set | 12 | side view |
| fighting or large action character | 64×64 character | 6-frame idle/action set | 12 | side view |
| inventory icon, item, prop | 24×24 prop | 1 frame | 1 | three-quarter icon |
| UI icon, tiny pickup | 16×16 prop | 1 frame | 1 | centered icon |
| portrait or dialogue bust | 64×64 character | 1 frame | 1 | front/three-quarter |
| terrain tile | 16×16 terrain | 1 seamless tile | 1 | top-down |
| impact, sparkle, smoke effect | 32×32 effect | 5-frame effect | 12 | centered |

## Inference rules

- A simple request for a style-inspired character should produce a useful animated set, not ask for size.
- Use the logical canvas dimensions exactly; Sprite Studio previews can scale pixels without changing the source.
- For a single object or icon, do not invent animation.
- For walking, running, attacking, idling, spellcasting, or effects, generate multiple frames even when the user omits a frame count.
- Use 3–6 main colors plus outline and highlight for small sprites. Add colors only when readability requires them.
- Place character feet within the bottom two rows and keep the pivot stable across frames.
- A transparent pixel is not a palette color. Avoid semi-transparent edge pixels for crisp pixel art.

## Style translation

- “Stardew-like” means cozy farming-RPG proportions, warm readable colors, compact pixel clusters, and an original outfit and face. Use the Pixel RPG preset; never reproduce an existing farmer sprite.
- “Pokemon-like” means compact top-down readability, strong color blocking, and an original silhouette; never reproduce an existing creature or trainer.
- “Zelda-like” means readable top-down adventure proportions and iconic equipment shapes; never reproduce Link or franchise symbols.
- When the user names any living artist, studio, or game, describe the transferable traits and create a distinct original asset.
