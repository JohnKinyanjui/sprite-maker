use serde::{Deserialize, Serialize};

use super::generation::MotionPlan;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub category: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub has_alpha: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetUsage {
    pub animation_names: Vec<String>,
    pub anchor_slugs: Vec<String>,
    pub sprite_sheet_items: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetVersion {
    pub id: String,
    pub asset_id: String,
    pub version_number: u32,
    pub parent_version_id: Option<String>,
    pub generation_id: Option<String>,
    pub path: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub has_alpha: bool,
    pub content_hash: String,
    pub change_kind: String,
    pub available: bool,
    pub selected: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImage {
    pub id: String,
    pub project_id: String,
    pub worktree_id: String,
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub category: String,
    pub notes: Option<String>,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub id: String,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub name: String,
    pub fps: f64,
    pub looping: bool,
    pub frames: Vec<AnimationFrame>,
    pub motion_plan: Option<MotionPlan>,
    pub review_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrame {
    pub asset_id: String,
    pub duration_ms: Option<u32>,
    #[serde(default)]
    pub offset_x: i32,
    #[serde(default)]
    pub offset_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTemplatePhase {
    pub id: String,
    pub template_id: String,
    pub position: u32,
    pub name: String,
    pub description: String,
    pub frame_count: u32,
    pub timing_weight: f64,
    pub movement_offset_x: f64,
    pub movement_offset_y: f64,
    pub weapon_position: Option<String>,
    pub pose_reference_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTemplate {
    pub id: String,
    pub project_id: String,
    pub source_animation_id: Option<String>,
    pub name: String,
    pub intent: String,
    pub motion_description: String,
    pub direction: String,
    pub looping: bool,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub pivot_x: Option<f64>,
    pub pivot_y: Option<f64>,
    pub frame_mode: String,
    pub preferred_frames: u32,
    pub min_frames: u32,
    pub max_frames: u32,
    pub generation_prompt: String,
    pub negative_prompt: String,
    pub weapon_behavior: Option<String>,
    pub phases: Vec<AnimationTemplatePhase>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateApplication {
    pub template: AnimationTemplate,
    pub target_asset: Asset,
    pub motion_plan: MotionPlan,
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationInput {
    pub id: Option<String>,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub name: String,
    pub fps: f64,
    pub looping: bool,
    pub frames: Vec<AnimationFrame>,
    pub motion_plan: Option<MotionPlan>,
    pub review_status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub png_path: String,
    pub metadata_path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainRuleInput {
    pub role: String,
    pub column: u32,
    pub row: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainExportInput {
    pub project_id: String,
    pub worktree_id: String,
    pub asset_id: String,
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub margin_x: u32,
    pub margin_y: u32,
    pub separation_x: u32,
    pub separation_y: u32,
    pub include_empty: bool,
    #[serde(default)]
    pub terrain_name: Option<String>,
    #[serde(default)]
    pub terrain_mode: Option<String>,
    #[serde(default)]
    pub terrain_rules: Vec<TerrainRuleInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainExportResult {
    pub directory_path: String,
    pub texture_path: String,
    pub resource_path: String,
    pub columns: u32,
    pub rows: u32,
    pub tile_count: u32,
    pub occupied_tile_count: u32,
    pub trailing_x: u32,
    pub trailing_y: u32,
    pub terrain_rule_count: u32,
    pub terrain_mode: String,
}
