use serde::{Deserialize, Serialize};

use super::media::Animation;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundJob {
    pub id: String,
    pub project_id: String,
    pub worktree_id: Option<String>,
    pub kind: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub status: String,
    pub progress: f64,
    pub stage: String,
    pub error_message: Option<String>,
    pub cancel_requested: bool,
    pub result_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobEvent {
    pub job: BackgroundJob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheet {
    pub id: String,
    pub project_id: String,
    pub worktree_id: Option<String>,
    pub animation_id: String,
    pub name: String,
    pub layout: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub padding: u32,
    pub spacing: u32,
    pub rows: u32,
    pub columns: u32,
    pub scale: u32,
    pub transparent: bool,
    pub alignment: String,
    pub pivot_x: f64,
    pub pivot_y: f64,
    pub png_path: String,
    pub metadata_path: String,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheetInput {
    pub project_id: String,
    pub worktree_id: Option<String>,
    pub animation_id: String,
    pub name: String,
    pub layout: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub padding: u32,
    pub spacing: u32,
    pub columns: u32,
    pub scale: u32,
    pub transparent: bool,
    pub alignment: String,
    pub pivot_x: f64,
    pub pivot_y: f64,
    pub metadata_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VfxEffect {
    pub id: String,
    pub project_id: String,
    pub worktree_id: String,
    pub animation_id: Option<String>,
    pub name: String,
    pub effect_type: String,
    pub blend_mode: String,
    pub center_x: f64,
    pub center_y: f64,
    pub opacity: f64,
    pub looping: bool,
    pub fps: f64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProceduralVfxInput {
    pub project_id: String,
    pub worktree_id: String,
    pub name: String,
    pub effect_type: String,
    pub blend_mode: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub fps: u32,
    pub looping: bool,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityCheck {
    pub id: String,
    pub report_id: String,
    pub position: u32,
    pub check_type: String,
    pub frame_index: Option<u32>,
    pub comparison_frame_index: Option<u32>,
    pub severity: String,
    pub score: f64,
    pub message: String,
    pub metric_value: Option<f64>,
    pub metric_unit: Option<String>,
    pub repair_action: Option<String>,
    pub acknowledged: bool,
    pub ignored: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub id: String,
    pub project_id: String,
    pub worktree_id: Option<String>,
    pub animation_id: String,
    pub job_id: Option<String>,
    pub status: String,
    pub overall_score: f64,
    pub character_consistency_score: f64,
    pub motion_continuity_score: f64,
    pub frame_alignment_score: f64,
    pub weapon_consistency_score: f64,
    pub loop_quality_score: f64,
    pub transparency_score: f64,
    pub frame_count: u32,
    pub analyzer_version: String,
    pub checks: Vec<QualityCheck>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameOptimizationInput {
    pub animation_id: String,
    #[serde(default = "default_optimization_changes")]
    pub max_changes: u32,
}

const fn default_optimization_changes() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameOptimizationResult {
    pub animation: Animation,
    pub removed_frames: u32,
    pub inserted_frames: u32,
    pub replaced_frames: u32,
    pub summary: String,
}
