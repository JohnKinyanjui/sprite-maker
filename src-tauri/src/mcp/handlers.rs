use super::schema::DEFAULT_PROVIDER;
use crate::{
    animations::export_animation_inner,
    assets::{export_asset_inner, scan_generation_assets_inner},
    pipeline::{
        character_contract_check_inner, get_anchor_inner, list_anchors_inner,
        clean_alpha_animation_inner, harden_animation_inner, normalize_animation_inner,
        nudge_animation_frames_inner, promote_anchor_inner, snap_animation_offsets_inner,
        set_animation_review_status_inner, size_contract_check_inner, orient_anchor_inner,
        queue_contract_retry_inner, queue_direction_set_inner, extract_video_frames_inner,
        finalize_contract_retry_inner, retry_size_contract_inner, score_strip_inner,
        split_sprite_strip_inner, detect_anchor_facing_inner, mirror_animation_inner,
        get_character_profile_inner,
    },
    pipeline::{
        export_character_pack_inner, list_facing_checks_inner, ListFacingChecksInput,
        production_score_inner, queue_motion_batch_inner, queue_region_regen_inner,
        score_animation_frames_inner,
    },
    rig::{
        analyze_rig_fit_inner, interpolate_rig_frames_inner, InterpolateRigFramesInput,
        render_rig_animation_blocking, save_rig_inner, suggest_rig_points_inner,
    },
    conversations::{create_conversation_inner, get_conversation, get_message},
    error::{CommandError, CommandResult},
    models::{GenerationOptions, ProviderRequestOptions, TerrainExportInput},
    providers::{detect_providers_inner, start_provider_run},
    quality::{get_quality_report_inner, queue_quality_analysis_inner},
    references::{import_reference_image_inner, set_conversation_reference_inner},
    settings::{get_setting_value, set_setting_value},
    terrain::export_godot_tileset_inner,
    workspace::{create_workspace_inner, open_workspace_inner},
    worktrees::list_worktrees_inner,
    AppState, GenerationSnapshot,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(crate) fn studio_status(
    state: &AppState,
    db_path: &Path,
) -> CommandResult<super::schema::StatusPayload> {
    let providers = detect_providers_inner(state);
    Ok(super::schema::StatusPayload {
        version: env!("CARGO_PKG_VERSION"),
        db_path: db_path.to_string_lossy().into_owned(),
        db_ok: db_path.is_file() || db_path.parent().is_some_and(Path::is_dir),
        providers,
    })
}

pub(crate) fn open_or_create_workspace(
    state: &AppState,
    params: super::schema::OpenWorkspaceParams,
) -> CommandResult<crate::models::Workspace> {
    let path = PathBuf::from(&params.path);
    if path.is_dir() {
        open_workspace_inner(params.path, state)
    } else {
        let name = params
            .name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Untitled workspace")
                    .to_string()
            });
        create_workspace_inner(name, params.path, state)
    }
}

pub(crate) fn ensure_conversation(
    state: &AppState,
    params: super::schema::EnsureConversationParams,
) -> CommandResult<crate::models::Conversation> {
    let provider = resolved_provider(params.provider.as_deref());
    let worktrees = list_worktrees_inner(&params.workspace_id, state)?;
    let general = worktrees
        .iter()
        .find(|worktree| worktree.kind == "general")
        .or_else(|| worktrees.first());
    let conversation = create_conversation_inner(
        params.workspace_id,
        general.map(|worktree| worktree.id.clone()),
        Some("MCP chat".into()),
        Some(provider),
        state,
    )?;
    if let Some(style) = params.style_preset.filter(|value| !value.trim().is_empty()) {
        set_setting_value(
            state,
            &format!("conversation-style:{}", conversation.id),
            Value::String(style),
        )?;
    }
    Ok(conversation)
}

pub(crate) fn resolved_provider(requested: Option<&str>) -> String {
    match requested.map(str::trim).filter(|value| !value.is_empty()) {
        Some(provider) => provider.to_string(),
        None => DEFAULT_PROVIDER.to_string(),
    }
}

