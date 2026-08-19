use crate::{models::GenerationOptions, motion_planner::build_motion_plan};

const DIRECTOR_SKILL: &str = include_str!("../resources/skills/sprite-director/SKILL.md");
const STYLE_PRESETS: &str =
    include_str!("../resources/skills/sprite-director/references/style-presets.md");
const QUALITY_GATES: &str =
    include_str!("../resources/skills/sprite-director/references/quality-gates.md");
const CHARACTER_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/character-harness.md");
const CHARACTER_ANIMATION_HARNESS: &str = include_str!(
    "../resources/skills/sprite-director/references/character-animation-harness.md"
);
const CREATURE_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/creature-harness.md");
const EFFECT_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/effect-harness.md");
const TERRAIN_TILESET_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/terrain-tileset-harness.md");
const GAME_OBJECT_ANIMATION_HARNESS: &str = include_str!(
    "../resources/skills/sprite-director/references/game-object-animation-harness.md"
);
const ASSET_PACK_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/asset-pack-harness.md");
const RIG_PLANNING_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/rig-planning-contract.md");
const PHYSICAL_MOTION_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/physical-motion-contract.md");
const AI_FRAME_POLISH_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/ai-frame-polish-contract.md");
const INTERNAL_ACCEPTANCE_LOOP: &str =
    include_str!("../resources/skills/sprite-director/references/internal-acceptance-loop.md");
const BIPED_LOCOMOTION_IDENTITY: &str = include_str!(
    "../resources/skills/sprite-director/references/biped-locomotion-identity.md"
);

#[derive(Debug, PartialEq)]
enum HarnessKind {
    Character,
    Creature,
    Prop,
    Terrain,
    Tileset,
    Effect,
}

impl HarnessKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Creature => "creature",
            Self::Prop => "prop",
            Self::Terrain => "terrain",
            Self::Tileset => "terrain tileset",
            Self::Effect => "effect",
        }
    }
}

#[derive(Debug, PartialEq)]
struct SpriteBrief {
    harness: HarnessKind,
    category: &'static str,
    width: u32,
    height: u32,
    frames: u32,
    fps: u32,
    preset: &'static str,
}

