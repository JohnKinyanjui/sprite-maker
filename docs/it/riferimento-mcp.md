# Riferimento MCP Sprite Studio

Server headless: `sprite-studio-mcp` (stdio). Condivide database SQLite e cartelle workspace con l'app desktop.

Setup: [README — MCP server](../../README.md#mcp-server-headless).

**Non** è il helper Python per-workspace `.sprite-studio/sprite_rig_mcp.py` (solo rig renderer).

---

## Indice tool

| Categoria | Tool |
| --- | --- |
| Workspace | `studio_status`, `open_workspace` |
| Chat / generazione | `ensure_conversation`, `attach_references`, `generate`, `get_generation`, `cancel_generation` |
| Asset | `list_artifacts`, `list_assets`, `list_packs`, `export` |
| Pipeline (hardening AI) | `promote_anchor`, `orient_anchor`, `check_anchor_facing`, `get_anchor`, `list_anchors`, `list_facing_checks`, `score_strip`, `split_strip`, `extract_video_frames`, `clean_alpha`, `normalize_animation`, `snap_to_pixel_grid`, `harden_animation`, `align_frames`, `check_size_contract`, `check_character_contract`, `mirror_animation`, `queue_direction_set`, `queue_motion_batch`, `queue_region_regen`, `retry_size_contract`, `queue_contract_retry`, `finalize_contract_retry`, `set_animation_review_status`, `score_animation_frames`, `get_production_score`, `export_character_pack` |
| Rig | `save_rig`, `render_rig_animation`, `suggest_rig_points`, `analyze_rig_fit`, `interpolate_rig_frames` |
| Job | `queue_sprite_sheet`, `queue_procedural_vfx`, `get_job`, `quality_report` |

---

## Pipeline di produzione (ordine consigliato)

### Agent-first (due tool)

Quando i frame esistono già su un'animazione:

1. **`harden_animation`** — catena idempotente: split opzionale → `clean_alpha` → `normalize_animation` → `snap_to_pixel_grid` opzionale → `check_size_contract` → scrive `last-generation.json`.
2. Se `contractReport.passed` è false e c'è una chat, richiama **`harden_animation`** con `options: { cleanAlpha: false, normalize: false, queueContractRetry: true }` e fai poll di **`get_job`** sul `jobId` restituito.
3. **`set_animation_review_status`** con `status: "accepted"` — sblocca export.

### Manuale / step-by-step

Per animazioni personaggio generate con AI:

1. **`promote_anchor`** — blocca canvas, foot baseline e pivot centroide dal master approvato.
2. **`score_strip`** (opzionale) — inferisce frame count, QC grid-ink e layout consigliato prima dell'import.
3. **`split_strip`** o **`extract_video_frames`** — importa frame come asset (`assetIds`).
4. Salva un'animazione (app o agente) con quegli `assetIds`.
5. **`normalize_animation`** — `lockFirstFrame: true`, `sharedScale: true`, `anchorSlug` impostato.
6. **`align_frames`** — micro-nudge opzionali.
7. **`check_size_contract`** — diagnostica (canvas, baseline, identità, centroide, motion).
8. **`retry_size_contract`** — fix deterministico; opzionale `regenerate: true` + `conversationId` per una rigenerazione AI manuale.
9. **`queue_contract_retry`** — loop **autonomo**: repair → rigenerazione AI (poll) → import strip → re-check (job `contract_retry`; poll con `get_job` e `jobId`).
10. **`finalize_contract_retry`** — path manuale dopo nuova strip.
11. **`check_character_contract`** (opzionale) — valida tutte le animazioni del worktree personaggio contro lo stesso anchor.
12. **`set_animation_review_status`** con `status: "accepted"` — sblocca export e sprite-sheet.
13. **`export`** o **`queue_sprite_sheet`** con `exportFormat` / `metadataFormat` per Aseprite, TexturePacker o Godot.

---

## Tool pipeline (dettaglio)

### `promote_anchor`

Promuove un asset in `.sprite-studio/anchors/<slug>.json`.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `assetId` | string | sì | Master sprite |
| `slug` | string | no | Default dal nome asset |
| `autoOrient` | boolean | no | Default `false` — rileva il facing e ruota verso ovest (side) o nord (top-down) |
| `view` | string | no | `side` o `top-down`; inferito dall'asset se omesso |

### `orient_anchor`

Ruota un anchor promosso verso il facing canonico (vista laterale → ovest). Aggiorna il sidecar anchor e `facing-check.json`.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `workspaceId` | string | sì |
| `slug` | string | sì |

### `check_anchor_facing`

Report read-only del facing di un anchor (euristiche massa/skew). Non modifica gli asset.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `workspaceId` | string | sì |
| `slug` | string | sì |

Restituisce `FacingCheckReport`: `detectedFacing`, `canonicalFacing`, `confidence`, `status`.

### `mirror_animation`

Deriva un'animazione specchiata senza AI (es. `walk-e` da `walk-w`). Usa il rig se presente, altrimenti flip dei PNG.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `animationId` | string | sì | Animazione sorgente |
| `targetFacing` | string | sì | `w`, `e`, `n`, `s`, `nw`, `ne`, `sw`, `se` |
| `sourceFacing` | string | no | Da meta direzione o suffisso nome |
| `anchorSlug` | string | no | Per normalize + contract |
| `rigId` | string | no | Forza mirror via rig |

### `queue_direction_set`

Job in background: specchia i facing derivabili, opzionalmente genera con AI i facing mancanti se `conversationId` è impostato, poi `check_character_contract`.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `worktreeId` | string | sì | Worktree personaggio |
| `sourceAnimationId` | string | sì | Facing canonico (di solito ovest) |
| `anchorSlug` | string | sì | |
| `motion` | string | sì | es. `walk`, `run`, `idle` |
| `set` | string | sì | `"4"` o `"8"` |
| `conversationId` | string | no | Genera con AI i facing non specchiabili |

Poll con **`get_job`**. Metadata: `mirroredAnimationIds`, `generatedAnimationIds`, `pendingFacings`, `characterContract`. Stato `failed` se il character contract non passa.

### `get_anchor` / `list_anchors`

Legge gli anchor promossi. `list_anchors` include `pivot` e `baselineY`.

### `score_strip`

QC pre-import su una strip: inferisce il frame count dalle valli alpha, rileva grid ink e suggerisce un layout. Non scrive asset.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `sourcePath` | string | sì | Path assoluto immagine strip |
| `frameCount` | number | no | Se impostato, valuta quella partizione; se omesso, inferisce il count |
| `layout` | string | no | Default `auto` — prova gutter profile, fallback DP |

Restituisce `StripScoreReport` (`suggestedFrameCount`, `inferenceConfidence`, `gridInkDetected`, `segmentWidths`, `layoutRecommended`, `warnings`, …).

### `split_strip`

Divide strip orizzontale, verticale o griglia in PNG singoli.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `sourcePath` | string | sì | Path assoluto immagine strip |
| `layout` | string | sì | `profile` (gutter alpha, **preferito per AI**), `auto` (inferenza + fallback profile/DP), `horizontal`, `vertical`, `grid` |
| `frameCount` | number | sì | Celle attese; usa **`0` con `layout: "auto"`** per inferire il count |
| `columns` | number | no | Solo layout `grid` |
| `recoverForeground` | boolean | no | Default `true` — ritaglia margini vuoti |
| `category` | string | no | Cartella asset, default `characters` |

Restituisce `SplitStripResult` con `assetIds`, `relativePaths`, `framePaths`, `warnings` opzionali, più `suggestedFrameCount`, `frameCountUsed`, `inferenceConfidence`, `layoutUsed` quando applicabile.

### `extract_video_frames`

Estrae frame PNG da un video con **ffmpeg incluso** negli installer o con ffmpeg di sistema nel `PATH` (fallback dev/MCP).

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `videoPath` | string | sì | Path assoluto `.mp4`, `.webm`, `.mov`, ecc. |
| `fps` | number | no | Default `10` — frequenza estrazione |

I frame finiscono in `assets/imports/<nome-video>-frames/` e vengono indicizzati come gli altri import.

Stesso formato di risposta di `split_strip` (senza warning grid-ink).

**Errori:** `ffmpeg_missing`, `ffmpeg_failed`, `ffmpeg_no_frames`, `video_not_found`.

**Uso tipico:** workflow video→sprite (Scenario, ecc.) — estrai frame, poi `normalize_animation`.

### `normalize_animation`

Allinea ogni frame al contratto anchor (foot baseline + centroide X).

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `animationId` | string | sì | |
| `anchorSlug` | string | no | Primo anchor promosso se omesso |
| `lockFirstFrame` | boolean | no | Lock verticale dal frame 1 |
| `sharedScale` | boolean | no | Scala unica per tutti i frame |
| `padding` | number | no | Padding interno al canvas |

### `clean_alpha`

Rimuove aloni semi-trasparenti, defringe il magenta e pulisce pixel orfani su ogni PNG dell'animazione.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `animationId` | string | sì |

### `snap_to_pixel_grid`

Allinea `offsetX` / `offsetY` a una griglia pixel. Con `gridSize` > 1 quantizza anche i canali RGB opachi.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `animationId` | string | sì | |
| `gridSize` | number | no | Default `1`. Usa `2` o `4` per quantizzazione palette |

### `harden_animation`

Orchestratore idempotente post-gen. Esegue la catena di produzione e restituisce un report aggregato.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `animationId` | string | sì | |
| `anchorSlug` | string | no | |
| `sourcePath` | string | no | Con `frameCount` esegue split prima della catena |
| `frameCount` | number | no | Obbligatorio con `sourcePath` |
| `conversationId` | string | no | Obbligatorio se `options.queueContractRetry` è true |
| `options.cleanAlpha` | boolean | no | Default `true` |
| `options.normalize` | boolean | no | Default `true` |
| `options.snapGrid` | boolean | no | Default `false` |
| `options.gridSize` | number | no | Per `snap_to_pixel_grid` |
| `options.splitLayout` | string | no | `profile`, `dp`, `auto` |
| `options.queueContractRetry` | boolean | no | Default `false` |

**Retry:** seconda chiamata solo per recovery contratto: `{ cleanAlpha: false, normalize: false, queueContractRetry: true }`.

### `align_frames`

Nudge `offsetX` / `offsetY` non distruttivi sui metadati.

### `check_size_contract`

Valida l'animazione rispetto all'anchor (quando applicabile).

**Scope:** solo worktree `character`, `creature`, `animation`, o con `anchorSlug` esplicito. VFX, props e tileset saltano il contratto.

**Codici:** `canvas_size`, `baseline_drift`, `identity_drift`, `identity_warning`, `centroid_drift`, `motion_still`, `missing_anchor` (warning non bloccante).

### `check_character_contract`

Esegue `check_size_contract` su **ogni animazione** del worktree personaggio/creatura/animazione e aggrega il risultato.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `worktreeId` | string | sì | Deve essere `character`, `creature` o `animation` |
| `anchorSlug` | string | no | Default: primo anchor promosso |

Ritorna `CharacterContractReport` con `passed`, `animationCount`, `anchorSlug` e `animationReports` per animazione.

### `export`

Esporta strip orizzontale PNG + metadata sidecar per una animazione.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `kind` | string | sì | Usa `animation` |
| `id` | string | sì | Id animazione |
| `destination` | string | no | Sottocartella in `exports/` |
| `exportFormat` | string | no | `sprite-studio` (default), `aseprite-json`, `texturepacker`, `godot-spriteframes` |

Il formato `sprite-studio` include i campi fixed-cell (`anchorSlug`, `baselineY`, `pivot`, `sourceSize`, `trimOffset`, `spriteSourceSize`). Gli altri formati portano pivot/baseline in `meta` (e Godot in `metadata/fixed_cell`).

### `queue_sprite_sheet`

Stessi valori `metadataFormat` di `export`. Godot scrive `.tres` accanto al PNG; gli altri `.json`.

### `set_animation_review_status`

`status`: `draft`, `accepted`, `rejected`. `accepted` richiede assenza di violazioni **bloccanti**.

---

## Esempio: video → walk cycle

```json
{
  "workspaceId": "<id>",
  "videoPath": "C:/games/mio-progetto/refs/walk-preview.mp4",
  "fps": 12
}
```

Poi crea l'animazione con gli `assetIds` restituiti, `normalize_animation`, `set_animation_review_status` accepted, export.

### `queue_contract_retry`

Loop autonomo in background (`kind: contract_retry`).

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `animationId` | string | sì | |
| `conversationId` | string | sì | Chat per la rigenerazione strip |
| `anchorSlug` | string | no | |
| `maxAiAttempts` | number | no | Default `2` |
| `maxDeterministicPasses` | number | no | Default `2` |

Ritorna subito `jobId` e un `finalReport` **pre-job** (`passed` sempre `false` all'enqueue). Poll **`get_job`** fino a `completed`/`failed`/`cancelled`. Usa `stage` per il progresso live e parsa `metadataJson` (stringa JSON) per `{ "attempts": [...] }`. Poi **`check_size_contract`**.

### `queue_motion_batch`

Accoda motion dal catalogo su un worktree personaggio. Poll **`get_job`** con il `jobId` restituito.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `worktreeId` | string | sì | Worktree `character` |
| `anchorSlug` | string | sì | Anchor promosso |
| `motions` | string[] | no | Id catalogo; vuoto = tutti i preset abilitati |
| `set` | string | sì | `"4"` o `"8"` |
| `conversationId` | string | no | Per generazione AI nel batch |
| `hardenAfter` | boolean | no | Harden dopo ogni motion |
| `seedAnimationId` | string | no | Riferimento west canonico |
| `onlyMissing` | boolean | no | Salta motion già soddisfatte sul facing canonico |

### `queue_region_regen`

Inpainting regionale su un frame. Richiede chat aperta.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `animationId` | string | sì |
| `frameIndex` | number | sì |
| `regions` | array | sì — `{ x, y, width, height }` |
| `conversationId` | string | sì |
| `prompt` | string | no |

### `export_character_pack`

Esporta tutte le animazioni del worktree in una cartella relativa al workspace.

| Parametro | Tipo | Obbligatorio | Note |
| --- | --- | --- | --- |
| `workspaceId` | string | sì | |
| `worktreeId` | string | sì | |
| `destination` | string | sì | Sottocartella sotto `exports/` |
| `anchorSlug` | string | no | |
| `metadataFormat` | string | no | Come `export` |
| `includeAnimatedPreviews` | boolean | no | GIF in `previews/` |

Flusso tipico: **`queue_motion_batch`** → poll job → **`export_character_pack`**.

### `get_production_score`

Punteggio aggregato worktree (size contract, character contract, quality). Soglia CI sulla fixture walk: **60**.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `workspaceId` | string | sì |
| `worktreeId` | string | sì |
| `anchorSlug` | string | no |

### `score_animation_frames`

Metriche per-frame (delta motion, stabilità) su una animazione.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `animationId` | string | sì |

### `interpolate_rig_frames`

Inserisce pose interpolate tra due keyframe del rig. Corpo `rig` come **`save_rig`**.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `rig` | object | sì |
| `fromIndex` | number | sì |
| `toIndex` | number | sì |
| `steps` | number | sì | 1–16 pose inserite |

### `list_facing_checks`

Storico controlli facing da `.sprite-studio/facing-checks.json`.

| Parametro | Tipo | Obbligatorio |
| --- | --- | --- |
| `workspaceId` | string | sì |
| `slug` | string | no | Filtra per anchor |

---

## Prerequisiti

| Requisito | Tool interessati |
| --- | --- |
| Codex CLI (`codex login`) | `generate` (provider default) |
| ffmpeg incluso (installer) o PATH (dev) | `extract_video_frames`, **Import video** in UI |
| Anchor promosso | `normalize_animation`, `check_size_contract` per personaggi |

---

## Documentazione correlata

- [Guida utente](guida-utente.md)
- [Confronto pipeline](pipeline-competitiva.md)
- [English MCP reference](../en/mcp-reference.md)
