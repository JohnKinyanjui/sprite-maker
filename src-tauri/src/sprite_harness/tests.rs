use super::{
    clean_refined_prompt_response, explicit_size, infer_brief, parts_master_canvas,
    refine_generation_prompt, rig_frames_prompt, studio_prompt, HarnessKind, SpriteBrief,
};
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
fn infers_top_down_adventure_and_nes_eight_bit_from_named_styles() {
    let top_down = infer_brief("top-down adventure");
    assert_eq!((top_down.width, top_down.height), (16, 24));
    assert_eq!((top_down.frames, top_down.fps), (4, 8));
    assert_eq!(top_down.preset, "top-down adventure");

    let nes = infer_brief("NES 8-bit hero");
    assert_eq!((nes.width, nes.height), (32, 32));
    assert_eq!((nes.frames, nes.fps), (1, 8));
    assert_eq!(nes.preset, "nes 8-bit");
}

#[test]
fn applies_compact_roguelike_canvas_from_selected_art_direction() {
    let prompt = studio_prompt(
        "make a knight",
        Some("Selected art direction: Compact roguelike"),
        None,
        None,
        None,
        None,
        false,
    );
    assert!(prompt.contains("logical canvas: 16x16 pixels"));
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
    let character = studio_prompt(
        "character run cycle",
        None,
        Some(&generation),
        Some("animate"),
        None,
        None,
        false,
    );
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
    let effect = studio_prompt(
        "explosion effect",
        None,
        Some(&generation),
        Some("animate"),
        None,
        None,
        false,
    );
    assert_eq!(
        effect
            .matches("This routed job has no paired-limb identity requirement.")
            .count(),
        1,
        "effects must skip the limb identity lock"
    );
    let single = studio_prompt(
        "character portrait",
        None,
        Some(&generation),
        Some("sprite"),
        None,
        None,
        false,
    );
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
        None,
        None,
        false,
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
            None,
        None,
        false,
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
            None,
        None,
        false,
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
        None,
        None,
        false,
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
    let prompt = studio_prompt("make a potion icon", None, None, None, None, None, false);
    assert!(prompt.contains(".sprite-studio/sprite_tool.py"));
    assert!(prompt.contains("WORKSPACE PYTHON RUNTIME"));
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
            None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
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
        None,
        None,
        false,
    );
    assert!(prompt.contains("saved deterministic rig"));
    assert!(prompt.contains("Rig only (default)"));
    assert!(prompt.contains("rough rig frame as pose-canonical"));
    assert!(prompt.contains("REAL-WORLD PHYSICAL MOTION CONTRACT"));
    assert!(prompt.contains("PHYSICAL ENVELOPE: scale <meters>; speed <m/s>"));
    assert!(prompt.contains("pixels_per_meter = observed_subject_pixel_height_or_length"));
    assert!(prompt
        .contains("User-stated physical quantities and explicit stylization override estimates"));
    assert!(prompt.contains("speed × cycle duration"));
    assert!(prompt.contains("final-to-first"));
    assert!(prompt.contains("limb count"));
    assert!(prompt.contains("dirty alpha"));
    assert!(prompt.contains("validate the loop headlessly"));
    assert!(prompt.contains("sprite_rig.py"));
    assert!(!prompt.contains("one high-quality AI frame per image call"));
}

