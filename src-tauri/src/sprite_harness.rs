use crate::{models::GenerationOptions, motion_planner::build_motion_plan};

const DIRECTOR_SKILL: &str = include_str!("../resources/skills/sprite-director/SKILL.md");
const STYLE_PRESETS: &str =
    include_str!("../resources/skills/sprite-director/references/style-presets.md");
const QUALITY_GATES: &str =
    include_str!("../resources/skills/sprite-director/references/quality-gates.md");
const CHARACTER_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/character-harness.md");
const CHARACTER_ANIMATION_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/character-animation-harness.md");
const CREATURE_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/creature-harness.md");
const EFFECT_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/effect-harness.md");
const TERRAIN_TILESET_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/terrain-tileset-harness.md");
const GAME_OBJECT_ANIMATION_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/game-object-animation-harness.md");
const ASSET_PACK_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/asset-pack-harness.md");
const RIG_PLANNING_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/rig-planning-contract.md");
const PHYSICAL_MOTION_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/physical-motion-contract.md");
const AI_FRAME_POLISH_CONTRACT: &str =
    include_str!("../resources/skills/sprite-director/references/ai-frame-polish-contract.md");

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
    } else if ["effect", "spark", "smoke", "explosion"]
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

    let (mut width, mut height, preset, mut frames, mut fps) = if lower
        .contains("graphic adventure")
        || lower.contains("graphic-adventure")
        || lower.contains("angular concept")
    {
        (192, 256, "graphic adventure", 4, 8)
    } else if lower.contains("cozy chibi")
        || lower.contains("cozy-chibi")
        || lower.contains("rounded cartoon")
    {
        (128, 160, "cozy chibi", 4, 8)
    } else if lower.contains("pixel rpg")
        || lower.contains("pixel-rpg")
        || lower.contains("stardew")
        || lower.contains("farming rpg")
        || lower.contains("cozy 16-bit")
    {
        (48, 64, "pixel RPG", 4, 8)
    } else if lower.contains("platform") || lower.contains("side-scroller") {
        (32, 32, "pixel platformer", 6, 12)
    } else if harness == HarnessKind::Tileset {
        (384, 256, "terrain tileset atlas", 1, 1)
    } else if category == "terrain" {
        (64, 64, "terrain game object", 1, 1)
    } else if category == "effects" {
        (32, 32, "animated effect", 5, 12)
    } else if category == "props" {
        (24, 24, "inventory prop", 1, 1)
    } else if category == "creatures" {
        (64, 64, "game creature", 6, 10)
    } else {
        (64, 96, "general ImageGen character", 4, 8)
    };

    if category == "characters" && (lower.contains("walk") || lower.contains("walking")) {
        frames = 8;
        fps = 10;
    } else if category == "characters" && (lower.contains("run") || lower.contains("running")) {
        frames = 8;
        fps = 12;
    }

    if lower.contains("portrait") || lower.contains("bust") {
        width = 64;
        height = 64;
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

pub fn studio_prompt(
    prompt: &str,
    context: Option<&str>,
    generation: Option<&GenerationOptions>,
    command: Option<&str>,
) -> String {
    let context = context.unwrap_or("").trim();
    if command == Some("pack") {
        return format!(
            "You are the creation agent inside Sprite Studio. The user requested a coordinated asset pack. Follow the pack harness exactly. Preserve every unrelated workspace file. Do not treat the items as animation frames. The user may specify the art style in plain language; that explicit style overrides the saved preset.\n\nSELECTED CHAT CONTEXT\n{}\n\nASSET PACK HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nUSER REQUEST\n{}",
            if context.is_empty() { "No predefined image context. Infer only from this request." } else { context },
            ASSET_PACK_HARNESS,
            STYLE_PRESETS,
            QUALITY_GATES,
            prompt
        );
    }
    let inference = if context.is_empty() {
        prompt.to_string()
    } else {
        format!("{prompt}\n{context}")
    };
    let mut brief = infer_brief(&inference);
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
    if let Some(frames) = explicit_count(prompt, "frames").filter(|value| (1..=32).contains(value))
    {
        brief.frames = frames;
    }
    if let Some(fps) = explicit_count(prompt, "fps").filter(|value| (1..=60).contains(value)) {
        brief.fps = fps;
    }
    let explicit_frames = explicit_count(prompt, "frames").filter(|value| (1..=32).contains(value));
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
    let routed_harness = if brief.harness == HarnessKind::Tileset {
        TERRAIN_TILESET_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Character && brief.frames > 1 {
        format!("{CHARACTER_HARNESS}\n\n{CHARACTER_ANIMATION_HARNESS}")
    } else if brief.harness == HarnessKind::Character {
        CHARACTER_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Creature {
        CREATURE_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Effect {
        EFFECT_HARNESS.to_string()
    } else if brief.frames > 1 {
        GAME_OBJECT_ANIMATION_HARNESS.to_string()
    } else {
        "This asset kind currently uses the non-character deterministic renderer section in the Sprite Director router.".to_string()
    };
    let motion_plan_text = if ai_recommends_frames {
        let options = generation.expect("AI recommendation requires generation options");
        format!(
            "Frame policy: AI visual recommendation\nAllowed range: {}–{} frames\nDo not select a frame count from the words 'walk' or 'cycle' alone. First inspect the attached image and assign a MORPHOLOGY TAG: biped, quadruped, hexapod, segmented-many-leg, serpentine, winged, amorphous, or rigid-object. Then recommend the smallest frame count that can show one mechanically complete loop for that observed anatomy within the allowed range. Consider independent limb groups, contact states, body/segment lag, secondary motion, canvas scale, and whether distinct poses remain readable at 1×. Before creating a rig, state exactly `AI FRAME RECOMMENDATION: N frames — <visual/mechanical reason>`. The rig and manifest must use N frames. Never silently fall back to eight frames.",
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
    let rig_contract = if brief.frames > 1
        && brief.harness != HarnessKind::Effect
        && brief.harness != HarnessKind::Tileset
    {
        RIG_PLANNING_CONTRACT
    } else {
        "This routed job does not use the layered mask-rig planning contract."
    };
    let frame_polish_contract = if brief.frames > 1 && command == Some("animate") {
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
    format!(
        "You are the creation agent inside Sprite Studio. Obey the routed harness. All router, harness, preset, and quality-gate text you need is embedded in this prompt; do not search the workspace for `references/*.md` files. ImageGen may create one new character, creature, illustrated game-object, terrain-tileset atlas, or visual-effect master. If a context asset is supplied, use it as the source master without calling ImageGen unless the routed terrain/effect harness or explicit motion-readiness revision allows a new master guided by references. Never ask ImageGen to invent animation timing or poses. Rig-only animations come entirely from `.sprite-studio/sprite_rig.py`; explicit AI-polish/full-redraw modes may edit already-rendered rough frames only under the embedded frame-polish contract, and raw ImageGen output must pass `.sprite-studio/sprite_polish.py` before entering `assets/`. Pose sheets remain forbidden. In user-facing text call it the focused reference or source master, never a locked image. Never replace master art with primitive JSON drawing commands. Preserve every unrelated workspace asset: never move, delete, rename, or overwrite existing assets unless the user explicitly asked to modify that exact asset. The routed asset category is a hard contract: the rig `category`, output folder, generation manifest `category`, scanned assets, and final response must all use exactly the routed category shown below. Never reuse an older rig or source merely because its filename or appearance is similar; when a focused reference is supplied, provenance must trace to that exact reference. Before reporting success, open the saved rig and `.sprite-studio/last-generation.json` and repair any category or provenance mismatch.\n\n\
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
         REAL-WORLD PHYSICAL MOTION CONTRACT\n{}\n\n\
         AI FRAME POLISH CONTRACT\n{}\n\n\
         SELECTED USER CONTEXT\n{}\n\n\
         BUNDLED ROUTER\n{}\n\nROUTED HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nUSER REQUEST\n{}",
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
        physical_motion_contract,
        frame_polish_contract,
        if context.is_empty() { "No saved style override." } else { context },
        DIRECTOR_SKILL,
        routed_harness,
        STYLE_PRESETS,
        QUALITY_GATES,
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
    fn terrain_objects_do_not_become_tileset_atlases() {
        let brief = infer_brief("make a windswept tree game object");
        assert_eq!(brief.harness, HarnessKind::Terrain);
        assert_eq!((brief.width, brief.height, brief.frames), (64, 64, 1));
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
    fn routes_effects_to_one_imagegen_master() {
        let prompt = studio_prompt(
            "/effect a bright arcane impact with transparent background",
            Some("ACTIVE REFERENCE IMAGES\n- palette [vfx]: /tmp/palette.png"),
            None,
            Some("effect"),
        );
        assert!(prompt.contains("routed harness: effect"));
        assert!(prompt.contains("# ImageGen visual-effects harness"));
        assert!(prompt.contains("image_gen__imagegen"));
        assert!(prompt.contains("referenced_image_paths"));
        assert!(prompt.contains("assets/effects/"));
    }

    #[test]
    fn requires_visual_inspection_before_rigging_an_attached_master() {
        let prompt = studio_prompt(
            "animate this creature",
            Some("ACTIVE REFERENCE IMAGES (ATTACHED AS REAL IMAGE INPUTS)\n- master: /tmp/master.png"),
            None,
            Some("animate"),
        );
        assert!(prompt.contains("Required visual inspection"));
        assert!(prompt
            .contains("Do not derive the subject, anatomy, masks, or pivots from its filename"));
        assert!(prompt.contains("occupied pixel bounds"));
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
    fn routes_segmented_monsters_to_the_creature_harness() {
        let brief = infer_brief("a cave centipede monster");
        assert_eq!(brief.harness, HarnessKind::Creature);
        assert_eq!(brief.category, "creatures");
        assert_eq!(
            (brief.width, brief.height, brief.frames, brief.fps),
            (64, 64, 6, 10)
        );

        let prompt = studio_prompt(
            "/animate a cave centipede monster crawling",
            None,
            None,
            Some("animate"),
        );
        assert!(prompt.contains("routed harness: creature"));
        assert!(prompt.contains("Segmented creature harness"));
        assert!(prompt.contains("assets/creatures/"));
        assert!(prompt.contains("phase-shifted leg wave"));
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
        assert!(prompt.contains("python3 .sprite-studio/sprite_rig.py"));
        assert!(prompt.contains("AI rig planning contract"));
        assert!(prompt.contains("sprite_rig.py --validate"));
        assert!(prompt.contains("SHA-256 hash of the locked master"));
        assert!(prompt.contains("Automatic AI repair loop"));
        assert!(prompt.contains("at most three repair passes"));
        assert!(prompt.contains("8-frame walk: left contact, down, passing, up"));
        assert!(
            prompt.contains("ImageGen pose sheets and independently generated frames are rejected")
        );
        assert!(prompt.contains(
            "If a context asset is supplied, use it as the source master without calling ImageGen"
        ));
        assert!(prompt.contains("The routed asset category is a hard contract"));
        assert!(prompt.contains("never a locked image"));
    }

    #[test]
    fn auto_frames_are_recommended_by_ai_after_morphology_inspection() {
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
        assert!(prompt.contains("Frame policy: AI visual recommendation"));
        assert!(prompt.contains("Allowed range: 4–12 frames"));
        assert!(prompt.contains("MORPHOLOGY TAG"));
        assert!(prompt.contains("AI FRAME RECOMMENDATION: N frames"));
        assert!(prompt.contains("Never silently fall back to eight frames"));
        assert!(!prompt.contains("Selected frame count: 8"));
    }

    #[test]
    fn animated_game_objects_use_the_layered_rig_renderer() {
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
        assert!(prompt.contains("chest: base, lid, latch"));
        assert!(prompt.contains("sprite_rig.py"));
        assert!(prompt.contains("hash every rendered PNG"));
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
    fn animation_harness_requires_an_anatomy_appropriate_motion_intent() {
        let prompt = studio_prompt(
            "/animate this rabbit",
            Some("Context asset: assets/creatures/rabbit.png"),
            None,
            Some("animate"),
        );
        assert!(prompt.contains("Stage 0 — confirm the movement"));
        assert!(prompt.contains("How should this <observed subject> move?"));
        assert!(prompt.contains("rabbit can hop, bound, pounce"));
        assert!(prompt.contains("crouch/compression, hind-leg extension"));
        assert!(prompt.contains("generic whole-image bobbing"));
        assert!(prompt.contains("Stage 0.5 — plan the loop closure"));
        assert!(prompt.contains("REAL-WORLD PHYSICAL MOTION CONTRACT"));
        assert!(prompt.contains("PHYSICAL ENVELOPE: scale <meters>; speed <m/s>"));
        assert!(prompt.contains("pixels_per_meter = observed_subject_pixel_height_or_length"));
        assert!(prompt.contains("User-stated physical quantities and explicit stylization override estimates"));
        assert!(prompt.contains("speed × cycle duration"));
        assert!(prompt.contains("LOOP INTENT: seamless"));
        assert!(prompt.contains("Do **not** copy it into the final frame"));
        assert!(prompt.contains("rabbit hop returns from landing compression"));
        assert!(prompt.contains("final-to-first transition"));
        assert!(prompt.contains("MOTION READY: yes|no"));
        assert!(prompt.contains("A rabbit hop may not be implemented by translating"));
        assert!(prompt.contains("hindlimb/haunch drive"));
        assert!(prompt.contains("explicitly authorize one automatic motion-ready master revision"));
        assert!(prompt.contains("continue through rigging and polishing in the same request"));
        assert!(prompt.contains("Do not stop to ask permission"));
        assert!(prompt.contains("AI frame-polish contract"));
        assert!(prompt.contains("Polish mode: AI polish."));
        assert!(prompt.contains("sprite_polish.py"));
        assert!(prompt.contains("--region <x,y,width,height>"));
        assert!(prompt.contains("up to three total attempts"));
        assert!(prompt.contains("outsideRegionUnchanged"));
        assert!(prompt.contains("Never ask ImageGen to invent animation timing or poses"));
    }
}
