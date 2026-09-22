use crate::{
    error::{CommandError, CommandResult},
    models::{AnimationFrame, CharacterAnchor, Pivot},
};
use serde_json::{json, Value};
use std::path::Path;

pub const EXPORT_FORMAT_SPRITE_STUDIO: &str = "sprite-studio";
pub const EXPORT_FORMAT_ASEPRITE: &str = "aseprite-json";
pub const EXPORT_FORMAT_TEXTUREPACKER: &str = "texturepacker";
pub const EXPORT_FORMAT_GODOT: &str = "godot-spriteframes";

pub fn normalize_export_format(format: Option<&str>) -> Result<&'static str, CommandError> {
    match format
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        None | Some("") | Some("sprite-studio") | Some("json") => Ok(EXPORT_FORMAT_SPRITE_STUDIO),
        Some("aseprite-json") | Some("aseprite") => Ok(EXPORT_FORMAT_ASEPRITE),
        Some("texturepacker") | Some("tp") => Ok(EXPORT_FORMAT_TEXTUREPACKER),
        Some("godot-spriteframes") | Some("godot") => Ok(EXPORT_FORMAT_GODOT),
        Some(other) => Err(CommandError::new(
            "invalid_export_format",
            format!("Unsupported export format `{other}`"),
        )),
    }
}

pub fn metadata_extension(format: &str) -> &'static str {
    if format == EXPORT_FORMAT_GODOT {
        "tres"
    } else {
        "json"
    }
}

#[derive(Debug, Clone)]
pub struct PreparedFrame {
    pub frame: AnimationFrame,
    pub asset_id: String,
    pub relative_path: String,
    pub source_width: u32,
    pub source_height: u32,
    pub sheet_x: u32,
    pub sheet_y: u32,
    pub draw_x: i32,
    pub draw_y: i32,
    pub duration_ms: u32,
}

#[derive(Debug, Clone)]
pub struct PreparedStripExport {
    pub frame_width: u32,
    pub frame_height: u32,
    pub sheet_width: u32,
    pub sheet_height: u32,
    pub frames: Vec<PreparedFrame>,
}

#[derive(Debug, Clone)]
pub struct AnchorExportMeta {
    pub slug: Option<String>,
    pub baseline_y: u32,
    pub pivot: Pivot,
    pub source_width: u32,
    pub source_height: u32,
}

pub fn anchor_meta_from_character(anchor: &CharacterAnchor) -> AnchorExportMeta {
    AnchorExportMeta {
        slug: Some(anchor.slug.clone()),
        baseline_y: anchor.baseline_y,
        pivot: anchor.pivot.clone(),
        source_width: anchor.frame_width,
        source_height: anchor.frame_height,
    }
}

pub fn default_anchor_meta(frame_width: u32, frame_height: u32) -> AnchorExportMeta {
    AnchorExportMeta {
        slug: None,
        baseline_y: frame_height.saturating_sub(1),
        pivot: Pivot {
            x: frame_width as f64 / 2.0,
            y: frame_height as f64 - 1.0,
        },
        source_width: frame_width,
        source_height: frame_height,
    }
}

fn frame_json_object(
    index: usize,
    prepared: &PreparedFrame,
    cell_width: u32,
    cell_height: u32,
    anchor: &AnchorExportMeta,
) -> Value {
    let sprite_x = prepared.draw_x.max(0) as u32;
    let sprite_y = prepared.draw_y.max(0) as u32;
    json!({
        "index": index,
        "assetId": prepared.asset_id,
        "source": prepared.relative_path,
        "x": prepared.sheet_x,
        "y": prepared.sheet_y,
        "width": cell_width,
        "height": cell_height,
        "durationMs": prepared.duration_ms,
        "sourceSize": { "w": anchor.source_width, "h": anchor.source_height },
        "spriteSourceSize": {
            "x": sprite_x,
            "y": sprite_y,
            "w": prepared.source_width,
            "h": prepared.source_height
        },
        "trimOffset": {
            "x": prepared.frame.offset_x,
            "y": prepared.frame.offset_y
        }
    })
}

#[derive(Debug, Clone)]
pub struct SpriteSheetGridMeta {
    pub layout: String,
    pub padding: u32,
    pub spacing: u32,
    pub rows: u32,
    pub columns: u32,
    pub scale: u32,
    pub transparent: bool,
    pub alignment: String,
    pub source_animation_id: String,
    pub source_animation_name: String,
}