pub(crate) fn attach_references(
    state: &AppState,
    params: super::schema::AttachReferencesParams,
) -> CommandResult<Vec<String>> {
    let conversation = get_conversation(state, &params.conversation_id)?;
    let worktree_id = conversation.worktree_id.clone().ok_or_else(|| {
        CommandError::new(
            "worktree_required",
            "Attach references to a chat that belongs to a worktree",
        )
    })?;
    let mut ids = params.reference_ids.unwrap_or_default();
    for path in params.paths.unwrap_or_default() {
        let imported = import_reference_image_inner(
            worktree_id.clone(),
            path,
            "character_appearance".into(),
            None,
            state,
        )?;
        ids.push(imported.id);
    }
    for id in &ids {
        set_conversation_reference_inner(&params.conversation_id, id, true, Some(1.0), state)?;
    }
    Ok(ids)
}

pub(crate) fn starts_with_animate_command(prompt: &str) -> bool {
    let trimmed = prompt.trim_start();
    if !trimmed.starts_with("/animate") {
        return false;
    }
    match trimmed.get(8..) {
        None => true,
        Some(rest) => rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t'),
    }
}

fn contains_word_token(haystack: &str, token: &str) -> bool {
    let lower = haystack.to_ascii_lowercase();
    let token_lower = token.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let token_bytes = token_lower.as_bytes();
    if token_bytes.is_empty() {
        return false;
    }
    let mut index = 0usize;
    while index + token_bytes.len() <= bytes.len() {
        if bytes[index..index + token_bytes.len()] == token_bytes[..] {
            let before_ok = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
            let after_index = index + token_bytes.len();
            let after_ok =
                after_index >= bytes.len() || !bytes[after_index].is_ascii_alphanumeric();
            if before_ok && after_ok {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn motion_intent(prompt: &str) -> bool {
    const TOKENS: &[&str] = &[
        "walk", "walks", "walking", "run", "runs", "running", "hop", "hops", "hopping", "animate",
        "animation", "idle", "fly", "flying", "crawl", "crawls", "crawling", "loop", "looping",
        "cycle",
    ];
    TOKENS.iter().any(|token| contains_word_token(prompt, token))
}

fn new_character_intent(prompt: &str) -> bool {
    const TOKENS: &[&str] = &["create", "generate", "make", "design", "draw", "build"];
    TOKENS.iter().any(|token| contains_word_token(prompt, token))
}

pub(crate) fn needs_native_rig_master_only(prompt: &str, command: Option<&str>) -> bool {
    if command == Some("animate") || starts_with_animate_command(prompt) {
        return false;
    }
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("polish mode: ai polish") || lower.contains("polish mode: full redraw") {
        return false;
    }
    if command.is_some_and(|value| value != "animate") {
        return false;
    }
    motion_intent(prompt) && new_character_intent(prompt)
}

pub(crate) fn generate(
    state: &AppState,
    params: super::schema::GenerateParams,
) -> CommandResult<serde_json::Value> {
    let conversation = get_conversation(state, &params.conversation_id)?;
    let context = build_generation_context(state, &conversation)?;
    let reference_ids = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let mut statement = connection.prepare(
            "SELECT reference_id FROM conversation_references WHERE conversation_id=?1 AND active=1 ORDER BY created_at",
        )?;
        let rows = statement.query_map([&params.conversation_id], |row| row.get(0))?;
        rows.filter_map(Result::ok).collect::<Vec<String>>()
    };
    let native_rig_master_only =
        needs_native_rig_master_only(&params.prompt, params.command.as_deref());
    let options = ProviderRequestOptions {
        model: params.model,
        reasoning_effort: None,
        command: params.command,
        generation: params.generation.map(into_generation_options),
        reference_ids,
        image_provider_id: params.image_provider_id,
        native_rig_master_only,
    };
    let request_id = start_provider_run(
        params.conversation_id,
        params.prompt,
        Some(context),
        Some(options),
        None,
        state,
    )?;
    Ok(serde_json::json!({ "requestId": request_id }))
}

fn into_generation_options(options: super::schema::McpGenerationOptions) -> GenerationOptions {
    GenerationOptions {
        quality: options.quality.unwrap_or_else(|| "mid".into()),
        width: options.width.unwrap_or(48),
        height: options.height.unwrap_or(64),
        frames: options.frames.unwrap_or(4),
        fps: options.fps.unwrap_or(8),
        frame_mode: options.frame_mode.unwrap_or_else(|| "auto".into()),
        min_frames: options.min_frames.unwrap_or(8),
        max_frames: options.max_frames.unwrap_or(12),
        allow_interpolation: options.allow_interpolation.unwrap_or(true),
        allow_auto_adjust: true,
    }
}

fn build_generation_context(
    state: &AppState,
    conversation: &crate::models::Conversation,
) -> CommandResult<String> {
    let mut parts = Vec::new();
    if let Some(worktree_id) = conversation.worktree_id.as_deref() {
        if let Some(worktree) = list_worktrees_inner(&conversation.workspace_id, state)?
            .into_iter()
            .find(|item| item.id == worktree_id)
        {
            parts.push(format!(
                "Active project section: {}. {}",
                worktree.name,
                worktree.description.unwrap_or_default()
            ));
        }
    }
    parts.push(style_context_line(state, conversation)?);
    if let Some(skills) = enabled_skill_context(state)? {
        parts.push(skills);
    }
    Ok(parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(crate) fn style_context_line(
    state: &AppState,
    conversation: &crate::models::Conversation,
) -> CommandResult<String> {
    let conversation_style =
        get_setting_value(state, &format!("conversation-style:{}", conversation.id))?;
    let workspace_style = get_setting_value(
        state,
        &format!("workspace-style:{}", conversation.workspace_id),
    )?;
    let custom = get_setting_value(state, "custom-arts")?;
    let id = conversation_style
        .as_str()
        .filter(|value| *value != "inherit" && !value.is_empty())
        .or_else(|| workspace_style.as_str().filter(|value| !value.is_empty()))
        .unwrap_or("pixel-rpg");
    let (name, prompt) = resolve_style(id, &custom);
    Ok(format!("Selected art direction: {name}. {prompt}."))
}

fn resolve_style(id: &str, custom: &Value) -> (String, String) {
    if let Some(arts) = custom.as_array() {
        for art in arts {
            if art.get("id").and_then(Value::as_str) == Some(id) {
                return (
                    art.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(id)
                        .to_string(),
                    art.get("prompt")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                );
            }
        }
    }
    builtin_style(id)
}

fn builtin_style(id: &str) -> (String, String) {
    for (style_id, name, prompt) in BUILTIN_STYLES {
        if *style_id == id {
            return ((*name).to_string(), (*prompt).to_string());
        }
    }
    (
        "Pixel RPG".into(),
        "crisp handcrafted pixel RPG character, compact readable clusters, warm restrained palette, clear face and silhouette".into(),
    )
}

pub(crate) const BUILTIN_STYLES: &[(&str, &str, &str)] = &[
    ("pixel-rpg", "Pixel RPG", "crisp handcrafted pixel RPG character, compact readable clusters, warm restrained palette, clear face and silhouette"),
    ("graphic-adventure", "Graphic adventure", "premium graphic adventure character, bold angular silhouette, simplified painted planes, controlled asymmetry and layered costume shapes"),
    ("cozy-chibi", "Cozy chibi", "polished cozy chibi game character, rounded proportions, oversized expressive head, clean dark outline and simple readable shapes"),
    ("limited-palette", "Limited palette", "handcrafted limited-palette pixel art, deliberate pixel clusters, one compact color ramp, selective dithering, crisp silhouette and no soft antialiasing"),
    ("isometric-pixel", "Isometric pixel", "polished 2:1 isometric pixel game art, consistent projection and top-left lighting, readable top and side planes, compact controlled palette"),
    ("painterly-fantasy", "Painterly fantasy", "original painterly fantasy game art, softly textured brushwork, layered material shapes, atmospheric color harmony, readable gameplay silhouette"),
    ("cel-shaded", "Cel shaded", "clean cel-shaded 2D game art, confident dark contour, flat graphic shadow shapes, saturated accent colors, highly readable silhouette"),
    ("one-bit", "One-bit", "high-clarity one-bit pixel art using exactly two colors, bold negative space, intentional clusters, no gray pixels and no antialiasing"),
    ("top-down-adventure", "Top-down adventure", "classic top-down adventure pixel character, compact 16 by 24 scale, down-facing walk-ready silhouette, short body and clear head, iconic readable equipment shapes such as shield and sword, warm limited palette, crisp clusters, no antialiasing"),
    ("snes-action-rpg", "SNES-era action RPG", "SNES-era action RPG pixel hero, taller 24 by 32 proportions, chunky readable clusters, three-quarter idle stance, restrained warm 16-bit ramp, clear cape and weapon shapes, crisp pixels without soft antialiasing"),
    ("compact-roguelike", "Compact roguelike", "tiny compact roguelike pixel character on a 16 by 16 canvas, high-contrast clusters, hybrid top-down and front facing, bold readable silhouette, minimal color count, no antialiasing"),
    ("pixel-platformer", "Pixel platformer", "side-view pixel platformer character, 32 by 32 scale, run-ready silhouette, grounded feet and clear weight, readable jump pose, compact clusters, saturated platformer palette, crisp pixels"),
    ("nes-eight-bit", "NES 8-bit", "authentic NES-era 8-bit pixel character, tight four-color ramp, chunky blocky clusters, bold silhouette, no antialiasing, no gradients, 32 by 32 game scale"),
    ("dark-fantasy-pixel", "Dark fantasy pixel", "dark fantasy pixel RPG character at 48 by 64 game scale, muted grim palette of ash, rust, and deep greens, compact readable clusters, clear silhouette armor and cloak, restrained highlights, no soft antialiasing"),
    ("paper-cutout", "Paper cutout", "handmade paper-cutout game character, stacked colored paper layers with visible edge thickness, slight drop shadow, collage craft look, flat matte paper colors, readable silhouette, not photorealistic"),
    ("watercolor", "Watercolor", "soft watercolor game character illustration, pigment blooms and paper grain, controlled wet edges, translucent layered washes, readable silhouette, storybook fantasy palette, not photorealistic"),
    ("comic-ink", "Comic ink", "inked comic-book game character, bold black contours, flat color fills, sparse hatching and graphic print look, high readability silhouette, saturated print palette, not photorealistic"),
    ("neon-synth", "Neon synth", "neon synthwave game character on a dark ground, electric magenta and cyan rim lighting, night silhouette, glossy dark materials with glowing accents, readable shape, not photorealistic"),
    ("clay", "Clay", "clay stop-motion game character, soft sculpted clay volumes, subtle fingerprint texture, rounded handmade forms, warm studio lighting, readable silhouette, not photorealistic"),
    ("voxel", "Voxel", "cubic voxel game character, hard cube volumes, isometric three-quarter view, limited face colors per block, blocky Minecraft-like construction but original design, crisp cubic edges, readable silhouette"),
];

fn enabled_skill_context(state: &AppState) -> CommandResult<Option<String>> {
    let value = get_setting_value(state, "custom-skills")?;
    let Some(skills) = value.as_array() else {
        return Ok(None);
    };
    let mut blocks = Vec::new();
    for skill in skills {
        if skill.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let name = skill.get("name").and_then(Value::as_str).unwrap_or("Skill");
        let instructions = skill
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !instructions.trim().is_empty() {
            blocks.push(format!("Custom skill {name}: {instructions}"));
        }
    }
    Ok((!blocks.is_empty()).then_some(blocks.join("\n")))
}

pub(crate) fn get_generation(
    state: &AppState,
    request_id: &str,
) -> CommandResult<GenerationSnapshot> {
    let mut snapshot = state.generation_snapshot(request_id).ok_or_else(|| {
        CommandError::new(
            "request_not_found",
            "No generation with that requestId is tracked in this MCP process",
        )
    })?;
    if !snapshot.assistant_id.is_empty() {
        if let Ok(message) = get_message(state, &snapshot.assistant_id) {
            snapshot.status = if matches!(
                snapshot.status.as_str(),
                "completed" | "failed" | "cancelled"
            ) {
                snapshot.status
            } else {
                message.status
            };
            if !message.content.is_empty() {
                snapshot.last_content = Some(message.content);
            }
        }
    }
    Ok(snapshot)
}

pub(crate) fn list_artifacts(
    state: &AppState,
    workspace_id: &str,
) -> CommandResult<Vec<serde_json::Value>> {
    let assets = scan_generation_assets_inner(workspace_id, None, None, state)?;
    Ok(assets
        .into_iter()
        .map(|asset| {
            serde_json::json!({
                "id": asset.id,
                "name": asset.name,
                "relativePath": asset.relative_path,
                "category": asset.category,
                "width": asset.width,
                "height": asset.height,
            })
        })
        .collect())
}

pub(crate) fn export_item(
    state: &AppState,
    params: super::schema::ExportParams,
) -> CommandResult<serde_json::Value> {
    match params.kind.as_str() {
        "asset" => {
            let result = export_asset_inner(&params.id, state)?;
            Ok(serde_json::to_value(result).unwrap_or(Value::Null))
        }
        "animation" => {
            let result = export_animation_inner(
                &params.id,
                params.destination,
                params.export_format.as_deref(),
                state,
            )?;
            Ok(serde_json::to_value(result).unwrap_or(Value::Null))
        }
        "godot_tileset" => {
            let input = TerrainExportInput {
                project_id: params.project_id.ok_or_else(|| {
                    CommandError::new("invalid_export", "Godot export needs projectId")
                })?,
                worktree_id: params.worktree_id.ok_or_else(|| {
                    CommandError::new("invalid_export", "Godot export needs worktreeId")
                })?,
                asset_id: params.id,
                name: params.name.unwrap_or_else(|| "tileset".into()),
                tile_width: params.tile_width.unwrap_or(32),
                tile_height: params.tile_height.unwrap_or(32),
                margin_x: 0,
                margin_y: 0,
                separation_x: 0,
                separation_y: 0,
                include_empty: false,
                terrain_name: None,
                terrain_mode: None,
                terrain_rules: Vec::new(),
            };
            let result = export_godot_tileset_inner(input, state)?;
            Ok(serde_json::to_value(result).unwrap_or(Value::Null))
        }
        _ => Err(CommandError::new(
            "invalid_export",
            "kind must be asset, animation, or godot_tileset",
        )),
    }
}

pub(crate) fn promote_anchor(
    state: &AppState,
    params: super::schema::PromoteAnchorParams,
) -> CommandResult<serde_json::Value> {
    let anchor = promote_anchor_inner(
        &params.workspace_id,
        &params.asset_id,
        params.slug.as_deref(),
        params.auto_orient.unwrap_or(false),
        params.view.as_deref(),
        state,
    )?;
    Ok(serde_json::to_value(anchor).unwrap_or(Value::Null))
}

pub(crate) fn check_anchor_facing(
    state: &AppState,
    params: super::schema::AnchorSlugParams,
) -> CommandResult<serde_json::Value> {
    let report = detect_anchor_facing_inner(&params.workspace_id, &params.slug, state)?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn orient_anchor(
    state: &AppState,
    params: super::schema::AnchorSlugParams,
) -> CommandResult<serde_json::Value> {
    let anchor = orient_anchor_inner(&params.workspace_id, &params.slug, state)?;
    Ok(serde_json::to_value(anchor).unwrap_or(Value::Null))
}

pub(crate) fn mirror_animation(
    state: &AppState,
    params: super::schema::MirrorAnimationParams,
) -> CommandResult<serde_json::Value> {
    let result = mirror_animation_inner(
        None,
        state,
        crate::models::MirrorAnimationInput {
            animation_id: params.animation_id,
            target_facing: params.target_facing,
            source_facing: params.source_facing,
            anchor_slug: params.anchor_slug,
            rig_id: params.rig_id,
        },
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn queue_direction_set(
    state: &AppState,
    params: super::schema::QueueDirectionSetParams,
) -> CommandResult<serde_json::Value> {
    let result = queue_direction_set_inner(
        crate::models::QueueDirectionSetInput {
            workspace_id: params.workspace_id,
            worktree_id: params.worktree_id,
            source_animation_id: params.source_animation_id,
            anchor_slug: params.anchor_slug,
            motion: params.motion,
            set: params.set,
            conversation_id: params.conversation_id,
        },
        None,
        state,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn clean_alpha(
    state: &AppState,
    params: super::schema::CleanAlphaParams,
) -> CommandResult<serde_json::Value> {
    let report = clean_alpha_animation_inner(state, &params.animation_id)?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn snap_to_pixel_grid(
    state: &AppState,
    params: super::schema::SnapToPixelGridParams,
) -> CommandResult<serde_json::Value> {
    let animation = snap_animation_offsets_inner(
        state,
        &params.animation_id,
        params.grid_size.unwrap_or(1),
    )?;
    Ok(serde_json::to_value(animation).unwrap_or(Value::Null))
}

pub(crate) fn harden_animation(
    state: &AppState,
    params: super::schema::HardenAnimationParams,
) -> CommandResult<serde_json::Value> {
    let report = harden_animation_inner(
        None,
        state,
        &params.animation_id,
        params.anchor_slug.as_deref(),
        params.source_path.as_deref(),
        params.frame_count,
        params.conversation_id.as_deref(),
        params.options.unwrap_or_default(),
    )?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn normalize_animation(
    state: &AppState,
    params: super::schema::NormalizeAnimationParams,
) -> CommandResult<serde_json::Value> {
    let animation = normalize_animation_inner(
        None,
        state,
        crate::models::NormalizeAnimationInput {
            animation_id: params.animation_id,
            anchor_slug: params.anchor_slug,
            lock_first_frame: params.lock_first_frame,
            shared_scale: params.shared_scale,
            padding: params.padding,
        },
    )?;
    Ok(serde_json::to_value(animation).unwrap_or(Value::Null))
}

pub(crate) fn score_strip(
    _state: &AppState,
    params: super::schema::ScoreStripParams,
) -> CommandResult<serde_json::Value> {
    let result = score_strip_inner(
        &params.source_path,
        params.frame_count,
        params.layout.as_deref(),
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn split_strip(
    state: &AppState,
    params: super::schema::SplitStripParams,
) -> CommandResult<serde_json::Value> {
    let result = split_sprite_strip_inner(
        &params.workspace_id,
        &params.source_path,
        &params.layout,
        params.frame_count,
        params.columns,
        params.recover_foreground.unwrap_or(true),
        params.category.as_deref().unwrap_or("characters"),
        state,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn extract_video_frames(
    state: &AppState,
    params: super::schema::ExtractVideoFramesParams,
) -> CommandResult<serde_json::Value> {
    let result = extract_video_frames_inner(
        state,
        &params.workspace_id,
        &params.video_path,
        params.fps,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn align_frames(
    state: &AppState,
    params: super::schema::AlignFramesParams,
) -> CommandResult<serde_json::Value> {
    let animation = nudge_animation_frames_inner(
        state,
        crate::models::NudgeAnimationFramesInput {
            animation_id: params.animation_id,
            deltas: params.deltas,
            apply_to_all: params.apply_to_all,
        },
    )?;
    Ok(serde_json::to_value(animation).unwrap_or(Value::Null))
}

pub(crate) fn check_size_contract(
    state: &AppState,
    params: super::schema::SizeContractParams,
) -> CommandResult<serde_json::Value> {
    let report = size_contract_check_inner(
        state,
        &params.animation_id,
        params.anchor_slug.as_deref(),
    )?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn check_character_contract(
    state: &AppState,
    params: super::schema::CharacterContractParams,
) -> CommandResult<serde_json::Value> {
    let report = character_contract_check_inner(
        state,
        &params.workspace_id,
        &params.worktree_id,
        params.anchor_slug.as_deref(),
    )?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn list_promoted_anchors(
    state: &AppState,
    workspace_id: &str,
) -> CommandResult<serde_json::Value> {
    let anchors = list_anchors_inner(workspace_id, state)?;
    Ok(serde_json::to_value(anchors).unwrap_or(Value::Null))
}

pub(crate) fn set_animation_review(
    state: &AppState,
    params: super::schema::SetReviewStatusParams,
) -> CommandResult<serde_json::Value> {
    let animation = set_animation_review_status_inner(
        state,
        &params.animation_id,
        &params.status,
    )?;
    Ok(serde_json::to_value(animation).unwrap_or(Value::Null))
}

pub(crate) fn save_rig(
    state: &AppState,
    input: crate::rig::RigInput,
) -> CommandResult<serde_json::Value> {
    let rig = save_rig_inner(input, state)?;
    Ok(serde_json::to_value(rig).unwrap_or(Value::Null))
}

pub(crate) fn render_rig_animation(
    state: &AppState,
    input: crate::rig::RigInput,
) -> CommandResult<serde_json::Value> {
    let result = render_rig_animation_blocking(input, state)?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn suggest_rig_points(
    state: &AppState,
    params: super::schema::SuggestRigParams,
) -> CommandResult<serde_json::Value> {
    let suggestion = suggest_rig_points_inner(
        state,
        &params.asset_id,
        params.morphology.as_deref(),
    )?;
    Ok(serde_json::to_value(suggestion).unwrap_or(Value::Null))
}

pub(crate) fn analyze_rig_fit(
    state: &AppState,
    params: super::schema::AssetIdParams,
) -> CommandResult<serde_json::Value> {
    let report = analyze_rig_fit_inner(state, &params.asset_id)?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn get_character_profile(
    state: &AppState,
    params: super::schema::AnchorSlugParams,
) -> CommandResult<serde_json::Value> {
    let profile = get_character_profile_inner(&params.workspace_id, &params.slug, state)?;
    Ok(serde_json::to_value(profile).unwrap_or(Value::Null))
}

pub(crate) fn retry_size_contract(
    state: &AppState,
    params: super::schema::RetrySizeContractParams,
) -> CommandResult<serde_json::Value> {
    let result = retry_size_contract_inner(
        state,
        &params.animation_id,
        params.anchor_slug.as_deref(),
        params.conversation_id.as_deref(),
        params.regenerate.unwrap_or(false),
        params.max_deterministic_passes,
        None,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn queue_contract_retry(
    state: &AppState,
    params: super::schema::QueueContractRetryParams,
) -> CommandResult<serde_json::Value> {
    let result = queue_contract_retry_inner(
        crate::models::QueueContractRetryInput {
            animation_id: params.animation_id,
            conversation_id: params.conversation_id,
            anchor_slug: params.anchor_slug,
            max_ai_attempts: params.max_ai_attempts,
            max_deterministic_passes: params.max_deterministic_passes,
        },
        None,
        state,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn finalize_contract_retry(
    state: &AppState,
    params: super::schema::FinalizeContractRetryParams,
) -> CommandResult<serde_json::Value> {
    let result = finalize_contract_retry_inner(
        state,
        &params.animation_id,
        &params.source_path,
        params.frame_count,
        params.anchor_slug.as_deref(),
        params.layout.as_deref(),
        None,
    )?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn get_promoted_anchor(
    state: &AppState,
    workspace_id: &str,
    slug: &str,
) -> CommandResult<serde_json::Value> {
    let anchor = get_anchor_inner(workspace_id, slug, state)?;
    Ok(serde_json::to_value(anchor).unwrap_or(Value::Null))
}

pub(crate) fn quality_report(
    state: &AppState,
    params: super::schema::QualityParams,
) -> CommandResult<serde_json::Value> {
    let existing = get_quality_report_inner(&params.animation_id, state)?;
    if params.analyze.unwrap_or(false)
        || existing
            .as_ref()
            .is_none_or(|report| report.status != "completed")
    {
        let job = queue_quality_analysis_inner(params.animation_id, None, state)?;
        return Ok(serde_json::json!({ "jobId": job.id, "status": job.status }));
    }
    Ok(serde_json::to_value(existing).unwrap_or(Value::Null))
}

pub(crate) fn queue_motion_batch(
    state: &AppState,
    input: crate::models::QueueMotionBatchInput,
) -> CommandResult<serde_json::Value> {
    let result = queue_motion_batch_inner(input, None, state)?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn queue_region_regen(
    state: &AppState,
    input: crate::models::QueueRegionRegenInput,
) -> CommandResult<serde_json::Value> {
    let result = queue_region_regen_inner(input, None, state)?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn export_character_pack(
    state: &AppState,
    input: crate::models::ExportCharacterPackInput,
) -> CommandResult<serde_json::Value> {
    if input.destination.trim().is_empty() {
        return Err(CommandError::new(
            "destination_required",
            "destination is required for headless character pack export",
        ));
    }
    let result = export_character_pack_inner(state, &input)?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn get_production_score(
    state: &AppState,
    params: super::schema::ProductionScoreParams,
) -> CommandResult<serde_json::Value> {
    if params.worktree_id.trim().is_empty() {
        return Err(CommandError::new(
            "missing_worktree",
            "worktreeId is required for production score",
        ));
    }
    let report = production_score_inner(
        state,
        &params.workspace_id,
        &params.worktree_id,
        params.anchor_slug.as_deref(),
    )?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn interpolate_rig_frames(
    state: &AppState,
    input: InterpolateRigFramesInput,
) -> CommandResult<serde_json::Value> {
    let result = interpolate_rig_frames_inner(input, state)?;
    Ok(serde_json::to_value(result).unwrap_or(Value::Null))
}

pub(crate) fn score_animation_frames(
    state: &AppState,
    params: super::schema::ScoreAnimationFramesParams,
) -> CommandResult<serde_json::Value> {
    let report = score_animation_frames_inner(&params.animation_id, state)?;
    Ok(serde_json::to_value(report).unwrap_or(Value::Null))
}

pub(crate) fn list_facing_checks(
    state: &AppState,
    input: ListFacingChecksInput,
) -> CommandResult<serde_json::Value> {
    let reports =
        list_facing_checks_inner(&input.workspace_id, input.slug.as_deref(), state)?;
    Ok(serde_json::to_value(reports).unwrap_or(Value::Null))
}

#[cfg(test)]
mod native_rig_master_routing_tests {
    use super::{needs_native_rig_master_only, starts_with_animate_command};

    #[test]
    fn animate_command_skips_master_only_routing() {
        assert!(!needs_native_rig_master_only("/animate walk cycle", Some("animate")));
        assert!(!needs_native_rig_master_only("/animate walk cycle", None));
    }

    #[test]
    fn create_walk_requests_need_master_only_routing() {
        assert!(needs_native_rig_master_only("create a warrior walking forward", None));
    }

    #[test]
    fn ai_polish_prompts_skip_master_only_routing() {
        assert!(!needs_native_rig_master_only(
            "create a warrior that walks. Polish mode: AI polish.",
            None,
        ));
    }

    #[test]
    fn butterfly_substring_does_not_trigger_fly_motion() {
        assert!(!needs_native_rig_master_only("create a butterfly", None));
        assert!(needs_native_rig_master_only("create a butterfly flying around", None));
    }

    #[test]
    fn animate_prefix_requires_word_boundary() {
        assert!(starts_with_animate_command("/animate walk"));
        assert!(!starts_with_animate_command("/animated walk"));
    }
}

#[cfg(test)]
mod generation_option_tests {
    use super::into_generation_options;
    use super::super::schema::McpGenerationOptions;

    #[test]
    fn defaults_allow_interpolation_to_true() {
        let options = into_generation_options(McpGenerationOptions {
            quality: None,
            width: None,
            height: None,
            frames: None,
            fps: None,
            frame_mode: None,
            min_frames: None,
            max_frames: None,
            allow_interpolation: None,
        });
        assert!(options.allow_interpolation);
    }

    #[test]
    fn honors_explicit_allow_interpolation_false() {
        let options = into_generation_options(McpGenerationOptions {
            quality: None,
            width: None,
            height: None,
            frames: None,
            fps: None,
            frame_mode: None,
            min_frames: None,
            max_frames: None,
            allow_interpolation: Some(false),
        });
        assert!(!options.allow_interpolation);
    }
}
