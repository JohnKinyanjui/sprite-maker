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
const GAME_OBJECT_ANIMATION_HARNESS: &str =
    include_str!("../resources/skills/sprite-director/references/game-object-animation-harness.md");

#[derive(Debug, PartialEq)]
enum HarnessKind {
    Character,
    Creature,
    Prop,
    Terrain,
    Effect,
}

impl HarnessKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Creature => "creature",
            Self::Prop => "prop",
            Self::Terrain => "terrain",
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
            (8..=256).contains(&width).then_some(())?;
            (8..=256).contains(&height).then_some((width, height))
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
    } else if ["tile", "terrain", "ground", "tree", "bush", "plant", "rock"]
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
    } else if category == "terrain" {
        (16, 16, "terrain tile", 1, 1)
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
    let inference = if context.is_empty() {
        prompt.to_string()
    } else {
        format!("{prompt}\n{context}")
    };
    let mut brief = infer_brief(&inference);
    if let Some(generation) = generation {
        brief.width = generation.width;
        brief.height = generation.height;
        brief.frames = generation.frames;
        brief.fps = generation.fps;
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
    let motion_plan = generation.and_then(|options| build_motion_plan(prompt, options).ok());
    if let Some(plan) = &motion_plan {
        brief.frames = plan.selected_frame_count;
    }
    match command {
        Some("animate") => brief.frames = brief.frames.max(2),
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
    let routed_harness = if brief.harness == HarnessKind::Character && brief.frames > 1 {
        format!("{CHARACTER_HARNESS}\n\n{CHARACTER_ANIMATION_HARNESS}")
    } else if brief.harness == HarnessKind::Character {
        CHARACTER_HARNESS.to_string()
    } else if brief.harness == HarnessKind::Creature {
        CREATURE_HARNESS.to_string()
    } else if brief.frames > 1 {
        GAME_OBJECT_ANIMATION_HARNESS.to_string()
    } else {
        "This asset kind currently uses the non-character deterministic renderer section in the Sprite Director router.".to_string()
    };
    let motion_plan_text = motion_plan
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
        .unwrap_or_else(|| "Frame policy: inferred legacy defaults".into());
    format!(
        "You are the creation agent inside Sprite Studio. Obey the routed harness. All router, harness, preset, and quality-gate text you need is embedded in this prompt; do not search the workspace for `references/*.md` files. ImageGen may create one new character, creature, or illustrated game-object master only. If a context asset is supplied, use it as the master without calling ImageGen. Animation frames and pose sheets must never come from ImageGen: animate the locked master with `.sprite-studio/sprite_rig.py`. Never replace master art with primitive JSON drawing commands. Preserve every unrelated workspace asset: never move, delete, rename, or overwrite existing assets unless the user explicitly asked to modify that exact asset.\n\n\
         DETERMINISTIC HARNESS BRIEF\n\
         - routed harness: {}\n\
         - asset category: {}\n\
         - write every generated frame to `assets/{}/`; the source asset's existing folder never overrides this routed category\n\
         - logical canvas: {}x{} pixels\n\
         - default frames: {}\n\
         - playback FPS: {}\n\
         - inferred preset: {}\n\
         - chat quality preset: {}\n\
         - slash command: {}\n\
         - explicit user constraints always override inferred defaults\n\n\
         MOTION PHASE PLAN\n{}\n\n\
         SELECTED USER CONTEXT\n{}\n\n\
         BUNDLED ROUTER\n{}\n\nROUTED HARNESS\n{}\n\nSTYLE PRESETS\n{}\n\nQUALITY GATES\n{}\n\nUSER REQUEST\n{}",
        brief.harness.as_str(),
        brief.category,
        brief.category,
        brief.width,
        brief.height,
        brief.frames,
        brief.fps,
        brief.preset,
        generation.map(|value| value.quality.as_str()).unwrap_or("automatic"),
        command.unwrap_or("none"),
        motion_plan_text,
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
    fn routes_an_explicit_single_frame_herbalist_as_a_character() {
        let prompt = studio_prompt(
            "make one original cozy chibi herbalist character, single frame",
            Some("Selected style preset: Cozy chibi. polished cozy chibi game character, rounded proportions, oversized expressive head, clean dark outline and simple readable shapes."),
            None,
            None,
        );
        assert!(prompt.contains("routed harness: character"));
        assert!(prompt.contains("asset category: characters"));
        assert!(prompt.contains("default frames: 1"));
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
        assert!(prompt.contains("default frames: 8"));
        assert!(prompt.contains("playback FPS: 12"));
        assert!(prompt.contains("chat quality preset: high"));
        assert!(prompt.contains("slash command: animate"));
        assert!(prompt.contains("python3 .sprite-studio/sprite_rig.py"));
        assert!(prompt.contains("8-frame walk: left contact, down, passing, up"));
        assert!(
            prompt.contains("ImageGen pose sheets and independently generated frames are rejected")
        );
        assert!(prompt.contains(
            "If a context asset is supplied, use it as the master without calling ImageGen"
        ));
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
}
