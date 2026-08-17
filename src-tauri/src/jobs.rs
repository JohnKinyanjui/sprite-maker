use crate::{
    assets::{get_asset, inspect, upsert},
    error::{CommandError, CommandResult},
    models::{
        AnimationFrame, BackgroundJob, JobEvent, ProceduralVfxInput, SpriteSheet, SpriteSheetInput,
        VfxEffect,
    },
    workspace::workspace_path,
    AppState,
};
use chrono::Utc;
use image::{imageops::FilterType, GenericImage, Rgba, RgbaImage};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

const MAX_SHEET_EDGE: u32 = 32_768;

fn job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackgroundJob> {
    Ok(BackgroundJob {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        kind: row.get(3)?,
        target_type: row.get(4)?,
        target_id: row.get(5)?,
        status: row.get(6)?,
        progress: row.get(7)?,
        stage: row.get(8)?,
        error_message: row.get(9)?,
        cancel_requested: row.get(10)?,
        result_path: row.get(11)?,
        created_at: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn select_job() -> &'static str {
    r#"SELECT id, project_id, worktree_id, kind, target_type, target_id, status,
              progress, stage, error_message, cancel_requested, result_path,
              created_at, started_at, completed_at, updated_at
       FROM background_jobs"#
}

fn sheet_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpriteSheet> {
    Ok(SpriteSheet {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        animation_id: row.get(3)?,
        name: row.get(4)?,
        layout: row.get(5)?,
        frame_width: row.get(6)?,
        frame_height: row.get(7)?,
        padding: row.get(8)?,
        spacing: row.get(9)?,
        rows: row.get(10)?,
        columns: row.get(11)?,
        scale: row.get(12)?,
        transparent: row.get(13)?,
        alignment: row.get(14)?,
        pivot_x: row.get(15)?,
        pivot_y: row.get(16)?,
        png_path: row.get(17)?,
        metadata_path: row.get(18)?,
        width: row.get(19)?,
        height: row.get(20)?,
        frame_count: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}

fn select_sheet() -> &'static str {
    r#"SELECT id, project_id, worktree_id, animation_id, name, layout,
              frame_width, frame_height, padding, spacing, rows, columns, scale,
              transparent, alignment, pivot_x, pivot_y, png_path, metadata_path,
              width, height, frame_count, created_at, updated_at
       FROM sprite_sheets"#
}

pub(crate) fn load_job(state: &AppState, id: &str) -> CommandResult<BackgroundJob> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection
        .query_row(&format!("{} WHERE id=?1", select_job()), [id], job_row)
        .optional()?
        .ok_or_else(|| CommandError::new("job_not_found", "The background job no longer exists"))
}

fn emit_job(app: &tauri::AppHandle, state: &AppState, id: &str) -> CommandResult<BackgroundJob> {
    let job = load_job(state, id)?;
    app.emit("job-event", JobEvent { job: job.clone() })
        .map_err(|error| CommandError::new("event_error", error.to_string()))?;
    Ok(job)
}

pub(crate) struct JobProgress<'a> {
    pub(crate) status: &'a str,
    pub(crate) progress: f64,
    pub(crate) stage: &'a str,
    pub(crate) error_message: Option<&'a str>,
    pub(crate) result_path: Option<&'a str>,
}

pub(crate) fn set_job_state(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    update: JobProgress<'_>,
) -> CommandResult<BackgroundJob> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"UPDATE background_jobs
           SET status=?2, progress=?3, stage=?4, error_message=?5,
               result_path=COALESCE(?6, result_path),
               started_at=CASE WHEN ?2='running' AND started_at IS NULL THEN ?7 ELSE started_at END,
               completed_at=CASE WHEN ?2 IN ('completed','failed','cancelled') THEN ?7 ELSE completed_at END,
               updated_at=?7
           WHERE id=?1"#,
        params![
            id,
            update.status,
            update.progress.clamp(0.0, 1.0),
            update.stage,
            update.error_message,
            update.result_path,
            now
        ],
    )?;
    drop(connection);
    emit_job(app, state, id)
}

pub(crate) fn cancellation_requested(state: &AppState, id: &str) -> CommandResult<bool> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    Ok(connection.query_row(
        "SELECT cancel_requested FROM background_jobs WHERE id=?1",
        [id],
        |row| row.get(0),
    )?)
}

fn validate_input(input: &SpriteSheetInput) -> CommandResult<()> {
    if input.name.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_sheet_name",
            "Sprite-sheet name cannot be empty",
        ));
    }
    if !matches!(input.layout.as_str(), "horizontal" | "vertical" | "grid") {
        return Err(CommandError::new(
            "invalid_sheet_layout",
            "Choose Horizontal, Vertical, or Grid layout",
        ));
    }
    if input.frame_width == 0 || input.frame_height == 0 {
        return Err(CommandError::new(
            "invalid_frame_size",
            "Frame width and height must be greater than zero",
        ));
    }
    if input.frame_width > 4096 || input.frame_height > 4096 {
        return Err(CommandError::new(
            "invalid_frame_size",
            "Frame width and height must be 4096 pixels or smaller",
        ));
    }
    if !(1..=8).contains(&input.scale) {
        return Err(CommandError::new(
            "invalid_export_scale",
            "Export scale must be between 1× and 8×",
        ));
    }
    if !matches!(
        input.alignment.as_str(),
        "top_left" | "center" | "bottom_center"
    ) {
        return Err(CommandError::new(
            "invalid_alignment",
            "Choose Top left, Center, or Bottom center alignment",
        ));
    }
    Ok(())
}

fn layout_dimensions(layout: &str, frame_count: u32, requested_columns: u32) -> (u32, u32) {
    match layout {
        "horizontal" => (frame_count, 1),
        "vertical" => (1, frame_count),
        _ => {
            let columns = requested_columns.clamp(1, frame_count.max(1));
            let rows = frame_count.div_ceil(columns);
            (columns, rows)
        }
    }
}

