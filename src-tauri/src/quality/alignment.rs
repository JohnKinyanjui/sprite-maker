use crate::{
    animations::resolve_animation_frames,
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{Animation, AnimationFrame},
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::RgbaImage;
use rusqlite::{params, OptionalExtension};
use tauri::Manager;
use uuid::Uuid;

use super::metrics::compute_metrics;

fn repair_alignment_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    animation_id: &str,
) -> CommandResult<Animation> {
    let (project_id, worktree_id, name, fps, looping, source_frames, created_at): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        Vec<AnimationFrame>,
        String,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let (project_id, worktree_id, name, fps, looping, frames_json, created_at): (
            String,
            Option<String>,
            String,
            f64,
            bool,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT workspace_id,worktree_id,name,fps,looping,frames_json,created_at FROM animations WHERE id=?1",
                [animation_id],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?;
        let source_frames = resolve_animation_frames(&connection, animation_id, &frames_json)?;
        (project_id, worktree_id, name, fps, looping, source_frames, created_at)
    };
    if source_frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to align",
        ));
    }
    let mut decoded = Vec::with_capacity(source_frames.len());
    let mut canvas_width = 0;
    let mut canvas_height = 0;
    for frame in &source_frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        canvas_width = canvas_width.max(image.width());
        canvas_height = canvas_height.max(image.height());
        let metrics = compute_metrics(&asset.id, &asset.path)?;
        decoded.push((frame, image, metrics.metrics.bounds));
    }
    let root = workspace_path(state, &project_id)?;
    let repair_id = Uuid::new_v4().to_string();
    let slug = name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    let output_directory = root.join("assets").join("repairs").join(format!(
        "{}-{}",
        if slug.is_empty() { "aligned" } else { slug },
        &repair_id[..8]
    ));
    std::fs::create_dir_all(&output_directory)?;
    crate::allow_asset_directory(Some(app), &output_directory, true)?;
    let mut repaired_frames = Vec::with_capacity(decoded.len());
    for (index, (source_frame, image, bounds)) in decoded.into_iter().enumerate() {
        let aligned = align_frame_to_canvas(&image, bounds, canvas_width, canvas_height);
        let path = output_directory.join(format!("aligned_{:02}.png", index + 1));
        aligned.save(&path)?;
        let asset = inspect(&project_id, &root, &path, None)?;
        upsert(state, &asset, "quality_alignment_repair")?;
        if let Some(worktree_id) = &worktree_id {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection.execute(
                "INSERT OR REPLACE INTO asset_worktrees(asset_id,worktree_id,relationship,created_at) VALUES (?1,?2,'owned',?3)",
                params![asset.id, worktree_id, Utc::now().to_rfc3339()],
            )?;
        }
        repaired_frames.push(AnimationFrame {
            asset_id: asset.id,
            duration_ms: source_frame.duration_ms,
            offset_x: 0,
            offset_y: 0,
        });
    }
    let now = Utc::now().to_rfc3339();
    let repaired = Animation {
        id: repair_id,
        workspace_id: project_id,
        worktree_id,
        name: format!("{name} — aligned"),
        fps,
        looping,
        frames: repaired_frames,
        motion_plan: None,
        review_status: "draft".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let repaired_json = serde_json::to_string(&repaired.frames)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO animations(id,workspace_id,worktree_id,name,fps,looping,frames_json,review_status,created_at,updated_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,'draft',?8,?8)"#,
            params![repaired.id,repaired.workspace_id,repaired.worktree_id,repaired.name,repaired.fps,repaired.looping,repaired_json,repaired.created_at],
        )?;
        transaction.execute(
            r#"INSERT INTO animation_revisions(
                id,animation_id,parent_animation_id,change_kind,summary,created_at
            ) VALUES (?1,?2,?3,'alignment_repair','Bottom-centered every opaque subject on a shared canvas without overwriting the source animation',?4)"#,
            params![Uuid::new_v4().to_string(),repaired.id,animation_id,repaired.created_at],
        )?;
        for (position, frame) in repaired.frames.iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(id,animation_id,asset_id,position,duration_ms,pivot_x,pivot_y,created_at)
                   VALUES (?1,?2,?3,?4,?5,0.5,1.0,?6)"#,
                params![Uuid::new_v4().to_string(),repaired.id,frame.asset_id,position as u32,frame.duration_ms,repaired.created_at],
            )?;
        }
        transaction.commit()?;
    }
    std::fs::write(
        root.join("animations")
            .join(format!("{}.json", repaired.id)),
        serde_json::to_vec_pretty(&repaired)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;
    let _ = created_at;
    Ok(repaired)
}

pub(crate) fn align_frame_to_canvas(
    image: &RgbaImage,
    bounds: Option<(u32, u32, u32, u32)>,
    canvas_width: u32,
    canvas_height: u32,
) -> RgbaImage {
    let mut aligned = RgbaImage::new(canvas_width, canvas_height);
    let Some((minimum_x, minimum_y, maximum_x, maximum_y)) = bounds else {
        let x_offset = canvas_width.saturating_sub(image.width()) / 2;
        let y_offset = canvas_height.saturating_sub(image.height()).saturating_sub(1);
        for y in 0..image.height() {
            for x in 0..image.width() {
                let target_x = x_offset + x;
                let target_y = y_offset + y;
                if target_x < canvas_width && target_y < canvas_height {
                    aligned.put_pixel(target_x, target_y, *image.get_pixel(x, y));
                }
            }
        }
        return aligned;
    };
    let subject_width = maximum_x - minimum_x + 1;
    let subject_height = maximum_y - minimum_y + 1;
    let destination_x = canvas_width.saturating_sub(subject_width) / 2;
    let destination_y = canvas_height
        .saturating_sub(subject_height)
        .saturating_sub(1);
    for source_y in minimum_y..=maximum_y {
        for source_x in minimum_x..=maximum_x {
            let target_x = destination_x + source_x - minimum_x;
            let target_y = destination_y + source_y - minimum_y;
            if target_x < canvas_width && target_y < canvas_height {
                aligned.put_pixel(target_x, target_y, *image.get_pixel(source_x, source_y));
            }
        }
    }
    aligned
}

#[tauri::command]
pub async fn repair_animation_alignment(
    animation_id: String,
    app: tauri::AppHandle,
) -> CommandResult<Animation> {
    let repair_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = repair_app.state::<AppState>();
        repair_alignment_inner(&repair_app, &state, &animation_id)
    })
    .await
    .map_err(|error| CommandError::new("repair_task_failed", error.to_string()))?
}
