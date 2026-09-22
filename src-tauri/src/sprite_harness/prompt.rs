use crate::{
    models::GenerationOptions,
    motion_planner::build_motion_plan,
    workspace::{inject_python_commands, launcher_command_line, resolve_python_launcher},
};

use super::routing::{
    asset_identity_context, explicit_count, explicit_size, has_explicit_asset_subject, infer_brief,
    inferred_style, HarnessKind,
};

const DIRECTOR_SKILL: &str = include_str!("../../resources/skills/sprite-director/SKILL.md");
const STYLE_PRESETS: &str =
    include_str!("../../resources/skills/sprite-director/references/style-presets.md");
const QUALITY_GATES: &str =
    include_str!("../../resources/skills/sprite-director/references/quality-gates.md");
const CHARACTER_HARNESS: &str =
    include_str!("../../resources/skills/sprite-director/references/character-harness.md");
const CHARACTER_ANIMATION_HARNESS: &str = include_str!(
    "../../resources/skills/sprite-director/references/character-animation-harness.md"
);
const CREATURE_HARNESS: &str =
    include_str!("../../resources/skills/sprite-director/references/creature-harness.md");
const EFFECT_HARNESS: &str =
    include_str!("../../resources/skills/sprite-director/references/effect-harness.md");
const TERRAIN_TILESET_HARNESS: &str =
    include_str!("../../resources/skills/sprite-director/references/terrain-tileset-harness.md");
const GAME_OBJECT_ANIMATION_HARNESS: &str = include_str!(
    "../../resources/skills/sprite-director/references/game-object-animation-harness.md"
);
const ASSET_PACK_HARNESS: &str =
    include_str!("../../resources/skills/sprite-director/references/asset-pack-harness.md");
const RIG_PLANNING_CONTRACT: &str =
    include_str!("../../resources/skills/sprite-director/references/rig-planning-contract.md");
const PHYSICAL_MOTION_CONTRACT: &str =
    include_str!("../../resources/skills/sprite-director/references/physical-motion-contract.md");
const AI_FRAME_POLISH_CONTRACT: &str =
    include_str!("../../resources/skills/sprite-director/references/ai-frame-polish-contract.md");
const INTERNAL_ACCEPTANCE_LOOP: &str =
    include_str!("../../resources/skills/sprite-director/references/internal-acceptance-loop.md");
const BIPED_LOCOMOTION_IDENTITY: &str =
    include_str!("../../resources/skills/sprite-director/references/biped-locomotion-identity.md");
const CURSOR_IMAGE_CONTRACT: &str =
    include_str!("../../resources/skills/sprite-director/references/cursor-image-contract.md");
const ANTIGRAVITY_IMAGE_CONTRACT: &str =
    include_str!("../../resources/skills/sprite-director/references/antigravity-image-contract.md");