fn checked_sheet_edge(
    cells: u32,
    frame_edge: u32,
    padding: u32,
    spacing: u32,
    scale: u32,
) -> CommandResult<u32> {
    let base = frame_edge
        .checked_mul(cells)
        .and_then(|value| value.checked_add(spacing.saturating_mul(cells.saturating_sub(1))))
        .and_then(|value| value.checked_add(padding.saturating_mul(2)))
        .ok_or_else(|| CommandError::new("sheet_too_large", "Sprite-sheet dimensions overflow"))?;
    let edge = base
        .checked_mul(scale)
        .ok_or_else(|| CommandError::new("sheet_too_large", "Sprite-sheet dimensions overflow"))?;
    if edge > MAX_SHEET_EDGE {
        return Err(CommandError::new(
            "sheet_too_large",
            format!("Sprite-sheet edge {edge}px exceeds the {MAX_SHEET_EDGE}px limit"),
        ));
    }
    Ok(edge)
}

fn portable_slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "sprite-sheet".into()
    } else {
        slug.into()
    }
}

fn fit_image(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.width() <= width && image.height() <= height {
        return image;
    }
    let ratio = (width as f64 / image.width() as f64).min(height as f64 / image.height() as f64);
    let target_width = ((image.width() as f64 * ratio).floor() as u32).max(1);
    let target_height = ((image.height() as f64 * ratio).floor() as u32).max(1);
    image::imageops::resize(&image, target_width, target_height, FilterType::Nearest)
}

fn frame_offset(
    alignment: &str,
    cell_width: u32,
    cell_height: u32,
    image_width: u32,
    image_height: u32,
) -> (u32, u32) {
    match alignment {
        "top_left" => (0, 0),
        "center" => (
            cell_width.saturating_sub(image_width) / 2,
            cell_height.saturating_sub(image_height) / 2,
        ),
        _ => (
            cell_width.saturating_sub(image_width) / 2,
            cell_height.saturating_sub(image_height),
        ),
    }
}

fn vfx_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VfxEffect> {
    Ok(VfxEffect {
        id: row.get(0)?,
        project_id: row.get(1)?,
        worktree_id: row.get(2)?,
        animation_id: row.get(3)?,
        name: row.get(4)?,
        effect_type: row.get(5)?,
        blend_mode: row.get(6)?,
        center_x: row.get(7)?,
        center_y: row.get(8)?,
        opacity: row.get(9)?,
        looping: row.get(10)?,
        fps: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn select_vfx() -> &'static str {
    r#"SELECT id, project_id, worktree_id, animation_id, name, effect_type,
              blend_mode, center_x, center_y, opacity, looping, fps, created_at, updated_at
       FROM vfx_effects"#
}

fn validate_vfx_input(input: &ProceduralVfxInput) -> CommandResult<()> {
    if input.name.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_vfx_name",
            "Effect name cannot be empty",
        ));
    }
    if !matches!(
        input.effect_type.as_str(),
        "fire"
            | "explosion"
            | "magic"
            | "slash"
            | "smoke"
            | "frost_lance"
            | "storm_lance"
            | "nova_beam"
            | "voltaic_snare"
    ) {
        return Err(CommandError::new(
            "invalid_vfx_type",
            "Choose a base effect or one of the experimental ability effects",
        ));
    }
    if !matches!(
        input.blend_mode.as_str(),
        "normal" | "add" | "screen" | "multiply"
    ) {
        return Err(CommandError::new(
            "invalid_blend_mode",
            "Choose Normal, Add, Screen, or Multiply blending",
        ));
    }
    if !(8..=1024).contains(&input.width) || !(8..=1024).contains(&input.height) {
        return Err(CommandError::new(
            "invalid_vfx_size",
            "VFX dimensions must be between 8 and 1024 pixels",
        ));
    }
    if !(2..=64).contains(&input.frames) {
        return Err(CommandError::new(
            "invalid_vfx_frames",
            "Procedural VFX must contain between 2 and 64 frames",
        ));
    }
    if !(1..=60).contains(&input.fps) {
        return Err(CommandError::new(
            "invalid_vfx_fps",
            "VFX playback must be between 1 and 60 FPS",
        ));
    }
    Ok(())
}

fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
        return;
    }
    let destination = image.get_pixel_mut(x as u32, y as u32);
    let source_alpha = color[3] as f32 / 255.0;
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let value = (color[channel] as f32 * source_alpha
            + destination[channel] as f32 * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn draw_disc(image: &mut RgbaImage, center_x: f64, center_y: f64, radius: f64, color: Rgba<u8>) {
    let minimum_x = (center_x - radius).floor() as i32;
    let maximum_x = (center_x + radius).ceil() as i32;
    let minimum_y = (center_y - radius).floor() as i32;
    let maximum_y = (center_y + radius).ceil() as i32;
    for y in minimum_y..=maximum_y {
        for x in minimum_x..=maximum_x {
            let distance = ((x as f64 - center_x).powi(2) + (y as f64 - center_y).powi(2)).sqrt();
            if distance <= radius {
                let edge = ((radius - distance) / radius.max(1.0)).clamp(0.0, 1.0);
                let mut shaded = color;
                shaded[3] = (color[3] as f64 * edge.sqrt()).round() as u8;
                blend_pixel(image, x, y, shaded);
            }
        }
    }
}

fn draw_ring(
    image: &mut RgbaImage,
    center_x: f64,
    center_y: f64,
    radius: f64,
    thickness: f64,
    color: Rgba<u8>,
) {
    let outer = radius + thickness;
    let minimum_x = (center_x - outer).floor() as i32;
    let maximum_x = (center_x + outer).ceil() as i32;
    let minimum_y = (center_y - outer).floor() as i32;
    let maximum_y = (center_y + outer).ceil() as i32;
    for y in minimum_y..=maximum_y {
        for x in minimum_x..=maximum_x {
            let distance = ((x as f64 - center_x).powi(2) + (y as f64 - center_y).powi(2)).sqrt();
            let delta = (distance - radius).abs();
            if delta <= thickness {
                let mut shaded = color;
                shaded[3] = (color[3] as f64 * (1.0 - delta / thickness.max(0.5))).round() as u8;
                blend_pixel(image, x, y, shaded);
            }
        }
    }
}

/// A soft, layered sprite stroke. It lets the procedural experiments retain a
/// readable direction of force at tiny game resolutions.
fn draw_stroke(
    image: &mut RgbaImage,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    radius: f64,
    color: Rgba<u8>,
) {
    let distance = (to_x - from_x).hypot(to_y - from_y);
    let steps = distance.ceil().max(1.0) as u32;
    for step in 0..=steps {
        let u = step as f64 / steps as f64;
        draw_disc(
            image,
            from_x + (to_x - from_x) * u,
            from_y + (to_y - from_y) * u,
            radius,
            color,
        );
    }
}

fn pseudo(seed: u64, value: u64) -> f64 {
    let mut number = seed ^ value.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    number ^= number >> 30;
    number = number.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    number ^= number >> 27;
    number = number.wrapping_mul(0x94D0_49BB_1331_11EB);
    ((number ^ (number >> 31)) as f64) / u64::MAX as f64
}

fn render_vfx_frame(input: &ProceduralVfxInput, frame_index: u32) -> RgbaImage {
    let mut image = RgbaImage::new(input.width, input.height);
    let denominator = if input.looping {
        input.frames as f64
    } else {
        input.frames.saturating_sub(1).max(1) as f64
    };
    let t = frame_index as f64 / denominator;
    let width = input.width as f64;
    let height = input.height as f64;
    let center_x = width * 0.5;
    let center_y = height * 0.54;
    match input.effect_type.as_str() {
        "frost_lance" => {
            // A travelling freeze front: narrow at the caster, opening into an
            // icy impact cluster. Each shard is a separate layer, not a flat
            // blue explosion, which keeps the effect legible at sprite scale.
            let travel = t.powf(0.68);
            let front_x = width * (0.18 + travel * 0.62);
            let ground_y = height * 0.70;
            let glow = (std::f64::consts::PI * t).sin().max(0.0);
            draw_stroke(
                &mut image,
                width * 0.14,
                ground_y,
                front_x,
                ground_y,
                1.1,
                Rgba([97, 207, 255, (150.0 * glow) as u8]),
            );
            for shard in 0..9 {
                let spread = (pseudo(input.seed, shard) - 0.5) * height * (0.10 + travel * 0.35);
                let x = front_x + (pseudo(input.seed + 5, shard) - 0.35) * width * 0.15;
                let y = ground_y + spread;
                let size = width.min(height)
                    * (0.025 + travel * 0.055)
                    * (0.65 + pseudo(input.seed + 9, shard));
                draw_disc(
                    &mut image,
                    x,
                    y,
                    size * 1.8,
                    Rgba([75, 168, 255, (105.0 * glow) as u8]),
                );
                draw_disc(
                    &mut image,
                    x,
                    y - size * 0.22,
                    size * 0.8,
                    Rgba([216, 250, 255, (235.0 * glow) as u8]),
                );
            }
            draw_ring(
                &mut image,
                front_x,
                ground_y,
                width.min(height) * (0.04 + travel * 0.14),
                1.2,
                Rgba([196, 247, 255, (210.0 * (1.0 - t * 0.45)) as u8]),
            );
        }
        "storm_lance" => {
            // Three offset filaments make this read as electricity rather than
            // as a generic beam. The target remains the brightest focal point.
            let travel = t.powf(0.55);
            let end_x = width * (0.16 + travel * 0.68);
            let start_y = height * 0.58;
            for strand in 0..3 {
                let lane = (strand as f64 - 1.0) * 2.4;
                let mut last_x = width * 0.14;
                let mut last_y = start_y + lane;
                for node in 1..10 {
                    let u = node as f64 / 9.0;
                    let x = width * 0.14 + (end_x - width * 0.14) * u;
                    let jitter = (pseudo(input.seed + strand, node) - 0.5)
                        * height
                        * 0.14
                        * (1.0 - u * 0.35);
                    let y = start_y + lane + jitter;
                    draw_stroke(
                        &mut image,
                        last_x,
                        last_y,
                        x,
                        y,
                        if strand == 1 { 1.45 } else { 0.8 },
                        Rgba([157, 104, 255, (125.0 + 90.0 * travel) as u8]),
                    );
                    draw_stroke(
                        &mut image,
                        last_x,
                        last_y,
                        x,
                        y,
                        0.55,
                        Rgba([235, 250, 255, (160.0 + 80.0 * travel) as u8]),
                    );
                    last_x = x;
                    last_y = y;
                }
            }
            let impact = width.min(height) * (0.035 + travel * 0.12);
            draw_disc(
                &mut image,
                end_x,
                start_y,
                impact * 1.9,
                Rgba([105, 71, 255, (125.0 * travel) as u8]),
            );
            draw_disc(
                &mut image,
                end_x,
                start_y,
                impact * 0.62,
                Rgba([244, 253, 255, (245.0 * travel) as u8]),
            );
        }
        "nova_beam" => {
            // Charge, release, sustained column, then collapse. The three
            // strokes are the sprite equivalent of core / shell / halo passes.
            let charge = (t / 0.22).clamp(0.0, 1.0);
            let release = ((t - 0.18) / 0.16).clamp(0.0, 1.0);
            let fade = ((1.0 - t) / 0.20).clamp(0.0, 1.0);
            let end_x = width * (0.22 + 0.62 * release);
            let y = height * 0.56;
            let orb = width.min(height) * (0.035 + charge * 0.11) * fade.max(0.45);
            let charge_visibility = charge.max(0.08);
            draw_disc(
                &mut image,
                width * 0.19,
                y,
                orb * 1.8,
                Rgba([68, 210, 255, (95.0 * charge_visibility) as u8]),
            );
            draw_disc(
                &mut image,
                width * 0.19,
                y,
                orb * 0.72,
                Rgba([255, 246, 178, (240.0 * charge_visibility) as u8]),
            );
            // Intake motes keep even the quiet charge frames visibly alive.
            for mote in 0..4 {
                let angle = std::f64::consts::TAU * (mote as f64 / 4.0 - t * 1.7);
                draw_disc(
                    &mut image,
                    width * 0.19 + angle.cos() * orb * 1.65,
                    y + angle.sin() * orb * 1.05,
                    0.8,
                    Rgba([255, 223, 117, (110.0 * charge_visibility) as u8]),
                );
            }
            if release > 0.0 {
                draw_stroke(
                    &mut image,
                    width * 0.19,
                    y,
                    end_x,
                    y,
                    orb * 1.15,
                    Rgba([49, 192, 255, (90.0 * fade) as u8]),
                );
                draw_stroke(
                    &mut image,
                    width * 0.19,
                    y,
                    end_x,
                    y,
                    orb * 0.57,
                    Rgba([104, 239, 255, (150.0 * fade) as u8]),
                );
                draw_stroke(
                    &mut image,
                    width * 0.19,
                    y,
                    end_x,
                    y,
                    orb * 0.20,
                    Rgba([255, 252, 216, (250.0 * fade) as u8]),
                );
                for ring in 0..4 {
                    draw_ring(
                        &mut image,
                        width * (0.22 + release * (0.14 + ring as f64 * 0.13)),
                        y,
                        orb * (0.6 + ring as f64 * 0.18),
                        0.8,
                        Rgba([255, 207, 89, (150.0 * fade) as u8]),
                    );
                }
            }
        }
        "voltaic_snare" => {
            // A measured zone: boundary first, an outward snap, then a charged
            // central pillar and orbiting sparks.
            let grow = (t / 0.26).clamp(0.0, 1.0);
            let hold = (std::f64::consts::PI * t).sin().max(0.25);
            let radius = width.min(height) * (0.12 + 0.24 * grow);
            draw_disc(
                &mut image,
                center_x,
                height * 0.62,
                radius,
                Rgba([92, 39, 194, (62.0 * hold) as u8]),
            );
            draw_ring(
                &mut image,
                center_x,
                height * 0.62,
                radius,
                1.8,
                Rgba([185, 86, 255, (235.0 * hold) as u8]),
            );
            draw_ring(
                &mut image,
                center_x,
                height * 0.62,
                radius * (1.14 - grow * 0.14),
                0.8,
                Rgba([231, 207, 255, (160.0 * (1.0 - grow * 0.5)) as u8]),
            );
            draw_stroke(
                &mut image,
                center_x,
                height * 0.62,
                center_x,
                height * (0.62 - 0.32 * grow),
                radius * 0.42,
                Rgba([115, 50, 255, (120.0 * hold) as u8]),
            );
            draw_stroke(
                &mut image,
                center_x,
                height * 0.62,
                center_x,
                height * (0.62 - 0.32 * grow),
                radius * 0.15,
                Rgba([241, 228, 255, (210.0 * hold) as u8]),
            );
            for spark in 0..7 {
                let angle = std::f64::consts::TAU * (spark as f64 / 7.0 + t * 1.4);
                draw_disc(
                    &mut image,
                    center_x + angle.cos() * radius,
                    height * 0.62 + angle.sin() * radius * 0.48,
                    1.2,
                    Rgba([247, 236, 255, (220.0 * hold) as u8]),
                );
            }
        }
        "explosion" => {
            let envelope = (std::f64::consts::PI * t).sin().max(0.0);
            let radius = width.min(height) * (0.08 + 0.34 * t);
            draw_disc(
                &mut image,
                center_x,
                center_y,
                radius,
                Rgba([255, 91, 27, (210.0 * envelope) as u8]),
            );
            draw_disc(
                &mut image,
                center_x,
                center_y,
                radius * 0.58,
                Rgba([255, 211, 73, (245.0 * envelope) as u8]),
            );
            draw_ring(
                &mut image,
                center_x,
                center_y,
                radius * 1.12,
                1.4,
                Rgba([255, 237, 154, (230.0 * (1.0 - t)) as u8]),
            );
            for particle in 0..10 {
                let angle = pseudo(input.seed, particle) * std::f64::consts::TAU;
                let distance = radius * (0.7 + pseudo(input.seed + 11, particle) * 0.9);
                draw_disc(
                    &mut image,
                    center_x + angle.cos() * distance,
                    center_y + angle.sin() * distance,
                    1.0 + 2.0 * (1.0 - t),
                    Rgba([255, 137, 35, (220.0 * envelope) as u8]),
                );
            }
        }
        "magic" => {
            let pulse = 0.5 + 0.5 * (std::f64::consts::TAU * t).sin();
            let radius = width.min(height) * (0.20 + 0.08 * pulse);
            draw_disc(
                &mut image,
                center_x,
                center_y,
                radius * 0.45,
                Rgba([110, 67, 255, 110]),
            );
            draw_ring(
                &mut image,
                center_x,
                center_y,
                radius,
                1.6,
                Rgba([121, 238, 255, 240]),
            );
            draw_ring(
                &mut image,
                center_x,
                center_y,
                radius * 0.72,
                1.0,
                Rgba([202, 126, 255, 210]),
            );
            for spark in 0..8 {
                let angle = std::f64::consts::TAU * (spark as f64 / 8.0 + t);
                draw_disc(
                    &mut image,
                    center_x + angle.cos() * radius * 1.25,
                    center_y + angle.sin() * radius * 1.25,
                    1.2,
                    Rgba([229, 251, 255, 230]),
                );
            }
            let marker_angle = std::f64::consts::TAU * t + 0.33;
            draw_disc(
                &mut image,
                center_x + marker_angle.cos() * radius * 1.55,
                center_y + marker_angle.sin() * radius * 1.55,
                1.0,
                Rgba([255, 255, 255, 255]),
            );
        }
        "slash" => {
            let radius = width.min(height) * 0.34;
            let head = -2.2 + 4.0 * t;
            for segment in 0..24 {
                let age = segment as f64 / 23.0;
                let angle = head - age * 1.25;
                let alpha =
                    (255.0 * (1.0 - age) * (std::f64::consts::PI * t).sin().max(0.15)) as u8;
                let x = center_x + angle.cos() * radius;
                let y = center_y + angle.sin() * radius * 0.72;
                draw_disc(
                    &mut image,
                    x,
                    y,
                    1.2 + 2.4 * (1.0 - age),
                    Rgba([173, 244, 255, alpha]),
                );
            }
            draw_disc(
                &mut image,
                center_x,
                center_y,
                2.0,
                Rgba([255, 255, 255, 120]),
            );
        }
        "smoke" => {
            let fade = (1.0 - t).max(0.0);
            for cloud in 0..7 {
                let phase = (t + pseudo(input.seed, cloud)) % 1.0;
                let x = center_x + (pseudo(input.seed + 17, cloud) - 0.5) * width * 0.25;
                let y = height * 0.78 - phase * height * 0.48;
                let radius = width.min(height) * (0.05 + phase * 0.10);
                draw_disc(
                    &mut image,
                    x,
                    y,
                    radius,
                    Rgba([157, 169, 183, (150.0 * fade.max(0.3)) as u8]),
                );
            }
        }
        _ => {
            for flame in 0..8 {
                let phase = (t + pseudo(input.seed, flame)) % 1.0;
                let x = center_x + (pseudo(input.seed + 31, flame) - 0.5) * width * 0.28;
                let y = height * 0.78 - phase * height * 0.42;
                let radius = width.min(height) * (0.035 + (1.0 - phase) * 0.065);
                draw_disc(
                    &mut image,
                    x,
                    y,
                    radius * 1.25,
                    Rgba([255, 72, 18, (210.0 * (1.0 - phase)) as u8]),
                );
                draw_disc(
                    &mut image,
                    x,
                    y + radius * 0.2,
                    radius * 0.7,
                    Rgba([255, 221, 69, (240.0 * (1.0 - phase)) as u8]),
                );
            }
        }
    }
    image
}

fn render_sprite_sheet(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    input: SpriteSheetInput,
) -> CommandResult<SpriteSheet> {
    validate_input(&input)?;
    let (animation_name, fps, looping, frames_json, animation_project, animation_worktree): (
        String,
        f64,
        bool,
        String,
        String,
        Option<String>,
    ) = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT name, fps, looping, frames_json, workspace_id, worktree_id FROM animations WHERE id=?1",
                [&input.animation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .optional()?
            .ok_or_else(|| CommandError::new("animation_not_found", "The animation no longer exists"))?
    };
    if animation_project != input.project_id {
        return Err(CommandError::new(
            "invalid_sheet_project",
            "The animation and sprite sheet must belong to the same project",
        ));
    }
    if let Some(worktree_id) = &input.worktree_id {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let valid: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2)",
            params![worktree_id, input.project_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(CommandError::new(
                "invalid_sheet_worktree",
                "The sprite-sheet worktree must belong to the same project",
            ));
        }
    }
    let frames: Vec<AnimationFrame> = serde_json::from_str(&frames_json).unwrap_or_default();
    if frames.is_empty() {
        return Err(CommandError::new(
            "empty_animation",
            "Add at least one frame before building a sprite sheet",
        ));
    }
    let frame_count = frames.len() as u32;
    let (columns, rows) = layout_dimensions(&input.layout, frame_count, input.columns);
    let output_width = checked_sheet_edge(
        columns,
        input.frame_width,
        input.padding,
        input.spacing,
        input.scale,
    )?;
    let output_height = checked_sheet_edge(
        rows,
        input.frame_height,
        input.padding,
        input.spacing,
        input.scale,
    )?;
    let base_width = output_width / input.scale;
    let base_height = output_height / input.scale;
    let background = if input.transparent {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([0, 0, 0, 255])
    };
    let mut canvas = RgbaImage::from_pixel(base_width, base_height, background);
    let mut item_metadata = Vec::with_capacity(frames.len());

    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.05,
            stage: "Loading frames",
            error_message: None,
            result_path: None,
        },
    )?;
    for (index, frame) in frames.iter().enumerate() {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Sprite-sheet build cancelled",
            ));
        }
        let asset = get_asset(state, &frame.asset_id)?;
        let image = fit_image(
            image::open(&asset.path)?.to_rgba8(),
            input.frame_width,
            input.frame_height,
        );
        let column = index as u32 % columns;
        let row = index as u32 / columns;
        let cell_x = input.padding + column * (input.frame_width + input.spacing);
        let cell_y = input.padding + row * (input.frame_height + input.spacing);
        let (offset_x, offset_y) = frame_offset(
            &input.alignment,
            input.frame_width,
            input.frame_height,
            image.width(),
            image.height(),
        );
        canvas.copy_from(&image, cell_x + offset_x, cell_y + offset_y)?;
        let duration = frame
            .duration_ms
            .unwrap_or_else(|| (1000.0 / fps.max(1.0)).round() as u32)
            .max(1);
        item_metadata.push(serde_json::json!({
            "index": index,
            "assetId": asset.id,
            "source": asset.relative_path,
            "row": row,
            "column": column,
            "x": cell_x * input.scale,
            "y": cell_y * input.scale,
            "width": input.frame_width * input.scale,
            "height": input.frame_height * input.scale,
            "durationMs": duration,
            "pivot": { "x": input.pivot_x, "y": input.pivot_y }
        }));
        let progress = 0.08 + 0.72 * ((index + 1) as f64 / frames.len() as f64);
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "running",
                progress,
                stage: &format!("Packing frame {} of {}", index + 1, frames.len()),
                error_message: None,
                result_path: None,
            },
        )?;
    }

    if cancellation_requested(state, job_id)? {
        return Err(CommandError::new(
            "job_cancelled",
            "Sprite-sheet build cancelled",
        ));
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.84,
            stage: "Scaling with nearest neighbour",
            error_message: None,
            result_path: None,
        },
    )?;
    let output = if input.scale == 1 {
        canvas
    } else {
        image::imageops::resize(&canvas, output_width, output_height, FilterType::Nearest)
    };
    let sheet_id = Uuid::new_v4().to_string();
    let root = workspace_path(state, &input.project_id)?;
    let output_directory = root.join("exports").join("sprite-sheets");
    std::fs::create_dir_all(&output_directory)?;
    let slug = portable_slug(input.name.trim());
    let revision = &sheet_id[..8];
    let png_path = output_directory.join(format!("{slug}-{revision}.png"));
    let metadata_path = output_directory.join(format!("{slug}-{revision}.json"));
    output.save(&png_path)?;

    let metadata = serde_json::json!({
        "name": input.name.trim(),
        "sourceAnimation": { "id": input.animation_id, "name": animation_name },
        "image": png_path.file_name().and_then(|value| value.to_str()).unwrap_or("sprite-sheet.png"),
        "layout": input.layout,
        "frameWidth": input.frame_width * input.scale,
        "frameHeight": input.frame_height * input.scale,
        "frameCount": frame_count,
        "rows": rows,
        "columns": columns,
        "padding": input.padding * input.scale,
        "spacing": input.spacing * input.scale,
        "scale": input.scale,
        "transparent": input.transparent,
        "alignment": input.alignment,
        "pivot": { "x": input.pivot_x, "y": input.pivot_y },
        "fps": fps,
        "loop": looping,
        "frames": item_metadata
    });
    std::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| CommandError::new("serialization_error", error.to_string()))?,
    )?;

    let now = Utc::now().to_rfc3339();
    let sheet = SpriteSheet {
        id: sheet_id,
        project_id: input.project_id,
        worktree_id: input.worktree_id.or(animation_worktree),
        animation_id: input.animation_id,
        name: input.name.trim().to_string(),
        layout: input.layout,
        frame_width: input.frame_width,
        frame_height: input.frame_height,
        padding: input.padding,
        spacing: input.spacing,
        rows,
        columns,
        scale: input.scale,
        transparent: input.transparent,
        alignment: input.alignment,
        pivot_x: input.pivot_x.clamp(0.0, 1.0),
        pivot_y: input.pivot_y.clamp(0.0, 1.0),
        png_path: png_path.to_string_lossy().into_owned(),
        metadata_path: metadata_path.to_string_lossy().into_owned(),
        width: output_width,
        height: output_height,
        frame_count,
        created_at: now.clone(),
        updated_at: now,
    };
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO sprite_sheets(
                id, project_id, worktree_id, animation_id, job_id, name, layout,
                frame_width, frame_height, padding, spacing, rows, columns, scale,
                transparent, alignment, pivot_x, pivot_y, png_path, metadata_path,
                width, height, frame_count, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)"#,
            params![
                sheet.id, sheet.project_id, sheet.worktree_id, sheet.animation_id,
                job_id, sheet.name, sheet.layout, sheet.frame_width, sheet.frame_height,
                sheet.padding, sheet.spacing, sheet.rows, sheet.columns, sheet.scale,
                sheet.transparent, sheet.alignment, sheet.pivot_x, sheet.pivot_y,
                sheet.png_path, sheet.metadata_path, sheet.width, sheet.height,
                sheet.frame_count, sheet.created_at, sheet.updated_at
            ],
        )?;
        for (index, frame) in frames.iter().enumerate() {
            let column = index as u32 % columns;
            let row = index as u32 / columns;
            let duration = frame
                .duration_ms
                .unwrap_or_else(|| (1000.0 / fps.max(1.0)).round() as u32)
                .max(1);
            transaction.execute(
                r#"INSERT INTO sprite_sheet_items(
                    id, sprite_sheet_id, animation_id, asset_id, position,
                    row_index, column_index, x, y, width, height, duration_ms
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
                params![
                    Uuid::new_v4().to_string(),
                    sheet.id,
                    sheet.animation_id,
                    frame.asset_id,
                    index as u32,
                    row,
                    column,
                    (input.padding + column * (input.frame_width + input.spacing)) * input.scale,
                    (input.padding + row * (input.frame_height + input.spacing)) * input.scale,
                    input.frame_width * input.scale,
                    input.frame_height * input.scale,
                    duration
                ],
            )?;
        }
        transaction.commit()?;
    }
    app.asset_protocol_scope()
        .allow_file(&png_path)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.95,
            stage: "Validating output",
            error_message: None,
            result_path: Some(sheet.png_path.as_str()),
        },
    )?;
    let verified = image::open(&png_path)?;
    if verified.width() != output_width || verified.height() != output_height {
        return Err(CommandError::new(
            "sheet_validation_failed",
            "The exported sprite-sheet dimensions did not match the plan",
        ));
    }
    Ok(sheet)
}

