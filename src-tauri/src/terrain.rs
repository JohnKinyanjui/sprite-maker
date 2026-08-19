use crate::{
    assets::get_asset,
    error::{CommandError, CommandResult},
    models::{TerrainExportInput, TerrainExportResult},
    workspace::workspace_path,
    AppState,
};
use image::{DynamicImage, GenericImageView, ImageFormat, RgbaImage};
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

const TERRAIN_RULE_ROLES: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

const BLOB_COLUMNS: u32 = 8;
const TERRAIN_BITS: [(u8, &str); 8] = [
    (1, "top_side"),
    (2, "top_right_corner"),
    (4, "right_side"),
    (8, "bottom_right_corner"),
    (16, "bottom_side"),
    (32, "bottom_left_corner"),
    (64, "left_side"),
    (128, "top_left_corner"),
];

#[derive(Debug, PartialEq)]
struct GridLayout {
    columns: u32,
    rows: u32,
    trailing_x: u32,
    trailing_y: u32,
}

fn axis_layout(image_size: u32, margin: u32, tile: u32, separation: u32) -> Option<(u32, u32)> {
    if tile == 0 || margin >= image_size || image_size - margin < tile {
        return None;
    }
    let available = image_size - margin;
    let stride = tile.checked_add(separation)?;
    let count = 1 + (available - tile) / stride;
    let consumed = count
        .checked_mul(tile)?
        .checked_add(count.saturating_sub(1).checked_mul(separation)?)?;
    Some((count, available - consumed))
}

fn grid_layout(
    image_width: u32,
    image_height: u32,
    input: &TerrainExportInput,
) -> CommandResult<GridLayout> {
    let (columns, trailing_x) = axis_layout(
        image_width,
        input.margin_x,
        input.tile_width,
        input.separation_x,
    )
    .ok_or_else(|| {
        CommandError::new(
            "invalid_terrain_grid",
            "Tile width and horizontal margin do not fit inside the atlas",
        )
    })?;
    let (rows, trailing_y) = axis_layout(
        image_height,
        input.margin_y,
        input.tile_height,
        input.separation_y,
    )
    .ok_or_else(|| {
        CommandError::new(
            "invalid_terrain_grid",
            "Tile height and vertical margin do not fit inside the atlas",
        )
    })?;
    Ok(GridLayout {
        columns,
        rows,
        trailing_x,
        trailing_y,
    })
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        "terrain-tileset".into()
    } else {
        output
    }
}

fn cell_has_pixels(
    image: &image::DynamicImage,
    column: u32,
    row: u32,
    input: &TerrainExportInput,
) -> bool {
    let start_x = input.margin_x + column * (input.tile_width + input.separation_x);
    let start_y = input.margin_y + row * (input.tile_height + input.separation_y);
    (start_y..start_y + input.tile_height)
        .any(|y| (start_x..start_x + input.tile_width).any(|x| image.get_pixel(x, y).0[3] != 0))
}

fn terrain_peering_bits(role: &str) -> Option<Vec<&'static str>> {
    if let Some(mask) = role
        .strip_prefix("blob_")
        .and_then(|value| value.parse::<u8>().ok())
    {
        if is_blob_mask(mask) {
            return Some(
                TERRAIN_BITS
                    .iter()
                    .filter_map(|(bit, name)| (mask & bit != 0).then_some(*name))
                    .collect(),
            );
        }
        return None;
    }
    match role {
        "top_left" => Some(vec!["right_side", "bottom_right_corner", "bottom_side"]),
        "top" => Some(vec![
            "right_side",
            "bottom_right_corner",
            "bottom_side",
            "bottom_left_corner",
            "left_side",
        ]),
        "top_right" => Some(vec!["bottom_side", "bottom_left_corner", "left_side"]),
        "left" => Some(vec![
            "top_side",
            "top_right_corner",
            "right_side",
            "bottom_right_corner",
            "bottom_side",
        ]),
        "center" => Some(vec![
            "right_side",
            "bottom_right_corner",
            "bottom_side",
            "bottom_left_corner",
            "left_side",
            "top_left_corner",
            "top_side",
            "top_right_corner",
        ]),
        "right" => Some(vec![
            "bottom_side",
            "bottom_left_corner",
            "left_side",
            "top_left_corner",
            "top_side",
        ]),
        "bottom_left" => Some(vec!["top_side", "top_right_corner", "right_side"]),
        "bottom" => Some(vec![
            "left_side",
            "top_left_corner",
            "top_side",
            "top_right_corner",
            "right_side",
        ]),
        "bottom_right" => Some(vec!["left_side", "top_left_corner", "top_side"]),
        _ => None,
    }
}

