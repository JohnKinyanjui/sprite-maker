#[derive(Debug, PartialEq)]
pub(super) enum HarnessKind {
    Character,
    Creature,
    Prop,
    Terrain,
    Tileset,
    Effect,
}

impl HarnessKind {
    pub(super) fn as_str(&self) -> &'static str {
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
pub(super) struct SpriteBrief {
    pub(super) harness: HarnessKind,
    pub(super) category: &'static str,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) frames: u32,
    pub(super) fps: u32,
    pub(super) preset: &'static str,
}

pub(super) fn explicit_size(prompt: &str) -> Option<(u32, u32)> {
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

pub(super) fn has_word(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == expected)
}

pub(super) fn explicit_count(prompt: &str, unit: &str) -> Option<u32> {
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

pub(super) fn inferred_style(text: &str) -> Option<(u32, u32, &'static str, u32, u32)> {
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
    } else if lower.contains("top-down-adventure") || lower.contains("top-down adventure") {
        Some((16, 24, "top-down adventure", 4, 8))
    } else if lower.contains("snes-action-rpg") || lower.contains("snes-era action rpg") {
        Some((24, 32, "snes-era action rpg", 4, 10))
    } else if lower.contains("compact-roguelike") || lower.contains("compact roguelike") {
        Some((16, 16, "compact roguelike", 2, 6))
    } else if lower.contains("nes-eight-bit") || lower.contains("nes 8-bit") {
        Some((32, 32, "nes 8-bit", 1, 8))
    } else if lower.contains("dark-fantasy-pixel") || lower.contains("dark fantasy pixel") {
        Some((48, 64, "dark fantasy pixel", 4, 8))
    } else if lower.contains("paper-cutout") || lower.contains("paper cutout") {
        Some((128, 160, "paper cutout", 4, 8))
    } else if has_word(&lower, "watercolor") {
        Some((192, 192, "watercolor", 1, 1))
    } else if lower.contains("comic-ink") || lower.contains("comic ink") {
        Some((128, 128, "comic ink", 1, 10))
    } else if lower.contains("neon-synth") || lower.contains("neon synth") {
        Some((128, 128, "neon synth", 1, 10))
    } else if has_word(&lower, "clay") {
        Some((128, 128, "clay", 1, 10))
    } else if has_word(&lower, "voxel") {
        Some((64, 64, "voxel", 1, 1))
    } else if lower.contains("platform") || lower.contains("side-scroller") {
        Some((32, 32, "pixel platformer", 6, 12))
    } else {
        None
    }
}

pub(super) fn has_explicit_asset_subject(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    [
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
        "character",
        "hero",
        "npc",
        "knight",
        "farmer",
        "herbalist",
        "tile",
        "tileset",
        "tilemap",
        "terrain",
        "ground",
        "tree",
        "bush",
        "plant",
        "rock",
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
        "prop",
        "item",
        "icon",
        "potion",
        "weapon",
        "object",
        "chest",
        "door",
        "machine",
        "vehicle",
        "torch",
        "turret",
    ]
    .iter()
    .any(|word| has_word(&lower, word))
}

pub(super) fn asset_identity_context(context: &str) -> String {
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

pub(super) fn infer_brief(prompt: &str) -> SpriteBrief {
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
