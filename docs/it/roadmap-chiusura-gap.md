# Roadmap chiusura gap competitivi

Piano di implementazione per **tutti** i gap individuati nell'analisi competitiva. Ogni fase termina con **verify-until-done**: test Rust, `bun run check`, post-task-reviewer, report di chiusura in italiano.

Vedi la versione dettagliata in inglese: [gap-closure-roadmap.md](../en/gap-closure-roadmap.md).

## Riepilogo fasi

| Fase | Gap chiusi | Contenuto principale |
|------|------------|----------------------|
| **1** | G01, G05, G13 | UI Production (harden, clean alpha, snap grid), direction meta in griglia facings |
| **2** | G02, G04 | Export character pack, Production Score unificato |
| **3** | G03 | Catalogo preset motion + coda batch su anchor |
| **4** | G06–G08 | Profilo personaggio, tool MCP rig, allineamento SKILL |
| **5** | G09–G12, G13 | Facing UI, wizard video, dialog contract retry, scoring per frame |
| **6** | G14–G16 | Inpainting, interpolazione rig, optical flow o deferral |
| **7** | G17 | Automazione test e regressioni su fixture |

## Stato

- **Fase 1:** completata (G01, G05, G13 parziale)
- **Fase 2:** completata (G02, G04)
- **Fase 3:** completata (G03) — catalogo preset, `queue_motion_batch`, UI picker + tabella progress
- **Fase 4:** completata (G06, G07, G08) — character profile, MCP rig tools, SKILL allineata
- **Fase 5:** completata (G09–G12, G13 resto) — `score_animation_frames`, `list_facing_checks`, FacingPanel, VideoImportWizard, ContractRetryDialog, badge direction set
- **Fase 6:** completata (G14–G16) — `queue_region_regen`, `interpolate_rig_frames`, optical flow deferral documentato + test feature flag
- **Fase 7:** completata (G17) — `fixture_regression_tests`, helper `test_fixtures`, CI (`bun run check`, `cargo test --lib`, pipeline E2E Linux + macOS)

## Ciclo successivo (gap residui)

Roadmap **Fase 8–13** (G18–G29): [roadmap-gap-residui.md](roadmap-gap-residui.md)