fn is_blob_mask(mask: u8) -> bool {
    let top = mask & 1 != 0;
    let top_right = mask & 2 != 0;
    let right = mask & 4 != 0;
    let bottom_right = mask & 8 != 0;
    let bottom = mask & 16 != 0;
    let bottom_left = mask & 32 != 0;
    let left = mask & 64 != 0;
    let top_left = mask & 128 != 0;
    (!top_right || top && right)
        && (!bottom_right || right && bottom)
        && (!bottom_left || bottom && left)
        && (!top_left || left && top)
}

fn blob_masks() -> Vec<u8> {
    (0..=u8::MAX).filter(|mask| is_blob_mask(*mask)).collect()
}

fn blob_rules() -> Vec<crate::models::TerrainRuleInput> {
    blob_masks()
        .into_iter()
        .enumerate()
        .map(|(index, mask)| crate::models::TerrainRuleInput {
            role: format!("blob_{mask}"),
            column: index as u32 % BLOB_COLUMNS,
            row: index as u32 / BLOB_COLUMNS,
        })
        .collect()
}

fn terrain_mode(input: &TerrainExportInput) -> CommandResult<&'static str> {
    if input.terrain_rules.is_empty() {
        return Ok("plain");
    }
    match input.terrain_mode.as_deref().unwrap_or("nine_slice") {
        "nine_slice" => Ok("nine_slice"),
        "blob_47" => Ok("blob_47"),
        _ => Err(CommandError::new(
            "invalid_terrain_mode",
            "Terrain rule mode must be nine_slice or blob_47",
        )),
    }
}

fn validate_terrain_rules(
    input: &TerrainExportInput,
    layout: &GridLayout,
    occupied: &[(u32, u32)],
) -> CommandResult<()> {
    if input.terrain_rules.is_empty() {
        return Ok(());
    }
    if input.terrain_rules.len() != TERRAIN_RULE_ROLES.len() {
        return Err(CommandError::new(
            "invalid_terrain_rules",
            "3x3 auto-connect requires all nine terrain roles",
        ));
    }
    let mut roles = HashSet::new();
    let mut coordinates = HashSet::new();
    let occupied: HashSet<(u32, u32)> = occupied.iter().copied().collect();
    for rule in &input.terrain_rules {
        if terrain_peering_bits(&rule.role).is_none() || !roles.insert(rule.role.as_str()) {
            return Err(CommandError::new(
                "invalid_terrain_rules",
                "Terrain roles must be the nine unique 3x3 positions",
            ));
        }
        if rule.column >= layout.columns || rule.row >= layout.rows {
            return Err(CommandError::new(
                "invalid_terrain_rules",
                format!(
                    "The {} rule points outside the detected atlas grid",
                    rule.role.replace('_', " ")
                ),
            ));
        }
        if !coordinates.insert((rule.column, rule.row)) {
            return Err(CommandError::new(
                "invalid_terrain_rules",
                "Each terrain role must use a different atlas cell",
            ));
        }
        if !occupied.contains(&(rule.column, rule.row)) {
            return Err(CommandError::new(
                "invalid_terrain_rules",
                format!(
                    "The {} rule points to a fully transparent cell",
                    rule.role.replace('_', " ")
                ),
            ));
        }
    }
    if TERRAIN_RULE_ROLES.iter().any(|role| !roles.contains(role)) {
        return Err(CommandError::new(
            "invalid_terrain_rules",
            "3x3 auto-connect requires all nine terrain roles",
        ));
    }
    Ok(())
}