#[test]
fn generation_prompts_validate_headlessly_within_a_bounded_budget() {
    // Reproduced: "make a running man" stalled because the agent tried to open
    // a Chrome playback preview for a required three-cycle approval, then
    // looped on one-pixel seam repairs.
    let prompts = [
        studio_prompt("make a running man", None, None, None, None, None, false),
        studio_prompt(
            "/animate this rabbit hopping",
            Some("Context asset: assets/creatures/rabbit.png"),
            None,
            Some("animate"),
            None,
            None,
            false,
        ),
        studio_prompt(
            "/pack forest potions",
            None,
            None,
            Some("pack"),
            None,
            None,
            false,
        ),
        studio_prompt("make a running man", None, None, None, None, None, true),
    ];
    for prompt in &prompts {
        assert!(
            !prompt.contains("preview at least three"),
            "prompt still demands an interactive playback preview"
        );
        assert!(!prompt.contains("three complete cycles"));
    }
    for prompt in &prompts[..3] {
        assert!(prompt.contains("Never open a browser"));
        assert!(prompt.contains("computer-use"));
        assert!(prompt.contains("at most three validate→fix cycles"));
        assert!(prompt.contains("never iterate pixel by pixel"));
        assert!(prompt.contains("GENERATION_FAILED: <reason>"));
    }
    let running_man = &prompts[0];
    assert!(running_man.contains("do not open Chrome or any browser"));
    assert!(running_man.contains("continue with the headless checks"));
    assert!(running_man.contains("sprite_rig_analyze_motion"));
    assert!(running_man.contains("enforces this budget at runtime"));
    // Reproduced: Codex has no Sprite Studio MCP rig tools and answered
    // GENERATION_FAILED because the embedded harness names only MCP calls.
    assert!(running_man.contains("missing MCP tools are never a reason to fail"));
    assert!(running_man.contains("sprite_rig.py --check RIG.json"));
}

#[test]
fn every_generation_embeds_one_silent_visual_retry() {
    let prompt = studio_prompt(
        "/animate a small dragon hovering",
        None,
        None,
        Some("animate"),
        None,
        None,
        false,
    );
    assert!(prompt.contains("# Internal visual acceptance loop"));
    assert!(prompt.contains("render exactly one replacement attempt"));
    assert!(prompt.contains("wing root continuously anchored"));
    assert!(prompt.contains("do not expose the review transcript"));
}

#[test]
fn cursor_runs_override_imagegen_with_generate_image() {
    let prompt = studio_prompt(
        "make me a character, single frame",
        Some("Selected style preset: Cozy chibi. rounded cartoon"),
        None,
        None,
        Some("cursor"),
        None,
        false,
    );
    assert!(prompt.contains("CURSOR IMAGE CONTRACT"));
    assert!(prompt.contains("GenerateImage"));
    assert!(prompt.contains("Do not call"));
    assert!(prompt.contains("image_gen__imagegen"));

    let pack = studio_prompt(
        "/pack six forest animals in one-bit style",
        None,
        None,
        Some("pack"),
        Some("cursor"),
        None,
        false,
    );
    assert!(pack.contains("CURSOR IMAGE CONTRACT"));
    assert!(pack.contains("GenerateImage"));
}

#[test]
fn antigravity_runs_override_imagegen_with_generate_image() {
    let prompt = studio_prompt(
        "make me a character, single frame",
        Some("Selected style preset: Cozy chibi. rounded cartoon"),
        None,
        None,
        Some("antigravity"),
        None,
        false,
    );
    assert!(prompt.contains("ANTIGRAVITY IMAGE CONTRACT"));
    assert!(prompt.contains("generate_image"));
    assert!(prompt.contains("Do not call"));
    assert!(prompt.contains("image_gen__imagegen"));
    assert!(!prompt.contains("CURSOR IMAGE CONTRACT"));

    let pack = studio_prompt(
        "/pack six forest animals in one-bit style",
        None,
        None,
        Some("pack"),
        Some("antigravity"),
        None,
        false,
    );
    assert!(pack.contains("ANTIGRAVITY IMAGE CONTRACT"));
    assert!(pack.contains("generate_image"));
}

#[test]
fn native_rig_master_only_skips_mask_rig_instructions() {
    let prompt = studio_prompt(
        "create a walking knight",
        Some("Selected art direction: Pixel RPG"),
        None,
        Some("animate"),
        None,
        None,
        true,
    );
    assert!(prompt.contains("native rig animation"));
    assert!(prompt.contains("Do not write mask rigs"));
    assert!(prompt.contains("MASTER HANDOFF CONTRACT"));
    assert!(prompt.contains("assets/characters/<slug>.png"));
    assert!(prompt.contains(".sprite-studio/last-generation.json"));
    assert!(prompt.contains("category: \"characters\""));
    assert!(prompt.contains("Do not report success unless both"));
    assert!(!prompt.contains("RIG_PLANNING_CONTRACT"));
}

