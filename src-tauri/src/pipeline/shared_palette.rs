use crate::{
    animations::load_animation_by_id,
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{QuantizeWorktreePaletteInput, SharedPaletteReport},
    workspace::workspace_path,
    AppState,
};
use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use tauri::State;

fn resolve_palette_color_count(count: u32) -> CommandResult<u32> {
    match count {
        8 | 16 | 32 => Ok(count),
        _ => Err(CommandError::new(
            "invalid_color_count",
            "Palette color count must be 8, 16, or 32",
        )),
    }
}

fn color_distance(left: (u8, u8, u8), right: (u8, u8, u8)) -> u32 {
    let dr = left.0 as i32 - right.0 as i32;
    let dg = left.1 as i32 - right.1 as i32;
    let db = left.2 as i32 - right.2 as i32;
    (dr * dr + dg * dg + db * db) as u32
}

fn collect_opaque_colors(image: &RgbaImage) -> HashMap<(u8, u8, u8), u32> {
    let mut colors = HashMap::new();
    for pixel in image.pixels() {
        if pixel[3] == 0 {
            continue;
        }
        let key = (pixel[0], pixel[1], pixel[2]);
        colors.entry(key).and_modify(|count| *count += 1).or_insert(1);
    }
    colors
}

fn build_palette(colors: &HashMap<(u8, u8, u8), u32>, target: usize) -> Vec<(u8, u8, u8)> {
    if colors.is_empty() {
        return Vec::new();
    }
    let mut entries = colors
        .iter()
        .map(|(color, count)| (*color, *count))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.1.cmp(&left.1));
    if entries.len() <= target {
        return entries.into_iter().map(|(color, _)| color).collect();
    }
    let mut palette = entries
        .into_iter()
        .take(target)
        .map(|(color, _)| color)
        .collect::<Vec<_>>();
    while palette.len() > target {
        let mut best_pair = (0usize, 1usize, u32::MAX);
        for left in 0..palette.len() {
            for right in left + 1..palette.len() {
                let distance = color_distance(palette[left], palette[right]);
                if distance < best_pair.2 {
                    best_pair = (left, right, distance);
                }
            }
        }
        let merged = (
            ((palette[best_pair.0].0 as u16 + palette[best_pair.1].0 as u16) / 2) as u8,
            ((palette[best_pair.0].1 as u16 + palette[best_pair.1].1 as u16) / 2) as u8,
            ((palette[best_pair.0].2 as u16 + palette[best_pair.1].2 as u16) / 2) as u8,
        );
        palette[best_pair.0] = merged;
        palette.remove(best_pair.1);
    }
    palette
}

fn nearest_palette_color(color: (u8, u8, u8), palette: &[(u8, u8, u8)]) -> (u8, u8, u8) {
    palette
        .iter()
        .min_by_key(|entry| color_distance(color, **entry))
        .copied()
        .unwrap_or(color)
}

pub(crate) fn apply_shared_palette(image: &RgbaImage, palette: &[(u8, u8, u8)]) -> RgbaImage {
    if palette.is_empty() {
        return image.clone();
    }
    let mut output = image.clone();
    for pixel in output.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }
        let mapped = nearest_palette_color((pixel[0], pixel[1], pixel[2]), palette);
        pixel[0] = mapped.0;
        pixel[1] = mapped.1;
        pixel[2] = mapped.2;
    }
    output
}

