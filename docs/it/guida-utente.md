# Sprite Studio — Guida utente

Sprite Studio è un workbench desktop *local-first* per creare, animare, testare ed esportare arte 2D per giochi, con agenti AI (Codex, Cursor, Antigravity) e strumenti deterministici in Rust.

Questa guida spiega **come usare l'app** e **come formulare richieste che producano asset utilizzabili**.

---

## Avvio rapido

1. **Aggiungi un progetto** — indica una cartella su disco. PNG, animazioni e metadati restano lì.
2. **Scegli un worktree** — Character, Creature, Environment, VFX, ecc. Ogni worktree organizza chat e asset.
3. **Apri una chat** — descrivi cosa vuoi in linguaggio naturale o con un comando slash.
4. **Controlla l'output** — apri gli sprite a scala pixel, riproduci le animazioni, correggi nella scheda Animate.
5. **Esporta** — sprite sheet con metadati JSON o PNG singoli dalla workspace.

---

## Best practice per i prompt

### Specifica ciò che manca

Il router completa stile e dimensioni dai preset, ma **le tue indicazioni esplicite hanno sempre priorità**. Un buon prompt indica:

- **Soggetto** — chi o cosa (ranger, slime, pozione salute)
- **Azione** — idle, camminata, attacco, lancio incantesimo
- **Canvas** — es. `32×32`, `64×64`, `128×128`
- **Sfondo** — di solito `trasparente`
- **Stile** — pixel art, palette limitata, top-down, isometrico, NES 8-bit, ecc.
- **Numero di frame** — solo se serve un conteggio esatto; altrimenti lascia **Auto**

**Buono**

> Genera un ranger della foresta idle 64×64 in pixel art, sfondo trasparente, palette verde/marrone limitata, silhouette leggibile a 1×.

**Debole**

> Fammi un personaggio.

### Un compito per richiesta

Suddividi il lavoro complesso:

1. Master sprite (`/sprite` o `/character`)
2. Approvazione nel viewer
3. Animazione con `/animate` o **Animate this** dal viewer

Evita “personaggio completo con 12 animazioni” in un solo messaggio, salvo template già collaudati.

### Usa i riferimenti con intenzione

- **Un riferimento identità** per personaggi e creature.
- **Riferimenti di stile** per palette, luce, tratto — non per copiare design protetti.
- In chat puoi focalizzare/sostituire/rimuovere i riferimenti per messaggio.

### Preferisci strip orizzontali per animazioni AI

Per locomozione (walk, run, idle→walk) chiedi **una strip orizzontale** con silhouette coerente, poi **Import strip** in Animate (o MCP `split_strip`). Frame generati separatamente divergono in dimensioni e baseline.

### Descrivi lo stile, non un marchio

Tratti visivi generici (“eroe roguelike compatto”, “action RPG stile SNES”) invece di personaggi protetti. Sprite Studio crea arte originale; i riferimenti guidano solo lo stile.

---

## Comandi slash

| Comando | Quando usarlo |
| --- | --- |
| `/sprite` | Un sprite statico rifinito |
| `/character` | Master personaggio (harness ImageGen) |
| `/animate` | Loop senza soluzione di continuità dal contesto chat |
| `/effect` | VFX animato (magia, colpo, esplosione) |
| `/pack` | Set coordinato di asset statici |
| `/rig` | Punti e ossa su un master esistente |

Anche il linguaggio naturale funziona; il router sceglie la pipeline corretta.

---

## Profilo di generazione (per chat)

Apri il menu **profilo di generazione** in chat (il pulsante ? spiega ogni campo):

| Impostazione | Suggerimento |
| --- | --- |
| **Quality** | Low/Mid/High impostano canvas e FPS |
| **Custom** | Per dimensioni di produzione (8–512 px) |
| **Frame mode** | **Auto** — loop minimo completo; **Fixed** — conteggio esatto |
| **FPS** | Allinea al gioco (6–12 retro, 12–24 più fluido) |
| **Interpolation** | Lascia attiva salvo pose discrete obbligatorie |

Le impostazioni sono **per chat**, non globali.

---

## Schede del workbench

| Scheda | Scopo |
| --- | --- |
| **Chat** | Generazione, riferimenti, card inline |
| **Sprites** | Libreria, viewer, **Animate this** |
| **References** | Immagini di riferimento del worktree |
| **Animate** | Timeline, qualità, aligner, normalize, export |
| **Rig** | Punti, ossa, IK, render deterministico |
| **Sheets** | Sprite sheet + metadati |
| **Packs** | Collezioni statiche coordinate |
| **Playground** | Scala, movimento, offset frame |

Scorciatoie: `Ctrl/⌘+1` … `Ctrl/⌘+8`.

---

## Percorsi per animare personaggi

### A. Rig nativo (consigliato)