#[test]
fn refine_generation_prompt_includes_draft_command_and_canvas() {
    let generation = GenerationOptions {
        quality: "mid".into(),
        width: 64,
        height: 64,
        frames: 8,
        fps: 12,
        frame_mode: "fixed".into(),
        min_frames: 8,
        max_frames: 12,
        allow_interpolation: true,
        allow_auto_adjust: false,
    };
    let prompt = refine_generation_prompt(
        "/animate cavaliere blu che corre",
        Some("Selected art direction: Pixel RPG"),
        Some("animate"),
        Some(&generation),
        Some("rig"),
    );
    assert!(prompt.contains("PROMPT REFINER CONTRACT"));
    assert!(prompt.contains("USER DRAFT\n/animate cavaliere blu che corre"));
    assert!(prompt.contains("ANIMATION"));
    assert!(prompt.contains("Canvas: 64x64"));
    assert!(prompt.contains("Animation polish mode selected in chat: rig."));
}

#[test]
fn clean_refined_prompt_response_strips_code_fences() {
    assert_eq!(
        clean_refined_prompt_response("```\n/animate A crisp 8-frame run cycle.\n```"),
        "/animate A crisp 8-frame run cycle."
    );
}

#[test]
fn humanoid_master_phase_asks_for_a_parts_sheet() {
    let generation = GenerationOptions {
        quality: "mid".into(),
        width: 128,
        height: 128,
        frames: 1,
        fps: 1,
        frame_mode: "fixed".into(),
        min_frames: 1,
        max_frames: 1,
        allow_interpolation: false,
        allow_auto_adjust: true,
    };
    assert_eq!(
        parts_master_canvas("make a running man", "", Some(&generation), None, true),
        Some((128, 128))
    );
    // Only master-only humanoid requests switch contracts.
    assert_eq!(parts_master_canvas("make a running man", "", Some(&generation), None, false), None);
    assert_eq!(parts_master_canvas("create a butterfly flying around", "", None, None, true), None);
    assert_eq!(parts_master_canvas("make a running man", "", None, Some("pack"), true), None);
    let prompt = studio_prompt("make a running man", None, Some(&generation), None, Some("codex"), None, true);
    let deliverable = prompt.find("DELIVERABLE: separate body parts").expect("parts deliverable");
    assert!(deliverable < prompt.find("Create exactly ONE transparent").unwrap(), "parts come first");
    assert!(prompt.contains(".sprite-studio/parts/<slug>/"));
    assert!(prompt.contains("thigh_far.png"));
    assert!(prompt.contains("\"canvas\": [128, 128]"));
    assert!(prompt.contains("MASTER HANDOFF CONTRACT"), "single master stays as the fallback");
    let creature = studio_prompt("create a butterfly flying around", None, None, None, None, None, true);
    assert!(!creature.contains("PARTS HANDOFF CONTRACT"));
}

#[test]
fn pose_prompt_for_a_parts_rig_fixes_the_skeleton_and_rotation_signs() {
    let point = |name: &str, kind: &str, y: f64| crate::rig::RigPoint {
        id: name.into(),
        name: name.into(),
        kind: kind.into(),
        x: 64.0,
        y,
        confidence: 1.0,
        source: "parts".into(),
        note: None,
    };
    let points = vec![point("hip_r", "joint", 70.0), point("knee_r", "joint", 90.0), point("foot_r", "contact", 120.5)];
    let bones = vec![crate::rig::RigBone {
        id: "b1".into(),
        name: "thigh_r".into(),
        start_point: "hip_r".into(),
        end_point: "knee_r".into(),
        radius: 3.0,
        parent: Some("torso".into()),
        z: 7,
    }];
    let prompt = rig_frames_prompt("run cycle", 128, 128, &points, &bones);
    assert!(prompt.contains("Do not propose points or bones"));
    assert!(prompt.contains("ground line y ≈ 120.5"));
    assert!(prompt.contains("- thigh_r: hip_r → knee_r, parent torso, z 7"));
    assert!(prompt.contains("FORWARD (toward +x) is NEGATIVE"));
    assert!(prompt.contains("Never bend a knee forward"));
    assert!(prompt.contains("MOTION INTENT\nrun cycle"));
}