fn source_rule_coordinates(
    input: &TerrainExportInput,
) -> CommandResult<HashMap<String, (u32, u32)>> {
    let coordinates = input
        .terrain_rules
        .iter()
        .map(|rule| (rule.role.clone(), (rule.column, rule.row)))
        .collect::<HashMap<_, _>>();
    if TERRAIN_RULE_ROLES
        .iter()
        .any(|role| !coordinates.contains_key(*role))
    {
        return Err(CommandError::new(
            "invalid_terrain_rules",
            "Complete 47-tile terrain generation requires all nine source roles",
        ));
    }
    Ok(coordinates)
}

fn quadrant_source_role(mask: u8, quadrant: u8) -> &'static str {
    let (side_a, side_b, diagonal, outer, edge_a, edge_b) = match quadrant {
        0 => (1, 64, 128, "top_left", "left", "top"),
        1 => (1, 4, 2, "top_right", "right", "top"),
        2 => (16, 4, 8, "bottom_right", "right", "bottom"),
        _ => (16, 64, 32, "bottom_left", "left", "bottom"),
    };
    match (mask & side_a != 0, mask & side_b != 0) {
        (false, false) => outer,
        (true, false) => edge_a,
        (false, true) => edge_b,
        (true, true) if mask & diagonal != 0 => "center",
        (true, true) => outer,
    }
}

fn expand_blob_atlas(
    image: &DynamicImage,
    input: &TerrainExportInput,
) -> CommandResult<DynamicImage> {
    if input.tile_width < 2
        || input.tile_height < 2
        || input.tile_width % 2 != 0
        || input.tile_height % 2 != 0
    {
        return Err(CommandError::new(
            "invalid_blob_tile_size",
            "Complete 47-tile terrain generation requires even tile dimensions of at least 2 pixels",
        ));
    }
    let coordinates = source_rule_coordinates(input)?;
    let masks = blob_masks();
    let rows = (masks.len() as u32).div_ceil(BLOB_COLUMNS);
    let mut output = RgbaImage::new(BLOB_COLUMNS * input.tile_width, rows * input.tile_height);
    let half_width = input.tile_width / 2;
    let half_height = input.tile_height / 2;

    for (index, mask) in masks.into_iter().enumerate() {
        let output_column = index as u32 % BLOB_COLUMNS;
        let output_row = index as u32 / BLOB_COLUMNS;
        for y in 0..input.tile_height {
            for x in 0..input.tile_width {
                let quadrant = match (y < half_height, x < half_width) {
                    (true, true) => 0,
                    (true, false) => 1,
                    (false, false) => 2,
                    (false, true) => 3,
                };
                let role = quadrant_source_role(mask, quadrant);
                let (source_column, source_row) = coordinates[role];
                let source_x =
                    input.margin_x + source_column * (input.tile_width + input.separation_x) + x;
                let source_y =
                    input.margin_y + source_row * (input.tile_height + input.separation_y) + y;
                output.put_pixel(
                    output_column * input.tile_width + x,
                    output_row * input.tile_height + y,
                    image.get_pixel(source_x, source_y),
                );
            }
        }
    }
    Ok(DynamicImage::ImageRgba8(output))
}