/// Shared contract for AI rig-point suggestions. Used directly by the
/// `ai_suggest_rig_points` command and embedded in `/rig` chat turns.
pub fn rig_suggestion_prompt(motion: &str, morphology: &str, width: u32, height: u32) -> String {
    let canvas = if width > 0 && height > 0 {
        format!("CANVAS: {width}x{height} source pixels")
    } else {
        "CANVAS: read the exact pixel dimensions from the attached master image".to_string()
    };
    let motion_text = if motion.trim().is_empty() {
        "None — provide bind-pose points and bones only; omit \"frames\".".to_string()
    } else {
        motion.trim().to_string()
    };
    format!(
        "RIG POINT ANALYST CONTRACT\n\
You are the rig-point analyst inside Sprite Studio. Study the attached sprite master and propose a native rig: named points and capsule bones that Sprite Studio's deterministic Rust rig engine renders animation from. You never render pixels for this request — the engine derives each bone's pixels automatically from your capsules.\n\n\
Answer with exactly ONE fenced code block tagged `rig-suggestion` containing JSON in this shape:\n\n\
```rig-suggestion\n\
{{\n\
  \"morphology\": \"biped\",\n\
  \"points\": [\n\
    {{\"name\": \"neck\", \"kind\": \"joint\", \"x\": 24, \"y\": 18, \"confidence\": 0.9, \"note\": \"chin line\"}},\n\
    {{\"name\": \"foot_r\", \"kind\": \"contact\", \"x\": 27, \"y\": 58, \"confidence\": 0.85}}\n\
  ],\n\
  \"bones\": [\n\
    {{\"name\": \"torso\", \"start\": \"neck\", \"end\": \"hip\", \"radius\": 6, \"parent\": null, \"z\": 5}},\n\
    {{\"name\": \"shin_r\", \"start\": \"knee_r\", \"end\": \"foot_r\", \"radius\": 3, \"parent\": \"thigh_r\", \"z\": 9}}\n\
  ],\n\
  \"frames\": [\n\
    {{\"phase\": \"contact\", \"rootDx\": 0, \"rootDy\": 0,\n\
      \"transforms\": [{{\"bone\": \"thigh_r\", \"rotate\": 18, \"dx\": 0, \"dy\": 0, \"scaleX\": 1, \"scaleY\": 1}}],\n\
      \"contacts\": [{{\"bone\": \"shin_r\", \"x\": 27, \"y\": 58, \"bend\": 1}}]}}\n\
  ],\n\
  \"reasoning\": \"one short paragraph: observed anatomy, joint evidence, pose logic\"\n\
}}\n\
```\n\n\
RULES\n\
- Coordinate space: source pixels of the attached master, origin top-left, +y down. Read joint positions off the visible anatomy; never guess or average blindly.\n\
- Point kinds: `joint` (articulation), `anchor` (extremity or tip), `contact` (planted point such as a foot), `pivot` (rotation center).\n\
- Every bone is a capsule from `start` to `end` (point names) whose `radius` in pixels covers that limb's thickness. Capsules must tile the silhouette so every opaque pixel is claimed by exactly one bone; give thin limbs small radii and the torso/head larger radii.\n\
- `parent` chains: limbs attach to torso or head, never cyclic. `z` layering: far-side limbs 1–3, torso/head 4–6, near-side limbs 7–10.\n\
- Include `frames` only when a motion intent is given. Establish visibly different contact/extreme key poses first, then add breakdowns between them. Adjacent changes should be smooth, but the full cycle must not be a near-static bob: animate at least two anatomical bones across 8 degrees, 1.5 source pixels, or 5% scale so the motion survives pixel quantization. Values such as +/-2 degrees or 0.999-1.001 scale are not useful animation. Keep planted points fixed through `contacts`, use `rotate` in degrees (positive = clockwise on screen), and make the final frame lead smoothly back into the first. Use `rootDx`/`rootDy` for whole-body offset only.\n\
- Body motion is mandatory for locomotion. Add semantic torso/pelvis/spine/body bones and animate them visibly: biped compression and counter-rotation, quadruped shoulder/pelvis/spine flexion, winged chest reaction, or a travelling wave through multiple serpentine body segments. Moving limbs beneath an unchanged body or adding only root bob is invalid.\n\
- After the JSON block write at most three sentences of summary. Nothing before the block.\n\n\
MOTION INTENT\n{motion_text}\n\n\
MORPHOLOGY HINT: {morphology} (biped | quadruped | winged | serpentine | object | amorphous — override only if the art clearly differs)\n\n\
{canvas}"
    )
}

