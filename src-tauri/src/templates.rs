use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{
        AnimationFrame, AnimationTemplate, AnimationTemplatePhase, GenerationOptions,
        TemplateApplication,
    },
    motion_planner::build_motion_plan,
    AppState,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;
use uuid::Uuid;

fn template_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnimationTemplate> {
    Ok(AnimationTemplate {
        id: row.get(0)?,
        project_id: row.get(1)?,
        source_animation_id: row.get(2)?,
        name: row.get(3)?,
        intent: row.get(4)?,
        motion_description: row.get(5)?,
        direction: row.get(6)?,
        looping: row.get(7)?,
        fps: row.get(8)?,
        width: row.get(9)?,
        height: row.get(10)?,
        pivot_x: row.get(11)?,
        pivot_y: row.get(12)?,
        frame_mode: row.get(13)?,
        preferred_frames: row.get(14)?,
        min_frames: row.get(15)?,
        max_frames: row.get(16)?,
        generation_prompt: row.get(17)?,
        negative_prompt: row.get(18)?,
        weapon_behavior: row.get(19)?,
        phases: Vec::new(),
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

fn phase_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnimationTemplatePhase> {
    Ok(AnimationTemplatePhase {
        id: row.get(0)?,
        template_id: row.get(1)?,
        position: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        frame_count: row.get(5)?,
        timing_weight: row.get(6)?,
        movement_offset_x: row.get(7)?,
        movement_offset_y: row.get(8)?,
        weapon_position: row.get(9)?,
        pose_reference_id: row.get(10)?,
    })
}

fn template_select() -> &'static str {
    r#"SELECT id, project_id, source_animation_id, name, intent, motion_description,
              direction, looping, fps, width, height, pivot_x, pivot_y, frame_mode,
              preferred_frames, min_frames, max_frames, generation_prompt,
              negative_prompt, weapon_behavior, created_at, updated_at
       FROM animation_templates"#
}

fn load_phases(
    connection: &rusqlite::Connection,
    template_id: &str,
) -> CommandResult<Vec<AnimationTemplatePhase>> {
    let mut statement = connection.prepare(
        r#"SELECT id, template_id, position, name, description, frame_count,
                  timing_weight, movement_offset_x, movement_offset_y,
                  weapon_position, pose_reference_id
           FROM animation_template_phases
           WHERE template_id=?1 ORDER BY position"#,
    )?;
    let rows = statement.query_map([template_id], phase_row)?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn load_template(connection: &rusqlite::Connection, id: &str) -> CommandResult<AnimationTemplate> {
    let mut template = connection
        .query_row(
            &format!("{} WHERE id=?1", template_select()),
            [id],
            template_row,
        )
        .optional()?
        .ok_or_else(|| {
            CommandError::new(
                "animation_template_not_found",
                "The animation template no longer exists",
            )
        })?;
    template.phases = load_phases(connection, id)?;
    Ok(template)
}

#[tauri::command]
pub fn list_animation_templates(
    project_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AnimationTemplate>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut statement = connection.prepare(&format!(
        "{} WHERE project_id=?1 ORDER BY updated_at DESC",
        template_select()
    ))?;
    let templates: Vec<_> = statement
        .query_map([project_id], template_row)?
        .filter_map(Result::ok)
        .collect();
    drop(statement);
    templates
        .into_iter()
        .map(|mut template| {
            template.phases = load_phases(&connection, &template.id)?;
            Ok(template)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_animation_template(
    animation_id: String,
    name: String,
    intent: String,
    motion_description: String,
    frame_mode: String,
    min_frames: u32,
    max_frames: u32,
    generation_prompt: String,
    negative_prompt: String,
    state: State<'_, AppState>,
) -> CommandResult<AnimationTemplate> {
    let name = name.trim();
    let intent = intent.trim();
    let motion_description = motion_description.trim();
    if name.is_empty() || intent.is_empty() || motion_description.is_empty() {
        return Err(CommandError::new(
            "invalid_animation_template",
            "Template name, intent, and motion description are required",
        ));
    }
    if !matches!(frame_mode.as_str(), "fixed" | "auto") {
        return Err(CommandError::new(
            "invalid_frame_mode",
            "Template frame mode must be Fixed or Auto",
        ));
    }
    let minimum = min_frames.clamp(1, 64);
    let maximum = max_frames.clamp(minimum, 64);
    let (project_id, animation_name, fps, looping, frames_json): (
        String,
        String,
        f64,
        bool,
        String,
    ) =
        {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection
            .query_row(
                "SELECT workspace_id, name, fps, looping, frames_json FROM animations WHERE id=?1",
                [&animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?
            .ok_or_else(|| {
                CommandError::new("animation_not_found", "The source animation no longer exists")
            })?
        };
    let frames: Vec<AnimationFrame> = serde_json::from_str(&frames_json).unwrap_or_default();
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add frames before saving an animation template",
        ));
    }
    let first_asset = get_asset(&state, &frames[0].asset_id)?;
    let source_frame_count = frames.len().clamp(1, 64) as u32;
    let preferred_frames = if frame_mode == "auto" {
        source_frame_count.clamp(minimum, maximum)
    } else {
        source_frame_count
    };
    let planning = GenerationOptions {
        quality: "custom".into(),
        width: first_asset.width,
        height: first_asset.height,
        frames: preferred_frames,
        fps: fps.round().clamp(1.0, 60.0) as u32,
        frame_mode: frame_mode.clone(),
        min_frames: minimum,
        max_frames: maximum,
        allow_interpolation: true,
        allow_auto_adjust: frame_mode == "auto",
    };
    let plan = build_motion_plan(motion_description, &planning)?;
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let template_prompt = if generation_prompt.trim().is_empty() {
        format!(
            "Reproduce the motion intent from {animation_name} while preserving the target asset's identity, palette, proportions, equipment, camera, and pixel density. {motion_description}"
        )
    } else {
        generation_prompt.trim().to_string()
    };
    let mut template = AnimationTemplate {
        id: id.clone(),
        project_id,
        source_animation_id: Some(animation_id),
        name: name.to_string(),
        intent: intent.to_string(),
        motion_description: motion_description.to_string(),
        direction: "side".into(),
        looping,
        fps,
        width: first_asset.width,
        height: first_asset.height,
        pivot_x: Some(0.5),
        pivot_y: Some(1.0),
        frame_mode,
        preferred_frames,
        min_frames: minimum,
        max_frames: maximum,
        generation_prompt: template_prompt,
        negative_prompt: negative_prompt.trim().to_string(),
        weapon_behavior: None,
        phases: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let mut connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let transaction = connection.transaction()?;
    transaction.execute(
        r#"INSERT INTO animation_templates(
            id, project_id, source_animation_id, name, intent, motion_description,
            direction, looping, fps, width, height, pivot_x, pivot_y, frame_mode,
            preferred_frames, min_frames, max_frames, generation_prompt, negative_prompt,
            created_at, updated_at
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)"#,
        params![
            template.id,
            template.project_id,
            template.source_animation_id,
            template.name,
            template.intent,
            template.motion_description,
            template.direction,
            template.looping,
            template.fps,
            template.width,
            template.height,
            template.pivot_x,
            template.pivot_y,
            template.frame_mode,
            template.preferred_frames,
            template.min_frames,
            template.max_frames,
            template.generation_prompt,
            template.negative_prompt,
            template.created_at,
            template.updated_at
        ],
    )?;
    for (position, phase) in plan.phases.into_iter().enumerate() {
        let template_phase = AnimationTemplatePhase {
            id: Uuid::new_v4().to_string(),
            template_id: id.clone(),
            position: position as u32,
            name: phase.name,
            description: phase.description,
            frame_count: phase.frame_count,
            timing_weight: phase.timing_weight,
            movement_offset_x: 0.0,
            movement_offset_y: 0.0,
            weapon_position: None,
            pose_reference_id: None,
        };
        transaction.execute(
            r#"INSERT INTO animation_template_phases(
                id, template_id, position, name, description, frame_count, timing_weight,
                movement_offset_x, movement_offset_y, weapon_position, pose_reference_id
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"#,
            params![
                template_phase.id,
                template_phase.template_id,
                template_phase.position,
                template_phase.name,
                template_phase.description,
                template_phase.frame_count,
                template_phase.timing_weight,
                template_phase.movement_offset_x,
                template_phase.movement_offset_y,
                template_phase.weapon_position,
                template_phase.pose_reference_id
            ],
        )?;
        template.phases.push(template_phase);
    }
    transaction.commit()?;
    Ok(template)
}

#[tauri::command]
pub fn apply_animation_template(
    template_id: String,
    target_asset_id: String,
    state: State<'_, AppState>,
) -> CommandResult<TemplateApplication> {
    let template = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        load_template(&connection, &template_id)?
    };
    let target_asset = get_asset(&state, &target_asset_id)?;
    if target_asset.workspace_id != template.project_id {
        return Err(CommandError::new(
            "incompatible_template_target",
            "The template and target asset must belong to the same project",
        ));
    }
    let generation = GenerationOptions {
        quality: "custom".into(),
        width: target_asset.width,
        height: target_asset.height,
        frames: template.preferred_frames,
        fps: template.fps.round().clamp(1.0, 60.0) as u32,
        frame_mode: template.frame_mode.clone(),
        min_frames: template.min_frames,
        max_frames: template.max_frames,
        allow_interpolation: true,
        allow_auto_adjust: template.frame_mode == "auto",
    };
    let motion_plan = build_motion_plan(&template.motion_description, &generation)?;
    let phases = template
        .phases
        .iter()
        .map(|phase| format!("{}: {}", phase.name, phase.description))
        .collect::<Vec<_>>()
        .join("; ");
    let prompt = format!(
        "/animate Apply the reusable motion template “{}” to the exact target master `{}`. Keep the target's appearance, palette, proportions, equipment, pixel density, and camera unchanged. Intent: {}. Motion: {}. Phase progression: {}. Use {} frames at {} FPS. {} Negative constraints: {}",
        template.name,
        target_asset.relative_path,
        template.intent,
        template.motion_description,
        phases,
        motion_plan.selected_frame_count,
        motion_plan.fps,
        template.generation_prompt,
        template.negative_prompt
    );
    Ok(TemplateApplication {
        template,
        target_asset,
        motion_plan,
        prompt,
    })
}

#[tauri::command]
pub fn delete_animation_template(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let changed = connection.execute("DELETE FROM animation_templates WHERE id=?1", [id])?;
    if changed == 0 {
        return Err(CommandError::new(
            "animation_template_not_found",
            "The animation template no longer exists",
        ));
    }
    Ok(())
}
