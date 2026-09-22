# Come altri tool risolvono i problemi delle sprite AI — e cosa integra Sprite Studio

Confronto tra approcci di settore e funzionalità di Sprite Studio.

## Matrice problemi

| Problema | Pattern di settore | Sprite Studio |
| --- | --- | --- |
| Celle strip che si sovrappongono | **Taglio profile/auto** (valli alpha, DP) — PerfectPixel, Picasso `--auto` | **`score_strip`** pre-QC; `split_strip` layout **`profile`** / **`auto`** (gutter + fallback DP); avviso **grid ink** |
| Jitter orizzontale con braccia estese | **Centroide alpha-weighted**, non bbox — PerfectPixel, Spriterrific | `normalize_animation` allinea il **centroide X** al pivot anchor |
| Piedi che scivolano | **Foot pivot + baseline** dal pixel opaco più basso | Anchor `baseline_y` dal **piede alpha**; contract ±1px |
| Deriva identità tra frame | **dHash / istogramma** vs riferimento | Contract **`identity_drift`** (dHash vs master anchor) |
| Animazione statica | **Motion presence** — PerfectPixel | Contract **`motion_still`** se frame adiacenti quasi uguali |
| Bordi griglia disegnati dall'AI | **Sheet QC grid** — vibedgames | Warning strip se rilevata **grid ink** |
| Split a larghezza fissa errato | Auto slice > griglia fissa — Picasso, Scenario | Import UI usa **`profile`**; `horizontal` resta disponibile |
| Video → frame sprite | **ffmpeg extract** a FPS fisso — workflow Scenario | MCP/UI **`extract_video_frames`** → `assets/imports/` poi normalize |
| Molti stati animazione | **Modello custom / reference lock** — Scenario, PixelLab | Reference chat + **anchor** + rig `/animate` |
| Micro-fix non distruttivi | **Offset metadata** — Spriterrific, sprite-gen | Pannello **Align** + Playground |
| Transizioni deboli | **In-between da motion** — pose rig, optical flow (spesso rimandato) | **Motion-aware midpoint**; **rig bridge**; **in-between renderizzati** (`interpolate_rig_animation_frames`) |
| Video denso da curare | **Keyframe picker / subsample** | **`subsample_video_frames`** + wizard video step 4 (filmstrip) |
| Maschera regen regionale | **Brush / rect** | **`queue_region_regen`** con `brushStrokes` + `FrameMaskOverlay` |
| Palette worktree | **Quantizzazione palette** — PerfectPixel | **`quantize_worktree_palette`** (8/16/32) |
| Cleanup alpha manuale | **Gomma versionata** | **`paint_frame_alpha`** + `restore_asset_version` |
| Handoff engine | **Manifest con pivot** — sprite-gen | Export JSON con `trimOffset`, `spriteSourceSize` |
| Set multi-direzione (4/8-way) | **Facing check + mirror** — PerfectPixel, Spriterrific | **`check_anchor_facing`** / `orient_anchor`; **`mirror_animation`**; **`queue_direction_set`**; contract cross-facing **`check_character_contract`** |

## Inventario gap G18–G29

