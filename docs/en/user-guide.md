# Sprite Studio — User guide

Sprite Studio is a local-first desktop workbench for creating, animating, testing, and exporting 2D game art with AI agents (Codex, Cursor, Antigravity) and deterministic Rust tools.

This guide explains **how to use the app** and **how to write requests that produce usable assets**.

---

## Quick start

1. **Add a project** — point Sprite Studio at a folder on disk. All PNGs, animations, and metadata stay in that folder.
2. **Pick a worktree** — Character, Creature, Environment, VFX, etc. Each worktree keeps its own chats and assets organized.
3. **Start a chat** — describe what you want in plain language or with a slash command.
4. **Review output** — open sprites at pixel scale, play animations inline, fix issues in the Animate tab.
5. **Export** — sprite sheets with JSON metadata, or individual PNGs from the workspace.

---

## Best practices for prompts

### Be specific about what is missing

The router fills in style and canvas size from presets, but **your explicit constraints always win**. Good prompts name:

- **Subject** — who or what (forest ranger, slime enemy, health potion)
- **Action** — idle, walk, attack, cast, open, explode
- **Canvas** — e.g. `32×32`, `64×64`, `128×128`
- **Background** — usually `transparent`
- **Style** — pixel art, limited palette, top-down, isometric, NES 8-bit, etc.
- **Frame count** — only when you need an exact number; otherwise leave **Auto**

**Good**

> Generate a 64×64 pixel-art forest ranger idle pose, transparent background, limited green/brown palette, readable silhouette at 1× scale.

**Weak**

> Make a character.

### One job per request

Split complex work:

1. Master sprite first (`/sprite` or `/character`)
2. Approve the master in the sprite viewer
3. Animate with `/animate` or **Animate this** from the viewer

Do not ask for “full game character with 12 animations” in a single message unless you are using a motion template you already trust.

### Use references deliberately

- Attach **one focused identity reference** for characters and creatures.
- Attach **style references** for palette, lighting, or line weight — not to copy a protected design.
- In chat, focus/replace/remove references per message; they do not leak to other chats.

### Prefer strip-first animation for AI frames

For locomotion (walk, run, idle→walk), ask for **one horizontal strip** with a consistent silhouette, then use **Import strip** in the Animate tab (or MCP `split_strip`). Independent frame generations drift in size and foot placement.

### Name your intent, not a trademark

Describe broad traits (“compact roguelike hero”, “SNES-era action RPG”) instead of “make Link” or “like Hollow Knight character X”. Sprite Studio creates original art; references guide style only.

---

## Slash commands

| Command | Use when |
| --- | --- |
| `/sprite` | One static polished sprite |
| `/character` | Character master via ImageGen harness |
| `/animate` | Seamless loop from chat context + motion settings |
| `/effect` | Animated VFX (magic, hit flash, explosion) |
| `/pack` | Coordinated set of static assets (potions, props, fauna) |
| `/rig` | Joint points and bones on an existing master (deterministic poses) |

Plain language works; the harness router picks the right pipeline.

---

## Generation profile (per chat)

Open the **generation profile** menu in chat (help button explains each field):

| Setting | Tip |
| --- | --- |
| **Quality** | Low/Mid/High presets set canvas and FPS defaults |
| **Custom** | Use for production sizes (8–512 px) |
| **Frame mode** | **Auto** — smallest complete loop in range; **Fixed** — exact count |
| **FPS** | Match your game (6–12 for retro, 12–24 for smoother motion) |
| **Interpolation** | Leave on unless you need strictly discrete poses |

Settings are **saved per chat**, not globally.

---

## Workbench tabs

| Tab | Purpose |
| --- | --- |
| **Chat** | Generate, attach references, review inline cards |
| **Sprites** | Browse library, open viewer, **Animate this** |
| **References** | Worktree reference images |
| **Animate** | Timeline, quality, aligner, normalize, export |
| **Rig** | Points, bones, IK, deterministic render |
| **Sheets** | Build sprite sheets + metadata |
| **Packs** | Coordinated static collections |
| **Playground** | Test scale, movement, frame offsets |

Shortcuts: `Ctrl/⌘+1` … `Ctrl/⌘+8` in the workbench.

---

## Character animation paths

### A. Native rig (recommended default)

1. Generate or import a **master** sprite.
2. Open **Rig** tab → place points or **Ask AI** / `/rig` in chat.
3. Send `/animate` with motion description — Sprite Studio renders frames in Rust (no per-frame ImageGen).
4. Review in **Animate** and **Playground**.