fn render_procedural_vfx(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    input: ProceduralVfxInput,
) -> CommandResult<VfxEffect> {
    validate_vfx_input(&input)?;
    let valid_worktree: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2 AND kind='vfx')",
            params![input.worktree_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !valid_worktree {
        return Err(CommandError::new(
            "invalid_vfx_worktree",
            "Procedural effects must be created inside a VFX worktree",
        ));
    }
    let effect_id = Uuid::new_v4().to_string();
    let animation_id = Uuid::new_v4().to_string();
    let root = workspace_path(state, &input.project_id)?;
    let slug = portable_slug(input.name.trim());
    let output_directory =
        root.join("assets")
            .join("vfx")
            .join(format!("{}-{}", slug, &effect_id[..8]));
    std::fs::create_dir_all(&output_directory)?;
    app.asset_protocol_scope()
        .allow_directory(&output_directory, true)
        .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
    let mut generated_assets = Vec::with_capacity(input.frames as usize);
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "running",
            progress: 0.04,
            stage: "Preparing procedural renderer",
            error_message: None,
            result_path: None,
        },
    )?;
    for frame_index in 0..input.frames {
        if cancellation_requested(state, job_id)? {
            return Err(CommandError::new(
                "job_cancelled",
                "Procedural VFX generation cancelled",
            ));
        }
        let frame = render_vfx_frame(&input, frame_index);
        let path =
            output_directory.join(format!("{}_{:02}.png", slug, frame_index.saturating_add(1)));
        frame.save(&path)?;
        let asset = inspect(&input.project_id, &root, &path, None)?;
        upsert(state, &asset, "procedural_vfx")?;
        {
            let connection = state
                .db
                .lock()
                .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
            connection.execute(
                "INSERT OR REPLACE INTO asset_worktrees(asset_id, worktree_id, relationship, created_at) VALUES (?1,?2,'owned',?3)",
                params![asset.id, input.worktree_id, Utc::now().to_rfc3339()],
            )?;
        }
        generated_assets.push(asset);
        set_job_state(
            app,
            state,
            job_id,
            JobProgress {
                status: "running",
                progress: 0.08 + 0.76 * ((frame_index + 1) as f64 / input.frames as f64),
                stage: &format!(
                    "Rendering effect frame {} of {}",
                    frame_index + 1,
                    input.frames
                ),
                error_message: None,
                result_path: None,
            },
        )?;
    }
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.90,
            stage: "Validating alpha and frame dimensions",
            error_message: None,
            result_path: None,
        },
    )?;
    for asset in &generated_assets {
        let frame = image::open(&asset.path)?.to_rgba8();
        if frame.width() != input.width || frame.height() != input.height {
            return Err(CommandError::new(
                "vfx_validation_failed",
                "A procedural frame did not match the requested dimensions",
            ));
        }
        if !frame.pixels().any(|pixel| pixel[3] == 0) {
            return Err(CommandError::new(
                "vfx_validation_failed",
                "A procedural frame lost its transparent background",
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    let duration_ms = (1000.0 / input.fps as f64).round() as u32;
    let animation_frames: Vec<_> = generated_assets
        .iter()
        .map(|asset| AnimationFrame {
            asset_id: asset.id.clone(),
            duration_ms: Some(duration_ms.max(1)),
        })
        .collect();
    let frames_json = serde_json::to_string(&animation_frames)
        .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
    let effect = VfxEffect {
        id: effect_id,
        project_id: input.project_id,
        worktree_id: input.worktree_id,
        animation_id: Some(animation_id.clone()),
        name: input.name.trim().to_string(),
        effect_type: input.effect_type,
        blend_mode: input.blend_mode,
        center_x: 0.5,
        center_y: 0.5,
        opacity: 1.0,
        looping: input.looping,
        fps: input.fps as f64,
        created_at: now.clone(),
        updated_at: now,
    };
    {
        let mut connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        let transaction = connection.transaction()?;
        transaction.execute(
            r#"INSERT INTO animations(
                id, workspace_id, worktree_id, name, fps, looping, frames_json, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)"#,
            params![
                animation_id,
                effect.project_id,
                effect.worktree_id,
                effect.name,
                effect.fps,
                effect.looping,
                frames_json,
                effect.created_at
            ],
        )?;
        for (position, frame) in animation_frames.iter().enumerate() {
            transaction.execute(
                r#"INSERT INTO animation_frames(
                    id, animation_id, asset_id, position, duration_ms, pivot_x, pivot_y, created_at
                ) VALUES (?1,?2,?3,?4,?5,0.5,0.5,?6)"#,
                params![
                    Uuid::new_v4().to_string(),
                    animation_id,
                    frame.asset_id,
                    position as u32,
                    frame.duration_ms,
                    effect.created_at
                ],
            )?;
        }
        transaction.execute(
            r#"INSERT INTO vfx_effects(
                id, project_id, worktree_id, animation_id, name, effect_type,
                blend_mode, center_x, center_y, opacity, looping, fps, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"#,
            params![
                effect.id,
                effect.project_id,
                effect.worktree_id,
                effect.animation_id,
                effect.name,
                effect.effect_type,
                effect.blend_mode,
                effect.center_x,
                effect.center_y,
                effect.opacity,
                effect.looping,
                effect.fps,
                effect.created_at,
                effect.updated_at
            ],
        )?;
        transaction.execute(
            "UPDATE background_jobs SET target_type='vfx', target_id=?2 WHERE id=?1",
            params![job_id, effect.id],
        )?;
        transaction.commit()?;
    }
    let result_path = generated_assets.first().map(|asset| asset.path.as_str());
    set_job_state(
        app,
        state,
        job_id,
        JobProgress {
            status: "analyzing",
            progress: 0.97,
            stage: "Registering animation and effect",
            error_message: None,
            result_path,
        },
    )?;
    Ok(effect)
}

#[tauri::command]
pub fn list_jobs(
    project_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<BackgroundJob>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut jobs = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY created_at DESC LIMIT 100",
            select_job()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], job_row)?;
        jobs.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY created_at DESC LIMIT 100",
            select_job()
        ))?;
        let rows = statement.query_map([project_id], job_row)?;
        jobs.extend(rows.filter_map(Result::ok));
    }
    Ok(jobs)
}