| ID | Priorità | Capacità | Stato |
| --- | --- | --- | --- |
| G18 | P1 | Catalogo motion UX — categorie, preset estesi, genera tutti i mancanti | Shipped (`MotionBatchPanel`, `queue_motion_batch` `onlyMissing`) |
| G19 | P1 | Export anteprima GIF animata (singola + character pack) | Shipped (`export_gif_preview`, pack `includeAnimatedPreviews`) |
| G20 | P1 | Gate production score prima dell'export pack | Shipped (`get_production_score`, guard export pack) |
| G21 | P1 | Nomi tool MCP allineati all'orchestrazione pipeline | Shipped (`harden_animation`, `queue_motion_batch`, ecc.) |
| G22 | P2 | Maschera visiva (brush + rettangolo) per regional regen | Shipped (`FrameMaskOverlay`, `queue_region_regen.brushStrokes`) |
| G23 | P2 | Curatela import video denso (subsample + filmstrip) | Shipped (`subsample_video_frames`, wizard video step 4) |
| G24 | P2 | Preset game-view anchor (platformer, top-down, RTS oblique) | Shipped (`PromoteAnchorDialog`, alias `normalize_view`) |
| G25 | P2 | Quantizzazione palette condivisa worktree (8/16/32) | Shipped (`quantize_worktree_palette`, opzione harden palette) |
| G26 | P2 | Cleanup alpha non distruttivo (gomma + restore versione) | Shipped (`paint_frame_alpha`, `restore_asset_version`) |
| G27 | P3 | In-between pixel da rig renderizzato nella timeline | Shipped (`interpolate_rig_animation_frames`, UI rig) |
| G28 | P3 | Gallery sessioni produzione (archivio generazioni per worktree) | Shipped (`ProductionSessionsGallery`, tab Media) |
| G29 | P3 | Optical flow Path B — deferral permanente | Deferred (nessun feature flag; rig bridge + midpoint + G27) |

## Workflow consigliato (produzione)

1. **Master** → **Promote anchor** (piede + centroide misurati dai pixel).
2. **Una strip orizzontale** per azione.
3. **`score_strip`** poi **`split_strip`** `layout: "profile"` / `"auto"` o **`extract_video_frames`** (video AI), poi `recoverForeground` / normalize.
4. Salva animazione → **`normalize_animation`** (`lockFirstFrame`, `sharedScale`).
5. **`align_frames`** se serve.
6. **`check_size_contract`**.
7. Se fallisce: **`queue_contract_retry`** (loop autonomo) o path manuale **`retry_size_contract`** / **`finalize_contract_retry`**.
8. **`accepted`** → export.

## Workflow multi-facing (4/8 direzioni)

1. **Promote anchor** con `autoOrient: true` (side view canonica **W**).
2. **`check_anchor_facing`** prima delle strip walk/run — correggi con **`orient_anchor`** o UI “Flip to canonical (W)”.
3. Genera solo la facing canonica (es. walk-**w**), poi **`mirror_animation`** o **`harden_animation`** con `options.deriveMirroredFacing: "e"`.
4. Per set completi, **`queue_direction_set`** (`set: "4"` o `"8"`) deriva i mirror possibili e lancia **`check_character_contract`** (`facing_identity_drift`, `facing_baseline_mismatch`).
5. L'export è bloccato finché la famiglia di direzioni non passa il contract cross-facing (se presente metadata direzione).

## Riparazione transizioni (Fase 5)

`optimize_animation_frames` / `regenerate_transition`:

1. **Rig bridge** — con `rigId` nel manifest, renderizza una pose rig intermedia invece del blend RGBA.
2. **Motion-aware midpoint** — altrimenti allinea gli offset metadata a metà prima del blend pixel.
3. **In-between renderizzati** — `interpolate_rig_animation_frames` inserisce frame PNG nell'animazione.
4. **Optical flow** — **deferral permanente (G29 Path B)**; nessun feature flag.

## Cosa non automatizziamo ancora

| Tecnica | Motivo |
| --- | --- |
| Modello personaggio cloud-only (Scenario/PixelLab) | Fuori scope OSS local-first; usare reference + anchor |
| In-between con optical flow | Deferral permanente (G29 Path B); rig bridge + motion-aware + in-between renderizzati |
| Scoring percettivo stile PerfectPixel | `score_strip` copre inferenza count + grid ink; hash percettivo completo rimandato |

## Riferimenti

- [PerfectPixel](https://pp.andrew-ai.app/)
- [Spriterrific](https://pypi.org/project/spriterrific/)
- [sprite-gen](https://github.com/bokjk/sprite-gen)
- [Picasso](https://github.com/jacklenzotti/picasso)
- [Scenario](https://www.scenario.com/blog/ai-sprite-generator)