Best for: walk/run cycles, game objects, deterministic loops, zero API cost per frame.

### B. AI polish / full redraw

Opt in via the motion dialog or chat animation mode. The agent edits rough rig frames or redraws them; output passes through polish validation.

Best for: hero characters where you accept higher cost and want painterly cleanup.

### C. Strip import + production pipeline

For AI-generated horizontal strips:

1. **Promote anchor** — in the asset inspector, set the approved master as **character anchor** (defines canvas size and foot baseline).
2. **Import strip** or **Import video** — Animate tab → Align panel. Strip uses `profile` layout (alpha gutters + grid-ink warnings). Video extraction uses **bundled ffmpeg** in installed builds and extracts frames at a chosen FPS.
3. **Normalize** — aligns every frame to the anchor contract (`lockFirstFrame` + `sharedScale` in MCP).
4. **Align** — 1 px metadata nudges if preview still jitters.
5. **Accept** — passes size contract, then export is allowed.

---

## Post-generation pipeline (production hardening)

| Step | Where | What it does |
| --- | --- | --- |
| Promote anchor | Sprite inspector | Saves `.sprite-studio/anchors/<slug>.json` |
| Split strip | Animate → Import strip / MCP `split_strip` | `profile` layout (alpha gutters), foreground recovery, grid-ink warnings |
| Video extract | Animate → Import video / MCP `extract_video_frames` | Bundled ffmpeg → PNG sequence under `assets/imports/` |
| Normalize | Animate → Normalize / MCP | Foot-baseline alignment, shared scale |
| Align frames | Animate → Align | Non-destructive `offsetX/Y` per frame |
| Size contract | Animate panel | Blocks export until canvas + baseline match |
| Auto-fix contract | Animate → Align → **Auto-fix contract** / MCP `queue_contract_retry` | Autonomous loop: deterministic repair → AI strip regen → import → re-check. Poll with `get_job` (`jobId`, `stage`, `metadataJson.attempts`) |
| Accept | Animate → Accept | Marks animation approved for export |

**Export** and sprite-sheet jobs are gated until the animation passes the contract or is explicitly **Accepted**.

---

## Quality panel

Quality analysis checks dimensions, duplicates, transparency, alignment, loop closure, and motion plausibility. Scores are **diagnostics**, not artistic grades.

- Click a warning to jump to the frame.
- Use **Repair** for safe automated fixes (transparency, alignment revision).
- Acknowledge false positives without changing art.
- **Playback** in Animate and Playground is the final judge.

---

## Skills and Arts libraries

- **Skills** — reusable instruction blocks injected into every generation in the project.
- **Arts** — default visual direction (built-in presets + custom reference + prompt).

Set workspace Arts in the sidebar; override per chat in the style menu.

---

## MCP (Cursor / Codex)

Headless `sprite-studio-mcp` shares the same database as the desktop app.

Typical flow:

1. `studio_status` — provider ready
2. `open_workspace` — project path
3. `ensure_conversation` — style preset optional
4. `generate` — prompt + dimensions
5. `promote_anchor` → `split_strip` or `extract_video_frames` → save animation → `normalize_animation` → `align_frames` → `check_size_contract` → `set_animation_review_status` accepted
6. `export` or `queue_sprite_sheet`

See [MCP reference](mcp-reference.md) for every tool, parameters, and examples. [README — MCP server](../../README.md#mcp-server-headless) covers `mcp.json` setup.

---

## Common mistakes

| Mistake | Better approach |
| --- | --- |
| Animating before the master is approved | Lock identity with one good master + anchor |
| Asking for 8 separate walk frames | One strip, then split + normalize |
| Ignoring size contract failures | Normalize to anchor or nudge aligner |
| Huge canvas “for detail” | Match game scale (32/64/128); zoom in viewer |
| Mixing styles in one chat | New chat or new worktree per art direction |
| Exporting without testing in Playground | Check foot slide, scale, and readability |

---

## Data safety

- Repairs and normalizations create **new files**; sources are preserved.
- Asset versions are content-hashed.
- Deleting a sheet export does not delete source frames.
- Workspaces are ordinary folders — back up, git, or zip like any project.

---

## Further reading

- [README](../../README.md) — features, build, MCP, workspace layout
- [MCP reference](mcp-reference.md) — all headless tools including `extract_video_frames`
- [Competitive pipeline analysis](competitive-pipeline.md) — how PerfectPixel, Spriterrific, Picasso, and others solve the same problems
- [Italian guide](../it/guida-utente.md) — Guida in italiano