#[tauri::command]
pub fn cancel_job(id: String, state: State<'_, AppState>) -> CommandResult<BackgroundJob> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    connection.execute(
        r#"UPDATE background_jobs
           SET cancel_requested=1,
               status=CASE WHEN status='queued' THEN 'cancelled' ELSE status END,
               stage=CASE WHEN status='queued' THEN 'Cancelled' ELSE 'Cancelling' END,
               completed_at=CASE WHEN status='queued' THEN ?2 ELSE completed_at END,
               updated_at=?2
           WHERE id=?1 AND status IN ('queued','running','analyzing')"#,
        params![id, now],
    )?;
    drop(connection);
    load_job(&state, &id)
}

#[tauri::command]
pub fn list_sprite_sheets(
    project_id: String,
    worktree_id: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Vec<SpriteSheet>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut sheets = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY updated_at DESC",
            select_sheet()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], sheet_row)?;
        sheets.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY updated_at DESC",
            select_sheet()
        ))?;
        let rows = statement.query_map([project_id], sheet_row)?;
        sheets.extend(rows.filter_map(Result::ok));
    }
    for sheet in &sheets {
        if Path::new(&sheet.png_path).is_file() {
            app.asset_protocol_scope()
                .allow_file(&sheet.png_path)
                .map_err(|error| CommandError::new("asset_scope_error", error.to_string()))?;
        }
    }
    Ok(sheets)
}

