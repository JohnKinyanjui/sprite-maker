# Release media provenance

The v0.2 showcase files are deterministic exports of assets generated and tested in Sprite Studio’s local end-to-end workspace on 2026-08-10.

| Export | Source | Frames | Playback |
| --- | --- | ---: | ---: |
| `rabbit-hop.gif` | `forest_rabbit_hop_forward_polish_v1` | 8 | 8 FPS |
| `dragon-flight.gif` | `cozy_chibi_dragon_flight` | 12 | 8 FPS |
| `centipede-crawl.gif` | `cave_centipede_crawl` | 12 | 12 FPS |
| `grasslands-pack.gif` | `green-grasslands-nature-pack` | 8 assets | 2.86 previews/s |
| `grasslands-terrain.png` | `beautiful_grasslands_ponds_tileset` | 1 atlas | Static |
| `sprite-studio-v0.2-showcase.gif` | The four animated showcases above | 40 | 10 FPS |

The GIF wrapper uses a fixed 960×540 dark presentation canvas, Arial text, nearest-neighbor sprite scaling, and a 256-color optimized palette. The compact social reel is 800×450 with full-frame disposal at scene boundaries. Source PNGs are never resampled with a smoothing filter.

Regenerate the media with:

```bash
./scripts/generate-release-media.sh /path/to/sprite-studio-workspace
```

The supplied workspace must contain the exact named rig outputs and Green Grasslands pack assets referenced by the script. ImageMagick 7 is required.