fn godot_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn godot_resource(
    resource_texture_path: &str,
    input: &TerrainExportInput,
    layout: &GridLayout,
    occupied: &[(u32, u32)],
) -> String {
    let mut resource = format!(
        "[gd_resource type=\"TileSet\" load_steps=3 format=3]\n\n\
[ext_resource type=\"Texture2D\" path=\"{resource_texture_path}\" id=\"1_texture\"]\n\n\
[sub_resource type=\"TileSetAtlasSource\" id=\"TileSetAtlasSource_terrain\"]\n\
texture = ExtResource(\"1_texture\")\n\
texture_region_size = Vector2i({}, {})\n\
margins = Vector2i({}, {})\n\
separation = Vector2i({}, {})\n",
        input.tile_width,
        input.tile_height,
        input.margin_x,
        input.margin_y,
        input.separation_x,
        input.separation_y,
    );
    for (column, row) in occupied {
        resource.push_str(&format!("{column}:{row}/0 = 0\n"));
    }
    for rule in &input.terrain_rules {
        resource.push_str(&format!(
            "{column}:{row}/0/terrain_set = 0\n{column}:{row}/0/terrain = 0\n",
            column = rule.column,
            row = rule.row,
        ));
        if let Some(bits) = terrain_peering_bits(&rule.role) {
            for bit in bits {
                resource.push_str(&format!(
                    "{column}:{row}/0/terrains_peering_bit/{bit} = 0\n",
                    column = rule.column,
                    row = rule.row,
                ));
            }
        }
    }
    let terrain_metadata = if input.terrain_rules.is_empty() {
        String::new()
    } else {
        let terrain_name = input
            .terrain_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Terrain");
        format!(
            "terrain_set_0/mode = 0\nterrain_set_0/terrain_0/name = \"{}\"\nterrain_set_0/terrain_0/color = Color(0.34901962, 0.65882355, 0.41568628, 1)\n",
            godot_string(terrain_name)
        )
    };
    resource.push_str(&format!(
        "\n[resource]\n\
tile_size = Vector2i({}, {})\n\
{}\
sources/0 = SubResource(\"TileSetAtlasSource_terrain\")\n",
        input.tile_width, input.tile_height, terrain_metadata
    ));
    debug_assert!(occupied.len() <= (layout.columns * layout.rows) as usize);
    resource
}