#[tauri::command]
pub fn queue_sprite_sheet(
    input: SpriteSheetInput,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    validate_input(&input)?;
    let animation_exists: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM animations WHERE id=?1 AND workspace_id=?2)",
            params![input.animation_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !animation_exists {
        return Err(CommandError::new(
            "animation_not_found",
            "The selected animation no longer exists in this project",
        ));
    }
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            r#"INSERT INTO background_jobs(
                id, project_id, worktree_id, kind, target_type, target_id,
                status, progress, stage, created_at, updated_at
            ) VALUES (?1,?2,?3,'sprite_sheet','animation',?4,'queued',0.0,'Queued',?5,?5)"#,
            params![
                job_id,
                input.project_id,
                input.worktree_id,
                input.animation_id,
                now
            ],
        )?;
    }
    let queued = emit_job(&app, &state, &job_id)?;
    let task_app = app.clone();
    let task_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let render_app = task_app.clone();
        let render_job_id = task_job_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let render_state = render_app.state::<AppState>();
            render_sprite_sheet(&render_app, &render_state, &render_job_id, input)
        })
        .await;
        let task_state = task_app.state::<AppState>();
        match result {
            Ok(Ok(sheet)) => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "completed",
                        progress: 1.0,
                        stage: "Completed",
                        error_message: None,
                        result_path: Some(sheet.png_path.as_str()),
                    },
                );
            }
            Ok(Err(error)) if error.code == "job_cancelled" => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "cancelled",
                        progress: 0.0,
                        stage: "Cancelled",
                        error_message: None,
                        result_path: None,
                    },
                );
            }
            Ok(Err(error)) => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&error.message),
                        result_path: None,
                    },
                );
            }
            Err(error) => {
                let message = error.to_string();
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&message),
                        result_path: None,
                    },
                );
            }
        }
    });
    Ok(queued)
}