pub fn studio_prompt(
    prompt: &str,
    context: Option<&str>,
    generation: Option<&GenerationOptions>,
    command: Option<&str>,
    agent_provider: Option<&str>,
    anchor_contract: Option<&str>,
    native_rig_master_only: bool,
) -> String {
    let context = context.unwrap_or("").trim();
    if native_rig_master_only {
        return apply_native_image_contract(
            format!(
                "You are the creation agent inside Sprite Studio. Create exactly ONE transparent motion-ready source master for a later native rig animation. Sprite Studio will suggest joint points, save the rig, and render frames locally after this master is saved. Do not write mask rigs, pose sheets, animation frames, or call `.sprite-studio/sprite_rig.py`.\n\nMASTER HANDOFF CONTRACT\n- Source and keyed intermediates may be kept under `.sprite-studio/imagegen-sources/<slug>/`, but they are not the deliverable.\n- Save or copy the single accepted transparent master to `assets/characters/<slug>.png`. Do not create any other asset file.\n- After the final master exists, write a fresh `.sprite-studio/last-generation.json` containing `kind: \"sprite\"`, the chosen `name`, `category: \"characters\"`, `fps: 1`, `files: [\"assets/characters/<slug>.png\"]`, `source: \"assets/characters/<slug>.png\"`, and a current ISO-8601 `generatedAt` timestamp.\n- The manifest and its listed asset must be newly written for this request. Never reuse or restore an older manifest.\n- Mention the final `assets/characters/<slug>.png` path in the response. Do not report success unless both that asset and the fresh manifest exist.\n\nSELECTED CHAT CONTEXT\n{}\n\nROUTED HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nUSER REQUEST\n{}",
                if context.is_empty() { "No saved style override." } else { context },
                CHARACTER_HARNESS,
                STYLE_PRESETS,
                QUALITY_GATES,
                prompt
            ),
            agent_provider,
        );
    }
    if command == Some("pack") {
        return apply_native_image_contract(
            format!(
            "You are the creation agent inside Sprite Studio. The user requested a coordinated asset pack. Follow the pack harness exactly. Preserve every unrelated workspace file. Do not treat the items as animation frames. The user may specify the art style in plain language; that explicit style overrides the saved preset.\n\nSELECTED CHAT CONTEXT\n{}\n\nASSET PACK HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nINTERNAL ACCEPTANCE LOOP\n{}\n\nUSER REQUEST\n{}",
            if context.is_empty() { "No predefined image context. Infer only from this request." } else { context },
            ASSET_PACK_HARNESS,
            STYLE_PRESETS,
            QUALITY_GATES,
            INTERNAL_ACCEPTANCE_LOOP,
            prompt
        ),
            agent_provider,
        );
    }
    if command == Some("rig") {
        let (width, height) = generation
            .map(|options| (options.width, options.height))
            .unwrap_or((0, 0));
        return apply_native_image_contract(
            format!(
            "You are the creation agent inside Sprite Studio. The user requested a native rig — named points, capsule bones, and pose frames — for Sprite Studio's deterministic Rust rig engine. The app captures your `rig-suggestion` JSON block, opens it in the Rig editor, and renders the animation itself; do not write rendered frames, masks, or rig-rendering scripts for this request. If no usable sprite master is attached or referenced, first follow the character harness to create exactly one clean transparent source master, save it under `assets/characters/`, and rig that exact file.\n\n{}\n\nSELECTED CHAT CONTEXT\n{}\n\nUSER REQUEST\n{}\n\nAnswer with the rig-suggestion JSON block now.",
            rig_suggestion_prompt(prompt, "biped", width, height),
            if context.is_empty() { "No saved style override." } else { context },
            prompt
        ),
            agent_provider,
        );
    }
    // Explicit user wording owns routing. For deictic requests such as
    // "animate this", only the selected/focused asset identity is a valid
    // fallback; legacy worktree labels and style prose remain irrelevant.
    let asset_identity = asset_identity_context(context);
    let routing_prompt = if !has_explicit_asset_subject(prompt) && !asset_identity.is_empty() {
        format!("{prompt}\n{asset_identity}")
    } else {
        prompt.to_string()
    };
    let mut brief = infer_brief(&routing_prompt);
    // Saved style context may supply character proportions, but it is never
    // allowed to reroute the requested asset or replace explicit user style.
    if brief.harness != HarnessKind::Tileset && inferred_style(prompt).is_none() {
        if let Some((width, height, preset, _, _)) = inferred_style(context) {
            brief.width = width;
            brief.height = height;
            brief.preset = preset;
        }
    }
    if let Some(generation) = generation {
        if brief.harness == HarnessKind::Tileset {
            (brief.width, brief.height) = match generation.quality.as_str() {
                "low" => (288, 192),
                "high" => (480, 320),
                "custom" => (generation.width, generation.height),
                _ => (384, 256),
            };
            brief.frames = 1;
            brief.fps = 1;
        } else {
            brief.width = generation.width;
            brief.height = generation.height;
            brief.frames = generation.frames;
            brief.fps = generation.fps;
        }
    }
    if let Some((width, height)) = explicit_size(prompt) {
        brief.width = width;
        brief.height = height;
    }
    if let Some(frames) = explicit_count(prompt, "frames").filter(|value| (1..=64).contains(value))
    {
        brief.frames = frames;
    }
    if let Some(fps) = explicit_count(prompt, "fps").filter(|value| (1..=60).contains(value)) {
        brief.fps = fps;
    }
    let explicit_frames = explicit_count(prompt, "frames").filter(|value| (1..=64).contains(value));
    let ai_recommends_frames = generation
        .map(|options| options.frame_mode == "auto")
        .unwrap_or(false)
        && explicit_frames.is_none()
        && command != Some("sprite")
        && brief.harness != HarnessKind::Tileset;
    let motion_plan = (!ai_recommends_frames && brief.harness != HarnessKind::Tileset)
        .then(|| generation.and_then(|options| build_motion_plan(prompt, options).ok()))
        .flatten();
    if let Some(plan) = &motion_plan {
        brief.frames = plan.selected_frame_count;
    }
    match command {
        Some("animate") if brief.harness != HarnessKind::Tileset => {
            brief.frames = brief.frames.max(2)
        }
        Some("sprite") => {
            brief.frames = 1;
            brief.fps = 1;
        }
        Some("character") => {
            brief.harness = HarnessKind::Character;
            brief.category = "characters";
        }
        Some("effect") => {
            brief.harness = HarnessKind::Effect;
            brief.category = "effects";
        }
        _ => {}
    }
    let animated = brief.frames > 1 && brief.harness != HarnessKind::Tileset;
    let routed_harness = if brief.harness == HarnessKind::Tileset {
        TERRAIN_TILESET_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Character && animated {
        format!("{CHARACTER_HARNESS}\n\n{CHARACTER_ANIMATION_HARNESS}")
    } else if brief.harness == HarnessKind::Character {
        CHARACTER_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Creature {
        CREATURE_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Effect {
        EFFECT_HARNESS.to_string()
    } else if animated {
        GAME_OBJECT_ANIMATION_HARNESS.to_string()
    } else {
        "This asset kind currently uses the non-character deterministic renderer section in the Sprite Director router.".to_string()
    };
    let motion_plan_text = if ai_recommends_frames {
        let options = generation.expect("AI recommendation requires generation options");
        format!(
            "Frame policy: visual motion recommendation\nAllowed range: {}–{} frames\nInspect the attached source master and assign a MORPHOLOGY TAG: biped, quadruped, hexapod, segmented-many-leg, serpentine, winged, amorphous, or rigid-object. Select the smallest frame count that represents every necessary mechanical phase without a discontinuity. Before building the rig, state exactly `FRAME RECOMMENDATION: N frames — <visual/mechanical reason>`. Use N consistently in the rig and manifest.",
            options.min_frames, options.max_frames
        )
    } else {
        motion_plan
        .as_ref()
        .map(|plan| {
            let phases = plan
                .phases
                .iter()
                .enumerate()
                .map(|(index, phase)| {
                    format!(
                        "{}. {} — {} frame(s): {}",
                        index + 1,
                        phase.name,
                        phase.frame_count,
                        phase.description
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "Frame policy: {}\nSelected frame count: {}\nAutomatic frame adjustment: {}\nInterpolation: {}\n{}\nPhases:\n{}",
                plan.frame_mode,
                plan.selected_frame_count,
                plan.allow_auto_adjust,
                plan.allow_interpolation,
                plan.explanation,
                phases
            )
        })
        .unwrap_or_else(|| "Frame policy: inferred legacy defaults".into())
    };
    let frame_budget_text = if ai_recommends_frames {
        let options = generation.expect("AI recommendation requires generation options");
        format!(
            "AI recommends after image inspection (allowed {}–{}; profile hint {} is not a decision)",
            options.min_frames, options.max_frames, options.frames
        )
    } else {
        brief.frames.to_string()
    };
    let rig_contract = if animated
        && brief.harness != HarnessKind::Effect
        && brief.harness != HarnessKind::Tileset
    {
        RIG_PLANNING_CONTRACT
    } else {
        "This routed job does not use the layered mask-rig planning contract."
    };
    let frame_polish_contract = if animated && command == Some("animate") {
        AI_FRAME_POLISH_CONTRACT
    } else {
        "This routed job does not use animation frame polishing."
    };
    let physical_motion_contract = if brief.frames > 1
        && brief.harness != HarnessKind::Effect
        && brief.harness != HarnessKind::Tileset
    {
        PHYSICAL_MOTION_CONTRACT
    } else {
        "This routed job does not use real-world articulated-motion scaling."
    };
    let limb_identity_contract = if animated
        && matches!(
            brief.harness,
            HarnessKind::Character | HarnessKind::Creature
        ) {
        BIPED_LOCOMOTION_IDENTITY
    } else {
        "This routed job has no paired-limb identity requirement."
    };
    apply_native_image_contract(
        format!(
        "You are the creation agent inside Sprite Studio. Obey the routed harness. Explicit subject words in the current USER REQUEST and an explicit slash command own routing. When the request says only `this`, `it`, or `selected`, the selected/focused asset filename may identify the subject; its legacy folder, worktree label, project-section name, description, reference category, and style prose must never override the subject. All router, harness, preset, quality-gate, and internal-review text you need is embedded in this prompt; do not search the workspace for `references/*.md` files. ImageGen may create one source master. Animation timing and poses come from a saved deterministic rig, never from independently invented AI frames. Render rig-only animations with Sprite Studio's native rig engine or `.sprite-studio/sprite_rig.py`. Use ImageGen on animation frames only when the user explicitly selected AI polish or experimental full redraw, and only after rough rig frames exist as pose authority. Pose sheets remain forbidden. Preserve every unrelated workspace asset: never move, delete, rename, or overwrite existing assets unless the user explicitly asked to modify that exact asset. The routed asset category is a hard contract: the rig category, output folder, generation manifest category, scanned assets, and final response must all match the routed category below. Never reuse an older rig or source because its filename or appearance is similar; provenance must trace to the exact focused reference. Before reporting success, run the silent internal acceptance loop, validate the saved rig and `.sprite-studio/last-generation.json`, preview at least three cycles, and run native quality analysis. Visual imperfections must degrade gracefully: after one repair attempt, publish the best structurally valid candidate and simplify motion when needed, ending with `GENERATION_WARNING: <concise limitation>`. An explicit animation request requires at least two distinct frames; never call a one-frame fallback an animation. Use `GENERATION_FAILED` only when no valid workspace-confined result of the requested kind can be produced at all. Never restore an old manifest as new output. Keep the user-facing reply to the result and any warning in at most three short sentences; never narrate the internal review or retry process.\n\n\
         DETERMINISTIC HARNESS BRIEF\n\
         - routed harness: {}\n\
         - asset category: {}\n\
         - write every generated frame to `assets/{}/`; the source asset's existing folder never overrides this routed category\n\
         - logical canvas: {}x{} pixels\n\
         - frame count: {}\n\
         - playback FPS: {}\n\
         - inferred preset: {}\n\
         - chat quality preset: {}\n\
         - slash command: {}\n\
         - explicit user constraints always override inferred defaults\n\n\
         {}\n\
         MOTION PHASE PLAN\n{}\n\n\
         RIG PLANNING CONTRACT\n{}\n\n\
         PAIRED-LIMB IDENTITY CONTRACT\n{}\n\n\
         REAL-WORLD PHYSICAL MOTION CONTRACT\n{}\n\n\
         AI FRAME POLISH CONTRACT\n{}\n\n\
         SELECTED USER CONTEXT\n{}\n\n\
         BUNDLED ROUTER\n{}\n\nROUTED HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nINTERNAL ACCEPTANCE LOOP\n{}\n\nUSER REQUEST\n{}",
        brief.harness.as_str(),
        brief.category,
        brief.category,
        brief.width,
        brief.height,
        frame_budget_text,
        brief.fps,
        brief.preset,
        generation.map(|value| value.quality.as_str()).unwrap_or("automatic"),
        command.unwrap_or("none"),
        anchor_contract
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim())
            .unwrap_or("CHARACTER ANCHOR CONTRACT\nNo promoted anchor exists yet. When a character master is approved, promote it with `promote_anchor` so follow-up animations inherit the same canvas and foot baseline."),
        motion_plan_text,
        rig_contract,
        limb_identity_contract,
        physical_motion_contract,
        frame_polish_contract,
        if context.is_empty() { "No saved style override." } else { context },
        DIRECTOR_SKILL,
        routed_harness,
        STYLE_PRESETS,
        QUALITY_GATES,
        INTERNAL_ACCEPTANCE_LOOP,
        prompt
    ),
        agent_provider,
    )
}

fn apply_python_runtime(prompt: String) -> String {
    match resolve_python_launcher() {
        Some(launcher) => {
            let command = launcher_command_line(&launcher);
            let injected = inject_python_commands(&prompt, &launcher);
            format!(
                "WORKSPACE PYTHON RUNTIME\nRun bundled scripts with `{command} .sprite-studio/<script>.py` (not bare `python3` on Windows).\n\n{injected}"
            )
        }
        None => format!(
            "WORKSPACE PYTHON RUNTIME\nPython 3 is not available on PATH. Install CPython from python.org or ensure `py -3` works before running `.sprite-studio/sprite_tool.py` or `sprite_rig.py`.\n\n{prompt}"
        ),
    }
}

fn apply_native_image_contract(prompt: String, agent_provider: Option<&str>) -> String {
    let merged = match agent_provider {
        Some("cursor") | Some("antigravity") => {
            let (heading, contract) = match agent_provider {
                Some("cursor") => ("CURSOR IMAGE CONTRACT", CURSOR_IMAGE_CONTRACT),
                _ => ("ANTIGRAVITY IMAGE CONTRACT", ANTIGRAVITY_IMAGE_CONTRACT),
            };
            match prompt.split_once("\n\n") {
                Some((opening, rest)) => format!("{opening}\n\n{heading}\n{contract}\n\n{rest}"),
                None => format!("{prompt}\n\n{heading}\n{contract}"),
            }
        }
        _ => prompt,
    };
    apply_python_runtime(merged)
}

/// Meta-prompt for translating and expanding a chat draft into a precise
/// English generation instruction without starting the sprite pipeline.
pub fn refine_generation_prompt(
    draft: &str,
    context: Option<&str>,
    command: Option<&str>,
    generation: Option<&GenerationOptions>,
    animation_mode: Option<&str>,
) -> String {
    let draft = draft.trim();
    let workflow = match command {
        Some("animate") => "ANIMATION — produce a looping sprite animation with readable motion, clean silhouettes, and frame-to-frame consistency.",
        Some("rig") => "RIG PLANNING — describe motion intent and anatomy cues so rig points and poses can be placed on an existing master.",
        Some("sprite") => "STATIC SPRITE — one polished, centered game sprite with a removable background.",
        Some("character") => "CHARACTER — one hero-style character master suitable for animation or rigging later.",
        Some("effect") => "GAME EFFECT — animated VFX such as magic, impacts, or particles.",
        Some("pack") => "ASSET PACK — a coordinated set of separate static sprites that share one art direction.",
        _ => "GENERAL SPRITE STUDIO REQUEST — infer whether the user wants a static sprite, animation, effect, or pack.",
    };
    let generation_block = generation
        .map(|options| {
            let frame_line = if options.frame_mode == "auto" {
                format!(
                    "Frames: auto (allowed {}–{}, target {}), {} fps",
                    options.min_frames,
                    options.max_frames,
                    options.frames,
                    options.fps
                )
            } else {
                format!(
                    "Frames: fixed {} at {} fps",
                    options.frames,
                    options.fps
                )
            };
            format!(
                "QUALITY PRESET: {}\nCanvas: {}x{} source pixels\n{}\nInterpolation allowed: {}\nAuto frame adjustment allowed: {}",
                options.quality,
                options.width,
                options.height,
                frame_line,
                options.allow_interpolation,
                options.allow_auto_adjust
            )
        })
        .unwrap_or_else(|| "Use sensible pixel-art defaults if the draft omits technical details.".to_string());
    let animation_mode_line = animation_mode
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Animation polish mode selected in chat: {value}."))
        .unwrap_or_default();
    let context_block = context
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("PROJECT CONTEXT\n{value}"))
        .unwrap_or_default();
    format!(
        "PROMPT REFINER CONTRACT\n\
You are the prompt refiner inside Sprite Studio, a Tauri app for pixel-art sprite and animation generation.\n\
Rewrite the user's draft into ONE final English instruction they can send to the generation agent.\n\n\
GOAL\n\
- Translate any language into clear, natural English.\n\
- Preserve the user's creative intent; do not invent a different subject.\n\
- Add concrete game-art details the draft omits: subject, pose or motion, palette or mood, silhouette readability, transparency/background, pixel-art scale, and loop behavior when relevant.\n\
- Align with the workflow, canvas, frame budget, and project context below.\n\
- If the draft starts with a slash command (/animate, /sprite, /character, /effect, /pack, /rig), keep that exact prefix on the first line and refine only the text after it.\n\
- Do not mention this contract, JSON, markdown fences, or your reasoning.\n\n\
OUTPUT FORMAT\n\
Return ONLY the refined prompt text — no title, bullet list, quotes, or commentary.\n\n\
WORKFLOW\n{workflow}\n{animation_mode_line}\n\n\
GENERATION SETTINGS\n{generation_block}\n\n\
{context_block}\n\n\
USER DRAFT\n{draft}"
    )
}

/// Strips common provider wrappers from a refine response.
pub fn clean_refined_prompt_response(response: &str) -> String {
    let mut text = response.trim().to_string();
    if text.starts_with("```") {
        let lines = text.lines().collect::<Vec<_>>();
        if lines.len() >= 2 {
            let end = lines
                .last()
                .filter(|line| line.trim() == "```")
                .map(|_| lines.len() - 1)
                .unwrap_or(lines.len());
            text = lines[1..end].join("\n").trim().to_string();
        }
    }
    text.trim_matches('"').trim().to_string()
}