pub fn build_sprite_studio_metadata(
    animation_name: &str,
    image_file_name: &str,
    fps: f64,
    looping: bool,
    anchor: &AnchorExportMeta,
    prepared: &PreparedStripExport,
    grid: Option<&SpriteSheetGridMeta>,
) -> Value {
    let frames = prepared
        .frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            frame_json_object(index, frame, prepared.frame_width, prepared.frame_height, anchor)
        })
        .collect::<Vec<_>>();
    let mut payload = json!({
        "format": EXPORT_FORMAT_SPRITE_STUDIO,
        "name": animation_name,
        "image": image_file_name,
        "frameWidth": prepared.frame_width,
        "frameHeight": prepared.frame_height,
        "frameCount": frames.len(),
        "fps": fps,
        "loop": looping,
        "anchorSlug": anchor.slug,
        "baselineY": anchor.baseline_y,
        "pivot": { "x": anchor.pivot.x, "y": anchor.pivot.y },
        "sourceSize": { "w": anchor.source_width, "h": anchor.source_height },
        "frames": frames
    });
    if let Some(grid) = grid {
        if let Some(object) = payload.as_object_mut() {
            object.insert("layout".into(), json!(grid.layout));
            object.insert("padding".into(), json!(grid.padding));
            object.insert("spacing".into(), json!(grid.spacing));
            object.insert("rows".into(), json!(grid.rows));
            object.insert("columns".into(), json!(grid.columns));
            object.insert("scale".into(), json!(grid.scale));
            object.insert("transparent".into(), json!(grid.transparent));
            object.insert("alignment".into(), json!(grid.alignment));
            object.insert(
                "sourceAnimation".into(),
                json!({
                    "id": grid.source_animation_id,
                    "name": grid.source_animation_name
                }),
            );
        }
    }
    payload
}

pub fn build_aseprite_metadata(
    animation_name: &str,
    image_file_name: &str,
    looping: bool,
    anchor: &AnchorExportMeta,
    prepared: &PreparedStripExport,
) -> Value {
    let mut frames = serde_json::Map::new();
    for (index, frame) in prepared.frames.iter().enumerate() {
        let key = format!("frame_{:04}.png", index + 1);
        frames.insert(
            key,
            json!({
                "frame": {
                    "x": frame.sheet_x,
                    "y": frame.sheet_y,
                    "w": prepared.frame_width,
                    "h": prepared.frame_height
                },
                "rotated": false,
                "trimmed": frame.frame.offset_x != 0 || frame.frame.offset_y != 0,
                "spriteSourceSize": {
                    "x": frame.draw_x.max(0),
                    "y": frame.draw_y.max(0),
                    "w": frame.source_width,
                    "h": frame.source_height
                },
                "sourceSize": {
                    "w": prepared.frame_width,
                    "h": prepared.frame_height
                },
                "duration": frame.duration_ms
            }),
        );
    }
    let last_frame = prepared.frames.len().saturating_sub(1);
    json!({
        "frames": frames,
        "meta": {
            "app": "Sprite Studio",
            "version": "1.0",
            "image": image_file_name,
            "format": "RGBA8888",
            "size": { "w": prepared.sheet_width, "h": prepared.sheet_height },
            "scale": "1",
            "frameTags": [{
                "name": animation_name,
                "from": 0,
                "to": last_frame,
                "direction": "forward"
            }],
            "anchorSlug": anchor.slug,
            "baselineY": anchor.baseline_y,
            "pivot": { "x": anchor.pivot.x, "y": anchor.pivot.y },
            "sourceSize": { "w": anchor.source_width, "h": anchor.source_height },
            "loop": looping
        }
    })
}

pub fn build_texturepacker_metadata(
    image_file_name: &str,
    anchor: &AnchorExportMeta,
    prepared: &PreparedStripExport,
) -> Value {
    let pivot_x = if anchor.source_width > 0 {
        anchor.pivot.x / anchor.source_width as f64
    } else {
        0.5
    };
    let pivot_y = if anchor.source_height > 0 {
        anchor.pivot.y / anchor.source_height as f64
    } else {
        1.0
    };
    let mut frames = serde_json::Map::new();
    for (index, frame) in prepared.frames.iter().enumerate() {
        let key = format!("frame_{:04}.png", index + 1);
        frames.insert(
            key,
            json!({
                "frame": {
                    "x": frame.sheet_x,
                    "y": frame.sheet_y,
                    "w": prepared.frame_width,
                    "h": prepared.frame_height
                },
                "rotated": false,
                "trimmed": false,
                "spriteSourceSize": {
                    "x": frame.draw_x.max(0),
                    "y": frame.draw_y.max(0),
                    "w": frame.source_width,
                    "h": frame.source_height
                },
                "sourceSize": {
                    "w": prepared.frame_width,
                    "h": prepared.frame_height
                },
                "pivot": { "x": pivot_x, "y": pivot_y }
            }),
        );
    }
    json!({
        "frames": frames,
        "meta": {
            "app": "Sprite Studio",
            "version": "1.0",
            "image": image_file_name,
            "format": "RGBA8888",
            "size": { "w": prepared.sheet_width, "h": prepared.sheet_height },
            "scale": "1",
            "smartupdate": "",
            "anchorSlug": anchor.slug,
            "baselineY": anchor.baseline_y,
            "sourceSize": { "w": anchor.source_width, "h": anchor.source_height }
        }
    })
}