#[tauri::command]
pub fn list_vfx_effects(
    project_id: String,
    worktree_id: Option<String>,
    state: State<'_, AppState>,
) -> CommandResult<Vec<VfxEffect>> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let mut effects = Vec::new();
    if let Some(worktree_id) = worktree_id {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 AND worktree_id=?2 ORDER BY updated_at DESC",
            select_vfx()
        ))?;
        let rows = statement.query_map(params![project_id, worktree_id], vfx_row)?;
        effects.extend(rows.filter_map(Result::ok));
    } else {
        let mut statement = connection.prepare(&format!(
            "{} WHERE project_id=?1 ORDER BY updated_at DESC",
            select_vfx()
        ))?;
        let rows = statement.query_map([project_id], vfx_row)?;
        effects.extend(rows.filter_map(Result::ok));
    }
    Ok(effects)
}

#[tauri::command]
pub fn queue_procedural_vfx(
    input: ProceduralVfxInput,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<BackgroundJob> {
    validate_vfx_input(&input)?;
    let valid_worktree: bool = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM worktrees WHERE id=?1 AND project_id=?2 AND kind='vfx')",
            params![input.worktree_id, input.project_id],
            |row| row.get(0),
        )?
    };
    if !valid_worktree {
        return Err(CommandError::new(
            "invalid_vfx_worktree",
            "Choose a VFX worktree before creating a procedural effect",
        ));
    }
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute(
            r#"INSERT INTO background_jobs(
                id, project_id, worktree_id, kind, target_type, status,
                progress, stage, created_at, updated_at
            ) VALUES (?1,?2,?3,'procedural_vfx','vfx','queued',0.0,'Queued',?4,?4)"#,
            params![job_id, input.project_id, input.worktree_id, now],
        )?;
    }
    let queued = emit_job(&app, &state, &job_id)?;
    let task_app = app.clone();
    let task_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let render_app = task_app.clone();
        let render_job_id = task_job_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            let render_state = render_app.state::<AppState>();
            render_procedural_vfx(&render_app, &render_state, &render_job_id, input)
        })
        .await;
        let task_state = task_app.state::<AppState>();
        match result {
            Ok(Ok(_)) => {
                let completed = load_job(&task_state, &task_job_id).ok();
                let result_path = completed
                    .as_ref()
                    .and_then(|job| job.result_path.as_deref());
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "completed",
                        progress: 1.0,
                        stage: "Completed",
                        error_message: None,
                        result_path,
                    },
                );
            }
            Ok(Err(error)) if error.code == "job_cancelled" => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "cancelled",
                        progress: 0.0,
                        stage: "Cancelled",
                        error_message: None,
                        result_path: None,
                    },
                );
            }
            Ok(Err(error)) => {
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&error.message),
                        result_path: None,
                    },
                );
            }
            Err(error) => {
                let message = error.to_string();
                let _ = set_job_state(
                    &task_app,
                    &task_state,
                    &task_job_id,
                    JobProgress {
                        status: "failed",
                        progress: 0.0,
                        stage: "Failed",
                        error_message: Some(&message),
                        result_path: None,
                    },
                );
            }
        }
    });
    Ok(queued)
}