fn rgb_to_hex(color: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

pub(crate) fn quantize_animation_palette_inner(
    state: &AppState,
    animation_id: &str,
    color_count: u32,
) -> CommandResult<SharedPaletteReport> {
    let color_count = resolve_palette_color_count(color_count)?;
    let animation = load_animation_by_id(state, animation_id)?;
    if animation.frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Animation has no frames to quantize",
        ));
    }
    let mut global_colors = HashMap::new();
    let mut images = Vec::new();
    for frame in &animation.frames {
        let asset = get_asset(state, &frame.asset_id)?;
        let image = image::open(&asset.path)?.to_rgba8();
        for (color, count) in collect_opaque_colors(&image) {
            global_colors
                .entry(color)
                .and_modify(|total| *total += count)
                .or_insert(count);
        }
        images.push((asset, image));
    }
    let palette = build_palette(&global_colors, color_count as usize);
    let unique_before = global_colors.len() as u32;
    let root = workspace_path(state, &animation.workspace_id)?;
    let mut frames_touched = 0u32;
    for (asset, image) in images {
        let quantized = apply_shared_palette(&image, &palette);
        quantized.save(&asset.path)?;
        let updated = inspect(
            &animation.workspace_id,
            &root,
            std::path::Path::new(&asset.path),
            Some(asset.id.clone()),
        )?;
        upsert(state, &updated, "palette_quantize")?;
        frames_touched += 1;
    }
    let unique_after = palette.len() as u32;
    Ok(SharedPaletteReport {
        unique_colors_before: unique_before,
        unique_colors_after: unique_after,
        frames_touched,
        palette: palette.iter().map(|color| rgb_to_hex(*color)).collect(),
    })
}