fn explicit_size(prompt: &str) -> Option<(u32, u32)> {
    prompt
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
        .filter_map(|token| {
            token
                .to_ascii_lowercase()
                .split_once('x')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .find_map(|(width, height)| {
            let width = width
                .trim_matches(|character: char| !character.is_ascii_digit())
                .parse()
                .ok()?;
            let height = height
                .trim_matches(|character: char| !character.is_ascii_digit())
                .parse()
                .ok()?;
            (8..=512).contains(&width).then_some(())?;
            (8..=512).contains(&height).then_some((width, height))
        })
}

fn has_word(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == expected)
}

fn explicit_count(prompt: &str, unit: &str) -> Option<u32> {
    let words: Vec<_> = prompt
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    words.windows(2).find_map(|pair| {
        (pair[1].eq_ignore_ascii_case(unit)
            || (unit == "frames" && pair[1].eq_ignore_ascii_case("frame")))
        .then(|| pair[0].parse().ok())
        .flatten()
    })
}

fn inferred_style(text: &str) -> Option<(u32, u32, &'static str, u32, u32)> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("graphic adventure")
        || lower.contains("graphic-adventure")
        || lower.contains("angular concept")
    {
        Some((192, 256, "graphic adventure", 4, 8))
    } else if lower.contains("cozy chibi")
        || lower.contains("cozy-chibi")
        || lower.contains("rounded cartoon")
    {
        Some((128, 160, "cozy chibi", 4, 8))
    } else if lower.contains("pixel rpg")
        || lower.contains("pixel-rpg")
        || lower.contains("stardew")
        || lower.contains("farming rpg")
        || lower.contains("cozy 16-bit")
    {
        Some((48, 64, "pixel RPG", 4, 8))
    } else if lower.contains("platform") || lower.contains("side-scroller") {
        Some((32, 32, "pixel platformer", 6, 12))
    } else {
        None
    }
}

fn has_explicit_asset_subject(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    [
        "monster", "creature", "centipede", "enemy", "animal", "beast", "insect", "spider",
        "slime", "rabbit", "bunny", "hare", "fox", "wolf", "bear", "cat", "dog", "bird",
        "bat", "character", "hero", "npc", "knight", "farmer", "herbalist", "tile", "tileset",
        "tilemap", "terrain", "ground", "tree", "bush", "plant", "rock", "effect", "spark",
        "smoke", "explosion", "burst", "impact", "fireball", "flame", "fire", "frost", "ice",
        "lightning", "thunder", "beam", "projectile", "spell", "magic", "slash", "prop", "item",
        "icon", "potion", "weapon", "object", "chest", "door", "machine", "vehicle", "torch",
        "turret",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
}

fn asset_identity_context(context: &str) -> String {
    context
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("Context asset:")
                || trimmed.starts_with("Selected asset:")
                || trimmed.starts_with("FOCUSED CHAT REFERENCE:")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn infer_brief(prompt: &str) -> SpriteBrief {
    let lower = prompt.to_ascii_lowercase();
    let explicitly_game_object = lower.contains("game object")
        || lower.contains("game-object")
        || has_word(&lower, "object");
    let category = if explicitly_game_object
        && ["tree", "bush", "plant", "rock", "terrain", "ground", "tile"]
            .iter()
            .any(|word| has_word(&lower, word))
    {
        "terrain"
    } else if explicitly_game_object {
        "props"
    } else if [
        "monster",
        "creature",
        "centipede",
        "enemy",
        "animal",
        "beast",
        "insect",
        "spider",
        "slime",
        "rabbit",
        "bunny",
        "hare",
        "fox",
        "wolf",
        "bear",
        "cat",
        "dog",
        "bird",
        "bat",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
    {
        "creatures"
    } else if ["character", "hero", "npc", "knight", "farmer", "herbalist"]
        .iter()
        .any(|word| has_word(&lower, word))
    {
        "characters"
    } else if [
        "tile", "tileset", "tilemap", "terrain", "ground", "tree", "bush", "plant", "rock",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
    {
        "terrain"
    } else if [
        "effect",
        "spark",
        "smoke",
        "explosion",
        "burst",
        "impact",
        "fireball",
        "flame",
        "fire",
        "frost",
        "ice",
        "lightning",
        "thunder",
        "beam",
        "projectile",
        "spell",
        "magic",
        "slash",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
    {
        "effects"
    } else if [
        "prop", "item", "icon", "potion", "weapon", "object", "chest", "door", "machine",
        "vehicle", "torch", "turret",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
    {
        "props"
    } else {
        "characters"
    };

    let harness = match category {
        "terrain"
            if ["tile", "tileset", "tilemap", "terrain", "ground"]
                .iter()
                .any(|word| has_word(&lower, word))
                && !["tree", "bush", "plant", "rock"]
                    .iter()
                    .any(|word| has_word(&lower, word)) =>
        {
            HarnessKind::Tileset
        }
        "terrain" => HarnessKind::Terrain,
        "effects" => HarnessKind::Effect,
        "props" => HarnessKind::Prop,
        "creatures" => HarnessKind::Creature,
        _ => HarnessKind::Character,
    };

    let (mut width, mut height, preset, mut frames, mut fps) =
        if let Some(style) = inferred_style(prompt) {
            style
        } else if harness == HarnessKind::Tileset {
            (384, 256, "terrain tileset atlas", 1, 1)
        } else if category == "terrain" {
            (128, 128, "terrain game object", 1, 1)
        } else if category == "effects" {
            // Effects need enough pixel area for a readable core, halo and
            // particles. At 32px, ImageGen masters collapse into a tiny blob when
            // centered on a gameplay canvas (the failure mode visible in the old
            // fireball experiments).
            (128, 128, "animated effect", 8, 12)
        } else if category == "props" {
            (128, 128, "inventory prop", 1, 1)
        } else if category == "creatures" {
            (128, 128, "game creature", 6, 10)
        } else {
            (128, 128, "general ImageGen character", 4, 8)
        };

    if category == "characters" && (lower.contains("walk") || lower.contains("walking")) {
        frames = 8;
        fps = 10;
    } else if category == "characters" && (lower.contains("run") || lower.contains("running")) {
        frames = 8;
        fps = 12;
    }

    if lower.contains("portrait") || lower.contains("bust") {
        width = 128;
        height = 128;
        frames = 1;
        fps = 1;
    }
    if lower.contains("single frame") || lower.contains("one frame") || lower.contains("static") {
        frames = 1;
        fps = 1;
    }
    if let Some(size) = explicit_size(prompt) {
        (width, height) = size;
    }

    SpriteBrief {
        harness,
        category,
        width,
        height,
        frames,
        fps,
        preset,
    }
}

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
) -> String {
    let context = context.unwrap_or("").trim();
    if command == Some("pack") {
        return format!(
            "You are the creation agent inside Sprite Studio. The user requested a coordinated asset pack. Follow the pack harness exactly. Preserve every unrelated workspace file. Do not treat the items as animation frames. The user may specify the art style in plain language; that explicit style overrides the saved preset.\n\nSELECTED CHAT CONTEXT\n{}\n\nASSET PACK HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nINTERNAL ACCEPTANCE LOOP\n{}\n\nUSER REQUEST\n{}",
            if context.is_empty() { "No predefined image context. Infer only from this request." } else { context },
            ASSET_PACK_HARNESS,
            STYLE_PRESETS,
            QUALITY_GATES,
            INTERNAL_ACCEPTANCE_LOOP,
            prompt
        );
    }
    if command == Some("rig") {
        let (width, height) = generation
            .map(|options| (options.width, options.height))
            .unwrap_or((0, 0));
        return format!(
            "You are the creation agent inside Sprite Studio. The user requested a native rig — named points, capsule bones, and pose frames — for Sprite Studio's deterministic Rust rig engine. The app captures your `rig-suggestion` JSON block, opens it in the Rig editor, and renders the animation itself; do not write rendered frames, masks, or rig-rendering scripts for this request. If no usable sprite master is attached or referenced, first follow the character harness to create exactly one clean transparent source master, save it under `assets/characters/`, and rig that exact file.\n\n{}\n\nSELECTED CHAT CONTEXT\n{}\n\nUSER REQUEST\n{}\n\nAnswer with the rig-suggestion JSON block now.",
            rig_suggestion_prompt(prompt, "biped", width, height),
            if context.is_empty() { "No saved style override." } else { context },
            prompt
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
        && matches!(brief.harness, HarnessKind::Character | HarnessKind::Creature)
    {
        BIPED_LOCOMOTION_IDENTITY
    } else {
        "This routed job has no paired-limb identity requirement."
    };
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
    )
}

#[cfg(test)]
mod tests {
    use super::{explicit_size, infer_brief, studio_prompt, HarnessKind, SpriteBrief};
    use crate::models::GenerationOptions;

    #[test]
    fn infers_a_cozy_farming_character_from_a_simple_prompt() {
        assert_eq!(
            infer_brief("make me a character similar to Stardew Valley"),
            SpriteBrief {
                harness: HarnessKind::Character,
                category: "characters",
                width: 48,
                height: 64,
                frames: 4,
                fps: 8,
                preset: "pixel RPG",
            }
        );
    }

    #[test]
    fn paired_limb_identity_lock_targets_animated_characters_and_creatures_only() {
        let generation = GenerationOptions {
            quality: "mid".into(),
            width: 64,
            height: 64,
            frames: 8,
            fps: 12,
            frame_mode: "fixed".into(),
            min_frames: 8,
            max_frames: 12,
            allow_interpolation: false,
            allow_auto_adjust: true,
        };
        let character =
            studio_prompt("character run cycle", None, Some(&generation), Some("animate"));
        assert!(
            character.contains("PAIRED-LIMB IDENTITY CONTRACT"),
            "animated characters must carry the limb identity lock"
        );
        assert!(character.contains("NEAR"));
        assert!(character.contains("FAR"));
        assert!(
            character.contains("Near contact") && character.contains("Far contact"),
            "the run phase plan must use NEAR/FAR leg phases"
        );
        let effect = studio_prompt("explosion effect", None, Some(&generation), Some("animate"));
        assert_eq!(
            effect.matches("This routed job has no paired-limb identity requirement.")
                .count(),
            1,
            "effects must skip the limb identity lock"
        );
        let single = studio_prompt("character portrait", None, Some(&generation), Some("sprite"));
        assert!(
            single.contains("This routed job has no paired-limb identity requirement."),
            "static sprites must skip the limb identity lock"
        );
    }

    #[test]
    fn explicit_dimensions_override_the_preset() {
        assert_eq!(explicit_size("a 48x64 knight"), Some((48, 64)));
        let brief = infer_brief("make a Stardew-like 48x64 knight");
        assert_eq!((brief.width, brief.height), (48, 64));
    }

    #[test]
    fn routes_terrain_tilesets_to_one_large_atlas() {
        assert_eq!(
            infer_brief("make a grassy terrain tilemap"),
            SpriteBrief {
                harness: HarnessKind::Tileset,
                category: "terrain",
                width: 384,
                height: 256,
                frames: 1,
                fps: 1,
                preset: "terrain tileset atlas",
            }
        );

        let generation = GenerationOptions {
            quality: "mid".into(),
            width: 64,
            height: 64,
            frames: 6,
            fps: 8,
            frame_mode: "auto".into(),
            min_frames: 4,
            max_frames: 32,
            allow_interpolation: false,
            allow_auto_adjust: true,
        };
        let prompt = studio_prompt(
            "make a grassy terrain tilemap like the attached reference",
            Some("ACTIVE REFERENCE IMAGES (ATTACHED AS REAL IMAGE INPUTS)\n- Tilemap_color1.png"),
            Some(&generation),
            None,
        );
        assert!(prompt.contains("routed harness: terrain tileset"));
        assert!(prompt.contains("logical canvas: 384x256 pixels"));
        assert!(prompt.contains("frame count: 1"));
        assert!(prompt.contains("exactly one final PNG"));
        assert!(prompt.contains("one-element `files` array"));
        assert!(!prompt.contains("AI FRAME RECOMMENDATION"));
    }

    #[test]
    fn user_request_overrides_legacy_worktree_type_context() {
        let prompt = studio_prompt(
            "make a desert terrain tileset",
            Some("Active worktree: Old heroes (character). Selected asset: assets/characters/old_hero.png"),
            None,
            None,
        );

        assert!(prompt.contains("routed harness: terrain tileset"));
        assert!(prompt.contains("asset category: terrain"));
        assert!(prompt.contains("logical canvas: 384x256 pixels"));
        assert!(prompt.contains("write every generated frame to `assets/terrain/`"));
        assert!(prompt.contains("legacy folder, worktree label"));
    }

    #[test]
    fn animate_this_uses_selected_asset_identity_without_trusting_its_folder() {
        let prompt = studio_prompt(
            "animate this hopping forward",
            Some("Active project section: Old heroes.\nContext asset: assets/characters/woodland-rabbit-retry_01.png\nSelected art direction: Pixel RPG."),
            None,
            Some("animate"),
        );

        assert!(prompt.contains("routed harness: creature"));
        assert!(prompt.contains("asset category: creatures"));
        assert!(prompt.contains("write every generated frame to `assets/creatures/`"));
    }

    #[test]
    fn explicit_user_style_overrides_saved_style_context() {
        let prompt = studio_prompt(
            "make a pixel RPG character, single frame",
            Some("Selected style preset: Cozy chibi. rounded cartoon"),
            None,
            None,
        );

        assert!(prompt.contains("logical canvas: 48x64 pixels"));
        assert!(prompt.contains("inferred preset: pixel RPG"));
        assert!(prompt.contains("frame count: 1"));
    }

    #[test]
    fn terrain_objects_do_not_become_tileset_atlases() {
        let brief = infer_brief("make a windswept tree game object");
        assert_eq!(brief.harness, HarnessKind::Terrain);
        assert_eq!((brief.width, brief.height, brief.frames), (128, 128, 1));
    }

    #[test]
    fn prompt_embeds_renderer_and_originality_rules() {
        let prompt = studio_prompt("make a potion icon", None, None, None);
        assert!(prompt.contains("python3 .sprite-studio/sprite_tool.py"));
        assert!(prompt.contains("original design"));
        assert!(prompt.ends_with("make a potion icon"));
    }

    #[test]
    fn routes_characters_to_imagegen_and_applies_saved_style() {
        let prompt = studio_prompt(
            "make me a character, single frame",
            Some("Selected style preset: Cozy chibi. rounded cartoon"),
            None,
            None,
        );
        assert!(prompt.contains("routed harness: character"));
        assert!(prompt.contains("image_gen__imagegen"));
        assert!(prompt.contains("logical canvas: 128x160 pixels"));
        assert!(!prompt.contains("# Deterministic character rig harness"));
    }

    #[test]
    fn routes_effects_through_the_effect_harness() {
        let prompt = studio_prompt(
            "/effect a bright arcane impact with transparent background",
            Some("ACTIVE REFERENCE IMAGES\n- palette [vfx]: /tmp/palette.png"),
            None,
            Some("effect"),
        );
        assert!(prompt.contains("routed harness: effect"));
        assert!(prompt.contains("# ImageGen visual-effects harness"));
        assert!(prompt.contains("Create one high-quality master"));
        assert!(prompt.contains("assets/effects/"));
    }

    #[test]
    fn routes_elemental_attacks_to_the_effect_harness() {
        let prompt = studio_prompt(
            "make an ice fireball end burst with a transparent background",
            None,
            None,
            None,
        );
        assert!(prompt.contains("routed harness: effect"));
        assert!(prompt.contains("# ImageGen visual-effects harness"));
        assert!(prompt.contains("logical canvas: 128x128 pixels"));
    }

    #[test]
    fn requires_visual_inspection_and_identity_lock_for_an_attached_master() {
        let prompt = studio_prompt(
            "animate this creature",
            Some("ACTIVE REFERENCE IMAGES (ATTACHED AS REAL IMAGE INPUTS)\n- master: /tmp/master.png"),
            None,
            Some("animate"),
        );
        assert!(prompt.contains("source master"));
        assert!(prompt.contains("provenance must trace to the exact focused reference"));
        assert!(prompt.contains("saved deterministic rig"));
        assert!(!prompt.contains("independently invented AI poses"));
    }

    #[test]
    fn routes_an_explicit_single_frame_herbalist_as_a_character() {
        let prompt = studio_prompt(
            "make one original cozy chibi herbalist character, single frame",
            Some("Selected style preset: Cozy chibi. polished cozy chibi game character, rounded proportions, oversized expressive head, clean dark outline and simple readable shapes."),
            None,
            None,
        );
        assert!(prompt.contains("routed harness: character"));
        assert!(prompt.contains("asset category: characters"));
        assert!(prompt.contains("frame count: 1"));
        assert!(prompt.contains("Preserve every unrelated workspace asset"));
    }

    #[test]
    fn does_not_treat_proportions_as_the_prop_keyword() {
        assert_eq!(
            infer_brief("a rounded character with cozy proportions").harness,
            HarnessKind::Character
        );
    }

    #[test]
    fn routes_segmented_monsters_to_the_creature_rig_harness() {
        let brief = infer_brief("a cave centipede monster");
        assert_eq!(brief.harness, HarnessKind::Creature);
        assert_eq!(brief.category, "creatures");
        assert_eq!(
            (brief.width, brief.height, brief.frames, brief.fps),
            (128, 128, 6, 10)
        );

        let prompt = studio_prompt(
            "/animate a cave centipede monster crawling",
            None,
            None,
            Some("animate"),
        );
        assert!(prompt.contains("routed harness: creature"));
        assert!(prompt.contains("RIG PLANNING CONTRACT"));
        assert!(prompt.contains("saved deterministic rig"));
        assert!(prompt.contains("assets/creatures/"));
        assert!(prompt.contains("metachronal support wave"));
    }

    #[test]
    fn chat_profile_and_animate_command_override_inferred_defaults() {
        let generation = GenerationOptions {
            quality: "high".into(),
            width: 128,
            height: 128,
            frames: 8,
            fps: 12,
            frame_mode: "fixed".into(),
            min_frames: 4,
            max_frames: 12,
            allow_interpolation: false,
            allow_auto_adjust: false,
        };
        let prompt = studio_prompt(
            "/animate a hunter walking",
            None,
            Some(&generation),
            Some("animate"),
        );
        assert!(prompt.contains("logical canvas: 128x128 pixels"));
        assert!(prompt.contains("frame count: 8"));
        assert!(prompt.contains("playback FPS: 12"));
        assert!(prompt.contains("chat quality preset: high"));
        assert!(prompt.contains("slash command: animate"));
        assert!(prompt.contains("# AI frame-polish contract"));
        assert!(prompt.contains("Rig only (default)"));
        assert!(prompt.contains("deterministic rig"));
        assert!(prompt.contains("sprite_rig.py"));
        assert!(prompt.contains("The routed asset category is a hard contract"));
        assert!(prompt.contains("exact focused reference"));
    }

    #[test]
    fn auto_frames_choose_the_smallest_mechanically_complete_rig() {
        let generation = GenerationOptions {
            quality: "high".into(),
            width: 64,
            height: 64,
            frames: 8,
            fps: 10,
            frame_mode: "auto".into(),
            min_frames: 4,
            max_frames: 12,
            allow_interpolation: false,
            allow_auto_adjust: true,
        };
        let prompt = studio_prompt(
            "/animate make this uploaded creature walk",
            Some("ACTIVE REFERENCE IMAGES (ATTACHED AS REAL IMAGE INPUTS)\n- creature.webp"),
            Some(&generation),
            Some("animate"),
        );
        assert!(prompt.contains("Frame policy: visual motion recommendation"));
        assert!(prompt.contains("Allowed range: 4–12 frames"));
        assert!(prompt.contains("MORPHOLOGY TAG"));
        assert!(prompt.contains("FRAME RECOMMENDATION: N frames"));
        assert!(prompt.contains("smallest frame count"));
        assert!(!prompt.contains("Selected frame count: 8"));
    }

    #[test]
    fn animated_game_objects_use_the_deterministic_rig_harness() {
        let generation = GenerationOptions {
            quality: "custom".into(),
            width: 64,
            height: 64,
            frames: 6,
            fps: 12,
            frame_mode: "fixed".into(),
            min_frames: 4,
            max_frames: 12,
            allow_interpolation: false,
            allow_auto_adjust: false,
        };
        let prompt = studio_prompt(
            "/animate a treasure chest opening",
            None,
            Some(&generation),
            Some("animate"),
        );
        assert!(prompt.contains("routed harness: prop"));
        assert!(prompt.contains("Deterministic game-object rig harness"));
        assert!(prompt.contains("Rig only (default)"));
        assert!(!prompt.contains("one high-quality AI frame per image call"));
    }

    #[test]
    fn explicit_game_object_intent_overrides_a_misfiled_character_source() {
        let prompt = studio_prompt(
            "/animate use this tree as the exact game-object master",
            Some("Selected asset: assets/characters/windy_tree_01.png"),
            None,
            Some("animate"),
        );
        assert!(prompt.contains("routed harness: terrain"));
        assert!(prompt.contains("asset category: terrain"));
        assert!(prompt.contains("write every generated frame to `assets/terrain/`"));
        assert!(prompt.contains("source asset's existing folder never overrides"));
    }

    #[test]
    fn pack_command_creates_static_coordinated_assets_and_manifest() {
        let prompt = studio_prompt(
            "/pack six forest animals in one-bit style",
            Some("Selected style preset: Pixel RPG"),
            None,
            Some("pack"),
        );
        assert!(prompt.contains("ASSET PACK HARNESS"));
        assert!(prompt.contains("do not cap it at 12"));
        assert!(prompt.contains("exactly the requested total"));
        assert!(prompt.contains("do not print raw folder links or individual asset links"));
        assert!(prompt.contains("Do not turn pack items into animation frames"));
        assert!(prompt.contains(".sprite-studio/packs/<pack-id>.json"));
        assert!(prompt.contains("explicit style overrides the saved preset"));
    }

    #[test]
    fn animation_harness_requires_a_reproducible_rig_and_optional_polish() {
        let prompt = studio_prompt(
            "/animate this rabbit",
            Some("Context asset: assets/creatures/rabbit.png"),
            None,
            Some("animate"),
        );
        assert!(prompt.contains("saved deterministic rig"));
        assert!(prompt.contains("Rig only (default)"));
        assert!(prompt.contains("rough rig frame as pose-canonical"));
        assert!(prompt.contains("REAL-WORLD PHYSICAL MOTION CONTRACT"));
        assert!(prompt.contains("PHYSICAL ENVELOPE: scale <meters>; speed <m/s>"));
        assert!(prompt.contains("pixels_per_meter = observed_subject_pixel_height_or_length"));
        assert!(prompt.contains(
            "User-stated physical quantities and explicit stylization override estimates"
        ));
        assert!(prompt.contains("speed × cycle duration"));
        assert!(prompt.contains("final-to-first"));
        assert!(prompt.contains("limb count"));
        assert!(prompt.contains("dirty alpha"));
        assert!(prompt.contains("preview at least three cycles"));
        assert!(prompt.contains("sprite_rig.py"));
        assert!(!prompt.contains("one high-quality AI frame per image call"));
    }

    #[test]
    fn every_generation_embeds_one_silent_visual_retry() {
        let prompt = studio_prompt(
            "/animate a small dragon hovering",
            None,
            None,
            Some("animate"),
        );
        assert!(prompt.contains("# Internal visual acceptance loop"));
        assert!(prompt.contains("render exactly one replacement attempt"));
        assert!(prompt.contains("wing root continuously anchored"));
        assert!(prompt.contains("do not expose the review transcript"));
    }
}
