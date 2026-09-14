use crate::{
    assets::{get_asset, inspect, normalize_sprite_file, upsert},
    error::{CommandError, CommandResult},
    models::Animation,
    workspace::workspace_path,
    AppState,
};
use rusqlite::OptionalExtension;
use tauri::Manager;

fn parse_animation_frames(
    frames_json: &str,
) -> CommandResult<Vec<crate::models::AnimationFrame>> {
    serde_json::from_str(frames_json).map_err(|error| {
        CommandError::new(
            "invalid_frames_json",
            format!("Animation frames are corrupted: {error}"),
        )
    })
}

fn repair_transparency_inner(
    app: &tauri::AppHandle,
    state: &AppState,
    animation_id: &str,
) -> CommandResult<Animation> {
    let (project_id, worktree_id, name, fps, looping, frames_json, created_at, _updated_at): (
        String,
        Option<String>,
        String,
        f64,
        bool,
        String,
        String,
        String,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT workspace_id,worktree_id,name,fps,looping,frames_json,created_at,updated_at FROM animations WHERE id=?1",
                [animation_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                CommandError::new("animation_not_found", "The animation no longer exists")
            })?
    };
    let frames = parse_animation_frames(&frames_json)?;
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "There are no frames to repair",
        ));
    }
    let root = workspace_path(state, &project_id)?;
    let master_path = crate::assets::read_generation_manifest(&root)?
        .and_then(|manifest| manifest.source)
        .map(|relative| root.join(relative))
        .filter(|path| path.is_file());
    for frame in &frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let path = std::path::Path::new(&asset.path);
        normalize_sprite_file(path, master_path.as_deref())?;
        let refreshed = inspect(&project_id, &root, path, Some(asset.id.clone()))?;
        upsert(state, &refreshed, "transparency_repair")?;
        crate::allow_asset_file(Some(app), path)?;
    }
    let repaired_at = chrono::Utc::now().to_rfc3339();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            "UPDATE animations SET updated_at=?1 WHERE id=?2",
            (&repaired_at, animation_id),
        )?;
    }
    Ok(Animation {
        id: animation_id.to_string(),
        workspace_id: project_id,
        worktree_id,
        name,
        fps,
        looping,
        frames,
        motion_plan: None,
        review_status: "draft".to_string(),
        created_at,
        updated_at: repaired_at,
    })
}

#[tauri::command]
pub async fn repair_animation_transparency(
    animation_id: String,
    app: tauri::AppHandle,
) -> CommandResult<Animation> {
    let repair_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = repair_app.state::<AppState>();
        repair_transparency_inner(&repair_app, &state, &animation_id)
    })
    .await
    .map_err(|error| CommandError::new("repair_task_failed", error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::parse_animation_frames;

    #[test]
    fn rejects_corrupted_frames_json() {
        let error = parse_animation_frames("{bad-json")
            .expect_err("corrupt json should fail");
        assert_eq!(error.code, "invalid_frames_json");
    }
}