1. Genera o importa un **master**.
2. Scheda **Rig** → punti manuali o **Ask AI** / `/rig` in chat.
3. Invia `/animate` con descrizione del movimento — render Rust senza ImageGen per frame.
4. Controlla in **Animate** e **Playground**.

Ideale per: cicli walk/run, oggetti, loop deterministici, costo zero per frame.

### B. AI polish / full redraw

Attiva dalla finestra motion o dalla modalità animazione in chat. L'agente rifinisce o ridisegna i frame grezzi con validazione polish.

Ideale per: eroi ad alto budget visivo.

### C. Import strip + pipeline di produzione

Per strip orizzontali generate dall'AI:

1. **Promote anchor** — nell'inspector asset, **Set as character anchor** sul master approvato.
2. **Import strip** o **Import video** — Animate → pannello Align. Strip con layout `profile`. L'estrazione video usa **ffmpeg incluso** negli installer e estrae frame al FPS scelto.
3. **Normalize** — allinea ogni frame al contratto anchor.
4. **Align** — micro-nudge 1 px se il preview trema ancora.
5. **Accept** — supera il size contract, poi l'export è consentito.

---

## Pipeline post-generazione

| Passo | Dove | Cosa fa |
| --- | --- | --- |
| Promote anchor | Inspector sprite | Salva `.sprite-studio/anchors/<slug>.json` |
| Split strip | Import strip / MCP `split_strip` | Layout `profile` (gutter alpha), recovery foreground, warning grid-ink |
| Estrazione video | Import video / MCP `extract_video_frames` | ffmpeg incluso → sequenza PNG in `assets/imports/` |
| Normalize | Animate / MCP | Allineamento foot-baseline, scala condivisa |
| Align frames | Animate → Align | Offset `offsetX/Y` non distruttivi |
| Size contract | Pannello Animate | Blocca export se canvas/baseline non combaciano |
| Auto-fix contract | Animate → Align → **Auto-fix contract** / MCP `queue_contract_retry` | Loop autonomo: repair deterministico → rigenerazione strip AI → import → re-check. Poll con `get_job` (`jobId`, `stage`, `metadataJson.attempts`) |
| Accept | Animate → Accept | Approva per export |

**Export** e job sprite-sheet sono bloccati finché il contratto non passa o l'animazione non è **Accepted**.

---

## Pannello Quality

Analisi su dimensioni, duplicati, trasparenza, allineamento, chiusura loop e plausibilità del movimento. I punteggi sono **diagnostici**, non giudizi artistici.

- Clic su un warning → salta al frame.
- **Repair** per fix automatici sicuri.
- Ignora falsi positivi senza modificare l'arte.
- Il giudizio finale è la **riproduzione** in Animate e Playground.

---

## Librerie Skills e Arts

- **Skills** — istruzioni riutilizzabili incluse in ogni generazione.
- **Arts** — direzione visiva di default (preset + stile custom).

Imposta Arts nel sidebar; override per chat nel menu stile.

---

## MCP (Cursor / Codex)

`sprite-studio-mcp` headless condivide il database dell'app desktop.

Flusso tipico:

1. `studio_status`
2. `open_workspace`
3. `ensure_conversation`
4. `generate`
5. `promote_anchor` → `split_strip` o `extract_video_frames` → salva animazione → `normalize_animation` → `align_frames` → `check_size_contract` → `set_animation_review_status` accepted
6. `export` o `queue_sprite_sheet`

Vedi il [Riferimento MCP](riferimento-mcp.md) per tutti i tool e i parametri. [README — MCP](../../README.md#mcp-server-headless) per `mcp.json`.

---

## Errori comuni

| Errore | Approccio migliore |
| --- | --- |
| Animare prima di approvare il master | Un master solido + anchor |
| Chiedere 8 frame walk separati | Una strip, poi split + normalize |
| Ignorare il size contract | Normalize o aligner |
| Canvas enorme “per il dettaglio” | Usa 32/64/128; zoom nel viewer |
| Stili misti in una chat | Nuova chat o worktree per direzione artistica |
| Export senza Playground | Controlla piedi, scala, leggibilità |

---

## Sicurezza dei dati

- Repair e normalize creano **nuovi file**; le fonti restano.
- Versioni asset con hash del contenuto.
- Cancellare un export non cancella i frame sorgente.
- Le workspace sono cartelle normali — backup, git, zip come qualsiasi progetto.

---

## Ulteriori letture

- [Riferimento MCP](riferimento-mcp.md) — tool headless incluso `extract_video_frames`
- [README](../../README.md) — funzionalità, build, MCP, layout workspace
- [Confronto pipeline competitiva](pipeline-competitiva.md) — PerfectPixel, Spriterrific, Picasso e altri
- [English guide](../en/user-guide.md) — User guide in English