fn godot_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

pub fn build_godot_spriteframes_resource(
    animation_name: &str,
    texture_path: &str,
    fps: f64,
    looping: bool,
    anchor: &AnchorExportMeta,
    prepared: &PreparedStripExport,
) -> String {
    let load_steps = 2 + prepared.frames.len();
    let mut lines = vec![
        format!("[gd_resource type=\"SpriteFrames\" load_steps={load_steps} format=3]"),
        format!(
            "[ext_resource type=\"Texture2D\" path=\"{}\" id=\"1_texture\"]",
            godot_escape(texture_path)
        ),
        String::new(),
    ];
    let mut atlas_entries = Vec::new();
    for (index, frame) in prepared.frames.iter().enumerate() {
        let duration_multiplier = (frame.duration_ms as f64 / 1000.0) * fps.max(1.0);
        let atlas_id = format!("AtlasTexture_{index}");
        lines.push(format!("[sub_resource type=\"AtlasTexture\" id=\"{atlas_id}\"]"));
        lines.push("atlas = ExtResource(\"1_texture\")".to_string());
        lines.push(format!(
            "region = Rect2({}, {}, {}, {})",
            frame.sheet_x,
            frame.sheet_y,
            prepared.frame_width,
            prepared.frame_height
        ));
        lines.push(String::new());
        atlas_entries.push(format!(
            "{{\"duration\": {duration_multiplier:.4}, \"texture\": SubResource(\"{atlas_id}\")}}"
        ));
    }
    let animation_slug = godot_escape(animation_name);
    lines.push("[resource]".to_string());
    lines.push(format!(
        "animations = [{{\"frames\": [{}], \"loop\": {}, \"name\": &\"{animation_slug}\", \"speed\": {}}}]",
        atlas_entries.join(", "),
        if looping { "true" } else { "false" },
        fps
    ));
    lines.push(format!(
        "metadata/fixed_cell = {{ \"anchorSlug\": \"{}\", \"baselineY\": {}, \"pivot\": Vector2({}, {}), \"sourceSize\": Vector2i({}, {}) }}",
        anchor.slug.clone().unwrap_or_default(),
        anchor.baseline_y,
        anchor.pivot.x,
        anchor.pivot.y,
        anchor.source_width,
        anchor.source_height
    ));
    lines.join("\n")
}

pub fn build_metadata_payload(
    format: &str,
    animation_name: &str,
    image_file_name: &str,
    godot_texture_path: &str,
    fps: f64,
    looping: bool,
    anchor: &AnchorExportMeta,
    prepared: &PreparedStripExport,
    grid: Option<&SpriteSheetGridMeta>,
) -> Result<(String, String), CommandError> {
    match format {
        EXPORT_FORMAT_SPRITE_STUDIO => {
            let payload = build_sprite_studio_metadata(
                animation_name,
                image_file_name,
                fps,
                looping,
                anchor,
                prepared,
                grid,
            );
            let bytes = serde_json::to_vec_pretty(&payload)
                .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
            Ok((String::from_utf8(bytes).unwrap_or_default(), "json".into()))
        }
        EXPORT_FORMAT_ASEPRITE => {
            let payload = build_aseprite_metadata(
                animation_name,
                image_file_name,
                looping,
                anchor,
                prepared,
            );
            let bytes = serde_json::to_vec_pretty(&payload)
                .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
            Ok((String::from_utf8(bytes).unwrap_or_default(), "json".into()))
        }
        EXPORT_FORMAT_TEXTUREPACKER => {
            let payload = build_texturepacker_metadata(image_file_name, anchor, prepared);
            let bytes = serde_json::to_vec_pretty(&payload)
                .map_err(|error| CommandError::new("serialization_error", error.to_string()))?;
            Ok((String::from_utf8(bytes).unwrap_or_default(), "json".into()))
        }
        EXPORT_FORMAT_GODOT => Ok((
            build_godot_spriteframes_resource(
                animation_name,
                godot_texture_path,
                fps,
                looping,
                anchor,
                prepared,
            ),
            "tres".into(),
        )),
        _ => Err(CommandError::new(
            "invalid_export_format",
            format!("Unsupported export format `{format}`"),
        )),
    }
}

pub fn write_metadata_file(path: &Path, contents: &str) -> CommandResult<()> {
    std::fs::write(path, contents)
        .map_err(|error| CommandError::new("export_write_failed", error.to_string()))
}

pub fn godot_texture_path_for_workspace(png_path: &Path, workspace_root: &Path) -> String {
    let relative = match png_path.strip_prefix(workspace_root) {
        Ok(path) => path.to_string_lossy().replace('\\', "/"),
        Err(_) => png_path.to_string_lossy().replace('\\', "/"),
    };
    format!("res://{relative}")
}