pub(crate) fn quantize_worktree_palette_inner(
    state: &AppState,
    input: &QuantizeWorktreePaletteInput,
) -> CommandResult<SharedPaletteReport> {
    let color_count = resolve_palette_color_count(input.color_count.unwrap_or(32))?;
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let animation_ids = if let Some(worktree_id) = input.worktree_id.as_deref() {
        let mut statement = connection.prepare(
            "SELECT id FROM animations WHERE workspace_id = ?1 AND worktree_id = ?2",
        )?;
        let rows = statement
            .query_map([&input.workspace_id, worktree_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    } else {
        let mut statement =
            connection.prepare("SELECT id FROM animations WHERE workspace_id = ?1")?;
        let rows = statement
            .query_map([&input.workspace_id], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    };
    if animation_ids.is_empty() {
        return Err(CommandError::new(
            "empty_worktree",
            "No animations were found for palette quantization",
        ));
    }
    let mut global_colors = HashMap::new();
    let mut asset_paths = Vec::new();
    for animation_id in animation_ids {
        let animation = load_animation_by_id(state, &animation_id)?;
        if let Some(slug) = input.anchor_slug.as_deref() {
            let matches = animation.name.contains(slug)
                || animation
                    .worktree_id
                    .as_deref()
                    .map(|_| animation.name.starts_with(&format!("{slug}-")))
                    .unwrap_or(false);
            if !matches {
                continue;
            }
        }
        for frame in animation.frames {
            let asset = get_asset(state, &frame.asset_id)?;
            let image = image::open(&asset.path)?.to_rgba8();
            for (color, count) in collect_opaque_colors(&image) {
                global_colors
                    .entry(color)
                    .and_modify(|total| *total += count)
                    .or_insert(count);
            }
            asset_paths.push((animation.workspace_id.clone(), asset, image));
        }
    }
    if asset_paths.is_empty() {
        return Err(CommandError::new(
            "empty_worktree",
            "No frame assets were found for palette quantization",
        ));
    }
    let palette = build_palette(&global_colors, color_count as usize);
    let unique_before = global_colors.len() as u32;
    let root = workspace_path(state, &input.workspace_id)?;
    let mut frames_touched = 0u32;
    for (workspace_id, asset, image) in asset_paths {
        let quantized = apply_shared_palette(&image, &palette);
        quantized.save(&asset.path)?;
        let updated = inspect(
            &workspace_id,
            &root,
            std::path::Path::new(&asset.path),
            Some(asset.id.clone()),
        )?;
        upsert(state, &updated, "palette_quantize")?;
        frames_touched += 1;
    }
    Ok(SharedPaletteReport {
        unique_colors_before: unique_before,
        unique_colors_after: palette.len() as u32,
        frames_touched,
        palette: palette.iter().map(|color| rgb_to_hex(*color)).collect(),
    })
}

#[tauri::command]
pub fn quantize_worktree_palette(
    input: QuantizeWorktreePaletteInput,
    state: State<'_, AppState>,
) -> CommandResult<SharedPaletteReport> {
    quantize_worktree_palette_inner(&state, &input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_shared_palette_preserves_transparent_pixels() {
        let mut image = RgbaImage::new(4, 4);
        image.put_pixel(0, 0, Rgba([200, 10, 10, 255]));
        image.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
        let palette = [(255, 0, 0)];
        let output = apply_shared_palette(&image, &palette);
        assert_eq!(output.get_pixel(1, 1)[3], 0);
        assert_eq!(output.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn rejects_invalid_palette_sizes() {
        let error = resolve_palette_color_count(64).expect_err("invalid");
        assert_eq!(error.code, "invalid_color_count");
    }

    #[test]
    fn quantize_animation_palette_reduces_unique_opaque_colors() {
        let root = std::env::temp_dir().join(format!("palette-anim-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("dir");
        let connection = crate::database::open(&root.join("app.sqlite3")).expect("db");
        let state = crate::AppState::from_connection(connection);
        let project = root.join("game");
        std::fs::create_dir_all(project.join("assets/characters")).expect("dirs");
        let workspace = crate::workspace::create_workspace_inner(
            "Game".into(),
            project.to_string_lossy().into_owned(),
            &state,
        )
        .expect("workspace");
        let ws_root = crate::workspace::workspace_path(&state, &workspace.id).expect("root");
        let mut frame_assets = Vec::new();
        for index in 0..12 {
            let path = ws_root.join(format!("assets/characters/frame-{:02}.png", index + 1));
            let mut image = RgbaImage::new(4, 4);
            image.put_pixel(0, 0, Rgba([(index * 17) as u8, 40, 60, 255]));
            image.save(&path).expect("save");
            let asset = inspect(&workspace.id, &ws_root, &path, None).expect("inspect");
            upsert(&state, &asset, "test").expect("upsert");
            frame_assets.push(asset);
        }
        let animation = crate::animations::save_animation_inner(
            crate::models::AnimationInput {
                id: None,
                workspace_id: workspace.id.clone(),
                worktree_id: None,
                name: "Palette".into(),
                fps: 8.0,
                looping: true,
                frames: frame_assets
                    .iter()
                    .map(|asset| crate::models::AnimationFrame {
                        asset_id: asset.id.clone(),
                        duration_ms: None,
                        offset_x: 0,
                        offset_y: 0,
                    })
                    .collect(),
                motion_plan: None,
                review_status: Some("draft".into()),
            },
            &state,
        )
        .expect("animation");

        let report =
            quantize_animation_palette_inner(&state, &animation.id, 8).expect("quantize");
        assert_eq!(report.frames_touched, 12);
        assert!(report.unique_colors_before > report.unique_colors_after);
        assert_eq!(report.unique_colors_after, 8);
        assert_eq!(report.palette.len(), 8);

        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quantize_worktree_palette_errors_on_empty_worktree() {
        let root = std::env::temp_dir().join(format!("palette-empty-{}", uuid::Uuid::new_v4()));
        let connection = crate::database::open(&root.join("app.sqlite3")).expect("db");
        let state = crate::AppState::from_connection(connection);
        let workspace = crate::workspace::create_workspace_inner(
            "Game".into(),
            root.join("game").to_string_lossy().into_owned(),
            &state,
        )
        .expect("workspace");
        let error = quantize_worktree_palette_inner(
            &state,
            &QuantizeWorktreePaletteInput {
                workspace_id: workspace.id,
                worktree_id: None,
                anchor_slug: None,
                color_count: Some(16),
            },
        )
        .expect_err("empty worktree");
        assert_eq!(error.code, "empty_worktree");
    }
}