#[tauri::command]
pub fn delete_sprite_sheet(id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let paths: Option<(String, String)> = {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection
            .query_row(
                "SELECT png_path, metadata_path FROM sprite_sheets WHERE id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
    };
    let Some((png_path, metadata_path)) = paths else {
        return Err(CommandError::new(
            "sprite_sheet_not_found",
            "The sprite sheet no longer exists",
        ));
    };
    {
        let connection = state
            .db
            .lock()
            .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
        connection.execute("DELETE FROM sprite_sheets WHERE id=?1", [&id])?;
    }
    for path in [PathBuf::from(png_path), PathBuf::from(metadata_path)] {
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{frame_offset, layout_dimensions, render_vfx_frame};
    use crate::models::ProceduralVfxInput;

    #[test]
    fn computes_horizontal_vertical_and_grid_layouts() {
        assert_eq!(layout_dimensions("horizontal", 12, 4), (12, 1));
        assert_eq!(layout_dimensions("vertical", 12, 4), (1, 12));
        assert_eq!(layout_dimensions("grid", 12, 4), (4, 3));
        assert_eq!(layout_dimensions("grid", 10, 4), (4, 3));
    }

    #[test]
    fn aligns_frames_without_losing_ground_contact() {
        assert_eq!(frame_offset("top_left", 64, 64, 32, 40), (0, 0));
        assert_eq!(frame_offset("center", 64, 64, 32, 40), (16, 12));
        assert_eq!(frame_offset("bottom_center", 64, 64, 32, 40), (16, 24));
    }

    #[test]
    fn procedural_vfx_frames_are_distinct_and_transparent() {
        for effect_type in [
            "magic",
            "frost_lance",
            "storm_lance",
            "nova_beam",
            "voltaic_snare",
        ] {
            let input = ProceduralVfxInput {
                project_id: "project".into(),
                worktree_id: "vfx".into(),
                name: "Effect test".into(),
                effect_type: effect_type.into(),
                blend_mode: "screen".into(),
                width: 64,
                height: 64,
                frames: 12,
                fps: 12,
                looping: effect_type == "magic",
                seed: 42,
            };
            let frames: Vec<_> = (0..input.frames)
                .map(|index| render_vfx_frame(&input, index))
                .collect();
            let hashes: std::collections::HashSet<_> = frames
                .iter()
                .map(|frame| blake3::hash(frame.as_raw()))
                .collect();
            assert_eq!(
                hashes.len(),
                frames.len(),
                "{effect_type} frames should animate"
            );
            assert!(frames
                .iter()
                .all(|frame| frame.pixels().any(|pixel| pixel[3] == 0)));
            assert!(frames
                .iter()
                .all(|frame| frame.pixels().any(|pixel| pixel[3] > 0)));
        }
    }
}