fn validate_worktree(state: &AppState, input: &TerrainExportInput) -> CommandResult<()> {
    let connection = state
        .db
        .lock()
        .map_err(|_| CommandError::new("database_locked", "Database lock was poisoned"))?;
    let kind: Option<String> = connection
        .query_row(
            "SELECT kind FROM worktrees WHERE id=?1 AND project_id=?2",
            [&input.worktree_id, &input.project_id],
            |row| row.get(0),
        )
        .optional()?;
    if kind.as_deref() != Some("tileset") {
        return Err(CommandError::new(
            "invalid_terrain_worktree",
            "Godot terrain exports require a Terrain worktree",
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn export_godot_tileset(
    input: TerrainExportInput,
    state: State<'_, AppState>,
) -> CommandResult<TerrainExportResult> {
    validate_worktree(&state, &input)?;
    let asset = get_asset(&state, &input.asset_id)?;
    if asset.workspace_id != input.project_id {
        return Err(CommandError::new(
            "invalid_terrain_asset",
            "The selected atlas does not belong to this project",
        ));
    }
    let source = PathBuf::from(&asset.path);
    if !source.is_file() {
        return Err(CommandError::new(
            "asset_missing",
            "The selected terrain atlas was moved or deleted",
        ));
    }
    let image = image::open(&source)?;
    let layout = grid_layout(image.width(), image.height(), &input)?;
    let mut visible = Vec::new();
    for row in 0..layout.rows {
        for column in 0..layout.columns {
            if cell_has_pixels(&image, column, row, &input) {
                visible.push((column, row));
            }
        }
    }
    let occupied = if input.include_empty {
        (0..layout.rows)
            .flat_map(|row| (0..layout.columns).map(move |column| (column, row)))
            .collect::<Vec<_>>()
    } else {
        visible.clone()
    };
    if occupied.is_empty() {
        return Err(CommandError::new(
            "empty_terrain_atlas",
            "No visible tiles were found. Enable empty cells or adjust the grid",
        ));
    }
    validate_terrain_rules(&input, &layout, &visible)?;
    let mode = terrain_mode(&input)?;

    let (export_image, export_input, export_layout, export_occupied) = if mode == "blob_47" {
        let generated = expand_blob_atlas(&image, &input)?;
        let mut generated_input = input.clone();
        generated_input.margin_x = 0;
        generated_input.margin_y = 0;
        generated_input.separation_x = 0;
        generated_input.separation_y = 0;
        generated_input.include_empty = false;
        generated_input.terrain_rules = blob_rules();
        let generated_layout = GridLayout {
            columns: BLOB_COLUMNS,
            rows: 6,
            trailing_x: 0,
            trailing_y: 0,
        };
        let generated_occupied = generated_input
            .terrain_rules
            .iter()
            .map(|rule| (rule.column, rule.row))
            .collect::<Vec<_>>();
        (
            generated,
            generated_input,
            generated_layout,
            generated_occupied,
        )
    } else {
        (image, input.clone(), layout, occupied)
    };

    let root = workspace_path(&state, &input.project_id)?;
    let export_slug = slug(&input.name);
    let directory = root.join("exports").join("godot").join(&export_slug);
    std::fs::create_dir_all(&directory)?;
    let texture_path = directory.join(format!("{export_slug}.png"));
    let resource_path = directory.join(format!("{export_slug}.tres"));
    export_image.save_with_format(&texture_path, ImageFormat::Png)?;
    let godot_texture_path = format!("res://exports/godot/{export_slug}/{export_slug}.png");
    let contents = godot_resource(
        &godot_texture_path,
        &export_input,
        &export_layout,
        &export_occupied,
    );
    std::fs::write(&resource_path, contents)?;

    Ok(TerrainExportResult {
        directory_path: directory.to_string_lossy().into_owned(),
        texture_path: texture_path.to_string_lossy().into_owned(),
        resource_path: resource_path.to_string_lossy().into_owned(),
        columns: export_layout.columns,
        rows: export_layout.rows,
        tile_count: export_layout.columns * export_layout.rows,
        occupied_tile_count: export_occupied.len() as u32,
        trailing_x: export_layout.trailing_x,
        trailing_y: export_layout.trailing_y,
        terrain_rule_count: export_input.terrain_rules.len() as u32,
        terrain_mode: mode.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        blob_masks, blob_rules, expand_blob_atlas, godot_resource, grid_layout, is_blob_mask, slug,
        validate_terrain_rules, GridLayout, BLOB_COLUMNS, TERRAIN_RULE_ROLES,
    };
    use crate::models::TerrainExportInput;
    use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

    fn input() -> TerrainExportInput {
        TerrainExportInput {
            project_id: "project".into(),
            worktree_id: "terrain".into(),
            asset_id: "atlas".into(),
            name: "Forest Ground".into(),
            tile_width: 16,
            tile_height: 16,
            margin_x: 1,
            margin_y: 1,
            separation_x: 1,
            separation_y: 1,
            include_empty: false,
            terrain_name: None,
            terrain_mode: None,
            terrain_rules: Vec::new(),
        }
    }

    #[test]
    fn calculates_bordered_atlas_grid_without_partial_cells() {
        let layout = grid_layout(69, 69, &input()).expect("grid should fit");
        assert_eq!(
            layout,
            GridLayout {
                columns: 4,
                rows: 4,
                trailing_x: 1,
                trailing_y: 1,
            }
        );
    }

    #[test]
    fn rejects_tiles_larger_than_the_atlas() {
        assert!(grid_layout(8, 8, &input()).is_err());
    }

    #[test]
    fn writes_native_godot_tileset_atlas_resource() {
        let layout = GridLayout {
            columns: 2,
            rows: 1,
            trailing_x: 0,
            trailing_y: 0,
        };
        let resource = godot_resource(
            "res://exports/godot/forest/forest.png",
            &input(),
            &layout,
            &[(0, 0), (1, 0)],
        );
        assert!(resource.contains("type=\"TileSet\""));
        assert!(resource.contains("texture_region_size = Vector2i(16, 16)"));
        assert!(resource.contains("0:0/0 = 0"));
        assert!(resource.contains("1:0/0 = 0"));
        assert_eq!(slug("  Forest Ground!  "), "forest-ground");
    }

    #[test]
    fn writes_nine_slice_terrain_metadata() {
        let mut input = input();
        input.terrain_name = Some("Ground \"A\"".into());
        input.terrain_rules = TERRAIN_RULE_ROLES
            .iter()
            .enumerate()
            .map(|(index, role)| crate::models::TerrainRuleInput {
                role: (*role).into(),
                column: (index % 3) as u32,
                row: (index / 3) as u32,
            })
            .collect();
        let layout = GridLayout {
            columns: 3,
            rows: 3,
            trailing_x: 0,
            trailing_y: 0,
        };
        let occupied = (0..3)
            .flat_map(|row| (0..3).map(move |column| (column, row)))
            .collect::<Vec<_>>();
        validate_terrain_rules(&input, &layout, &occupied).expect("rules should be valid");
        let resource = godot_resource("res://terrain.png", &input, &layout, &occupied);
        assert!(resource.contains("terrain_set_0/mode = 0"));
        assert!(resource.contains("terrain_0/name = \"Ground \\\"A\\\"\""));
        assert!(resource.contains("1:1/0/terrains_peering_bit/top_left_corner = 0"));
        assert!(resource.contains("0:0/0/terrains_peering_bit/bottom_right_corner = 0"));
    }

    #[test]
    fn enumerates_the_canonical_47_blob_masks() {
        let masks = blob_masks();
        assert_eq!(masks.len(), 47);
        assert_eq!(masks.first(), Some(&0));
        assert_eq!(masks.last(), Some(&255));
        assert!(!is_blob_mask(2));
        assert!(!is_blob_mask(128));
        assert!(is_blob_mask(255));
        let rules = blob_rules();
        assert_eq!(rules.len(), 47);
        assert_eq!(rules[46].role, "blob_255");
        assert_eq!((rules[46].column, rules[46].row), (6, 5));
    }

    #[test]
    fn expands_nine_source_roles_into_an_eight_by_six_blob_atlas() {
        let mut input = input();
        input.margin_x = 0;
        input.margin_y = 0;
        input.separation_x = 0;
        input.separation_y = 0;
        input.tile_width = 4;
        input.tile_height = 4;
        input.terrain_mode = Some("blob_47".into());
        input.terrain_rules = TERRAIN_RULE_ROLES
            .iter()
            .enumerate()
            .map(|(index, role)| crate::models::TerrainRuleInput {
                role: (*role).into(),
                column: (index % 3) as u32,
                row: (index / 3) as u32,
            })
            .collect();
        let mut pixels = RgbaImage::new(12, 12);
        for (index, _) in TERRAIN_RULE_ROLES.iter().enumerate() {
            let color = Rgba([index as u8 + 1, 0, 0, 255]);
            let column = index as u32 % 3;
            let row = index as u32 / 3;
            for y in row * 4..row * 4 + 4 {
                for x in column * 4..column * 4 + 4 {
                    pixels.put_pixel(x, y, color);
                }
            }
        }
        let expanded = expand_blob_atlas(&DynamicImage::ImageRgba8(pixels), &input)
            .expect("blob atlas should generate");
        assert_eq!(expanded.dimensions(), (32, 24));
        assert_eq!(expanded.get_pixel(0, 0), Rgba([1, 0, 0, 255]));
        let full_index = blob_masks().iter().position(|mask| *mask == 255).unwrap() as u32;
        let full_x = (full_index % BLOB_COLUMNS) * 4;
        let full_y = (full_index / BLOB_COLUMNS) * 4;
        assert_eq!(expanded.get_pixel(full_x, full_y), Rgba([5, 0, 0, 255]));
    }
}
