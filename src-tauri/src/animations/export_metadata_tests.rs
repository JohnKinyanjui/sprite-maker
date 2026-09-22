use super::export_metadata::{
    build_aseprite_metadata, build_godot_spriteframes_resource, build_sprite_studio_metadata,
    build_texturepacker_metadata, default_anchor_meta, godot_texture_path_for_workspace,
    PreparedFrame, PreparedStripExport,
};
use crate::models::AnimationFrame;
use std::path::Path;

fn sample_export() -> PreparedStripExport {
    PreparedStripExport {
        frame_width: 32,
        frame_height: 32,
        sheet_width: 64,
        sheet_height: 32,
        frames: vec![
            PreparedFrame {
                frame: AnimationFrame {
                    asset_id: "a1".into(),
                    duration_ms: Some(100),
                    offset_x: 0,
                    offset_y: 0,
                },
                asset_id: "a1".into(),
                relative_path: "assets/walk/frame-01.png".into(),
                source_width: 28,
                source_height: 30,
                sheet_x: 0,
                sheet_y: 0,
                draw_x: 2,
                draw_y: 1,
                duration_ms: 100,
            },
            PreparedFrame {
                frame: AnimationFrame {
                    asset_id: "a2".into(),
                    duration_ms: Some(100),
                    offset_x: 1,
                    offset_y: 0,
                },
                asset_id: "a2".into(),
                relative_path: "assets/walk/frame-02.png".into(),
                source_width: 27,
                source_height: 30,
                sheet_x: 32,
                sheet_y: 0,
                draw_x: 3,
                draw_y: 1,
                duration_ms: 100,
            },
        ],
    }
}

#[test]
fn sprite_studio_metadata_includes_fixed_cell_fields() {
    let anchor = default_anchor_meta(32, 32);
    let metadata = build_sprite_studio_metadata("Walk", "walk.png", 10.0, true, &anchor, &sample_export(), None);
    assert_eq!(metadata["anchorSlug"], serde_json::Value::Null);
    assert_eq!(metadata["baselineY"], 31);
    assert_eq!(metadata["sourceSize"]["w"], 32);
    assert_eq!(metadata["frames"][0]["spriteSourceSize"]["x"], 2);
}

#[test]
fn aseprite_metadata_includes_frame_tags_and_pivot() {
    let anchor = default_anchor_meta(32, 32);
    let metadata = build_aseprite_metadata("Walk", "walk.png", true, &anchor, &sample_export());
    assert_eq!(metadata["meta"]["image"], "walk.png");
    assert!(metadata["frames"]["frame_0001.png"].is_object());
    assert_eq!(metadata["frames"]["frame_0002.png"]["duration"], 100);
    assert_eq!(metadata["meta"]["frameTags"][0]["name"], "Walk");
    assert_eq!(metadata["meta"]["pivot"]["x"], 16.0);
}

#[test]
fn texturepacker_metadata_matches_expected_layout_and_pivot() {
    let anchor = default_anchor_meta(32, 32);
    let metadata = build_texturepacker_metadata("walk.png", &anchor, &sample_export());
    assert_eq!(metadata["frames"]["frame_0001.png"]["frame"]["x"], 0);
    assert_eq!(metadata["frames"]["frame_0002.png"]["frame"]["x"], 32);
    assert_eq!(metadata["frames"]["frame_0001.png"]["pivot"]["x"], 0.5);
}

#[test]
fn godot_spriteframes_resource_uses_workspace_relative_texture_and_frame_duration() {
    let anchor = default_anchor_meta(32, 32);
    let resource = build_godot_spriteframes_resource(
        "Walk",
        "res://exports/sprite-sheets/walk-abc.png",
        10.0,
        true,
        &anchor,
        &sample_export(),
    );
    assert!(resource.contains("load_steps=4"));
    assert!(resource.contains("res://exports/sprite-sheets/walk-abc.png"));
    assert!(resource.contains("region = Rect2(0, 0, 32, 32)"));
    assert!(resource.contains("region = Rect2(32, 0, 32, 32)"));
    assert!(resource.contains("\"duration\": 1.0000"));
    assert!(resource.contains("\"speed\": 10"));
}

#[test]
fn godot_texture_path_is_relative_to_workspace_root() {
    let workspace = Path::new("C:/game");
    let png = workspace.join("exports").join("sprite-sheets").join("hero.png");
    assert_eq!(
        godot_texture_path_for_workspace(&png, workspace),
        "res://exports/sprite-sheets/hero.png"
    );
}

#[test]
fn grid_sheet_trim_coordinates_stay_within_cell_bounds() {
    let anchor = default_anchor_meta(64, 64);
    let prepared = PreparedStripExport {
        frame_width: 64,
        frame_height: 64,
        sheet_width: 128,
        sheet_height: 64,
        frames: vec![PreparedFrame {
            frame: AnimationFrame {
                asset_id: "a1".into(),
                duration_ms: Some(100),
                offset_x: 2,
                offset_y: -1,
            },
            asset_id: "a1".into(),
            relative_path: "assets/walk/frame-01.png".into(),
            source_width: 60,
            source_height: 62,
            sheet_x: 64,
            sheet_y: 0,
            draw_x: 4,
            draw_y: 1,
            duration_ms: 100,
        }],
    };
    let metadata = build_sprite_studio_metadata("Walk", "sheet.png", 10.0, true, &anchor, &prepared, None);
    assert_eq!(metadata["frames"][0]["x"], 64);
    assert_eq!(metadata["frames"][0]["spriteSourceSize"]["x"], 4);
    assert!(metadata["frames"][0]["spriteSourceSize"]["x"]
        .as_u64()
        .unwrap()
        < metadata["sourceSize"]["w"].as_u64().unwrap());
}
