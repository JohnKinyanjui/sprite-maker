use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pivot {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterAnchor {
    pub slug: String,
    pub asset_id: String,
    pub relative_path: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub pivot: Pivot,
    pub baseline_y: u32,
    pub content_hash: String,
    pub promoted_at: String,
    /// side | top_down | isometric
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// w | e | n | s | nw | ne | sw | se
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing_confidence: Option<f64>,
    /// ok | auto_oriented | mismatch | uncertain
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterAnchorSummary {
    pub slug: String,
    pub asset_id: String,
    pub relative_path: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub pivot: Pivot,
    pub baseline_y: u32,
    pub promoted_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing_confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FacingCheckReport {
    pub slug: String,
    pub asset_id: String,
    pub view: String,
    pub detected_facing: String,
    pub canonical_facing: String,
    pub confidence: f64,
    pub status: String,
    pub auto_oriented: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrameScore {
    pub frame_index: u32,
    pub asset_id: String,
    pub motion_delta: f64,
    pub alpha_coverage: f64,
    pub size_drift: f64,
    pub pixel_delta: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrameScoreReport {
    pub animation_id: String,
    pub frame_count: u32,
    pub mean_motion_delta: f64,
    pub mean_score: f64,
    pub frames: Vec<AnimationFrameScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationDirectionMeta {
    pub animation_id: String,
    pub direction_family: String,
    pub facing: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirrored_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MirrorAnimationInput {
    pub animation_id: String,
    pub target_facing: String,
    #[serde(default)]
    pub source_facing: Option<String>,
    #[serde(default)]
    pub anchor_slug: Option<String>,
    #[serde(default)]
    pub rig_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MirrorAnimationResult {
    pub source_animation_id: String,
    pub mirrored_animation_id: String,
    pub target_facing: String,
    pub contract_report: SizeContractReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueDirectionSetInput {
    pub workspace_id: String,
    pub worktree_id: String,
    pub source_animation_id: String,
    pub anchor_slug: String,
    pub motion: String,
    /// "4" or "8"
    pub set: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DirectionSetResult {
    pub job_id: String,
    pub source_animation_id: String,
    pub mirrored_animation_ids: Vec<String>,
    #[serde(default)]
    pub pending_facings: Vec<String>,
    #[serde(default)]
    pub character_contract: Option<CharacterContractReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeAnimationInput {
    pub animation_id: String,
    pub anchor_slug: Option<String>,
    pub lock_first_frame: Option<bool>,
    pub shared_scale: Option<bool>,
    pub padding: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitStripInput {
    pub workspace_id: String,
    pub source_path: String,
    pub layout: String,
    pub frame_count: u32,
    pub columns: Option<u32>,
    pub recover_foreground: Option<bool>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StripScoreReport {
    pub suggested_frame_count: u32,
    pub frame_count_used: u32,
    pub inference_confidence: f64,
    pub grid_ink_detected: bool,
    pub min_cell_width: u32,
    pub mean_motion_delta: f64,
    pub segment_widths: Vec<u32>,
    pub layout_recommended: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitStripResult {
    pub frame_paths: Vec<String>,
    pub asset_ids: Vec<String>,
    pub relative_paths: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_frame_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_count_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_used: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameNudgeDelta {
    pub frame_index: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NudgeAnimationFramesInput {
    pub animation_id: String,
    pub deltas: Vec<FrameNudgeDelta>,
    pub apply_to_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SizeContractViolation {
    pub code: String,
    pub message: String,
    pub blocking: bool,
    pub frame_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SizeContractReport {
    pub animation_id: String,
    pub anchor_slug: Option<String>,
    pub passed: bool,
    pub review_status: String,
    pub violations: Vec<SizeContractViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetryAttempt {
    pub attempt: u32,
    pub phase: String,
    pub report: SizeContractReport,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterContractReport {
    pub workspace_id: String,
    pub worktree_id: String,
    pub anchor_slug: String,
    pub passed: bool,
    pub animation_count: u32,
    pub animation_reports: Vec<SizeContractReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_facing_violations: Vec<SizeContractViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetryResult {
    pub animation_id: String,
    pub passed: bool,
    pub attempts: Vec<ContractRetryAttempt>,
    pub corrective_prompt: Option<String>,
    pub generation_request_id: Option<String>,
    pub job_id: Option<String>,
    pub next_step: Option<String>,
    pub final_report: SizeContractReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueContractRetryInput {
    pub animation_id: String,
    pub conversation_id: String,
    pub anchor_slug: Option<String>,
    pub max_ai_attempts: Option<u32>,
    pub max_deterministic_passes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetrySizeContractInput {
    pub animation_id: String,
    pub anchor_slug: Option<String>,
    pub conversation_id: Option<String>,
    pub regenerate: Option<bool>,
    pub max_deterministic_passes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeContractRetryInput {
    pub animation_id: String,
    pub source_path: String,
    pub frame_count: u32,
    pub anchor_slug: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardenAnimationOptions {
    #[serde(default)]
    pub clean_alpha: Option<bool>,
    #[serde(default)]
    pub normalize: Option<bool>,
    #[serde(default)]
    pub snap_grid: Option<bool>,
    #[serde(default)]
    pub grid_size: Option<u32>,
    #[serde(default)]
    pub split_layout: Option<String>,
    #[serde(default)]
    pub queue_contract_retry: Option<bool>,
    /// After harden passes, derive a mirrored facing (e.g. "e" from canonical "w").
    #[serde(default)]
    pub derive_mirrored_facing: Option<String>,
    #[serde(default)]
    pub quantize_palette: Option<bool>,
    #[serde(default)]
    pub palette_color_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardenAnimationReport {
    pub animation_id: String,
    pub steps: Vec<String>,
    pub contract_report: SizeContractReport,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractVideoFramesInput {
    pub workspace_id: String,
    pub video_path: String,
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportCharacterPackInput {
    pub workspace_id: String,
    pub worktree_id: String,
    pub destination: String,
    #[serde(default)]
    pub anchor_slug: Option<String>,
    #[serde(default)]
    pub metadata_format: Option<String>,
    #[serde(default)]
    pub include_animated_previews: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterPackAnimationEntry {
    pub animation_id: String,
    pub name: String,
    pub direction_family: String,
    pub facing: String,
    pub png_path: String,
    pub metadata_path: String,
    pub preview_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_animated: Option<String>,
    pub frame_count: u32,
    pub mirrored_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterPackExportResult {
    pub directory_path: String,
    pub manifest_path: String,
    pub anchor_path: String,
    pub animation_count: u32,
    pub animations: Vec<CharacterPackAnimationEntry>,
    pub character_contract_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MotionPreset {
    pub id: String,
    pub label: String,
    pub motion: String,
    #[serde(default = "default_motion_category")]
    pub category: String,
    pub frame_count: u32,
    pub fps: f64,
    pub looping: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_motion_category() -> String {
    "locomotion".into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MotionPresetCatalog {
    pub version: u32,
    pub presets: Vec<MotionPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueMotionBatchInput {
    pub workspace_id: String,
    pub worktree_id: String,
    pub anchor_slug: String,
    /// Motion ids from the catalog. Empty uses all enabled presets.
    #[serde(default)]
    pub motions: Vec<String>,
    /// "4" or "8"
    pub set: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub harden_after: Option<bool>,
    /// Used for frame count / fps when generating a missing canonical west-facing strip.
    #[serde(default)]
    pub seed_animation_id: Option<String>,
    #[serde(default)]
    pub only_missing: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListMissingMotionsResult {
    pub anchor_slug: String,
    pub canonical_facing: String,
    pub missing_preset_ids: Vec<String>,
    pub missing_motions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportAnimationPreviewInput {
    pub animation_id: String,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportAnimationPreviewResult {
    pub gif_path: String,
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSession {
    pub session_id: String,
    pub worktree_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation_id: Option<String>,
    pub manifest_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSessionIndex {
    pub version: u32,
    pub sessions: Vec<GenerationSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MotionBatchProgressEntry {
    pub motion: String,
    pub phase: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MotionBatchResult {
    pub job_id: String,
    pub motions: Vec<String>,
    #[serde(default)]
    pub entries: Vec<MotionBatchProgressEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterProfile {
    pub version: u32,
    pub anchor_slug: String,
    pub asset_id: String,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    pub palette: Vec<String>,
    pub d_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rig_id: Option<String>,
    #[serde(default)]
    pub direction_families: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportCharacterProfileInput {
    pub workspace_id: String,
    pub slug: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportCharacterProfileInput {
    pub workspace_id: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegionMaskRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BrushStamp {
    pub x: u32,
    pub y: u32,
    pub radius: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueRegionRegenInput {
    pub animation_id: String,
    pub frame_index: u32,
    pub regions: Vec<RegionMaskRect>,
    pub conversation_id: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub brush_strokes: Vec<BrushStamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubsampleVideoFramesInput {
    pub workspace_id: String,
    pub asset_ids: Vec<String>,
    #[serde(default)]
    pub min_delta: Option<f64>,
    #[serde(default)]
    pub stride: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubsampleVideoFramesResult {
    pub kept_asset_ids: Vec<String>,
    pub dropped_asset_ids: Vec<String>,
    pub kept_indices: Vec<u32>,
    pub dropped_indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuantizeWorktreePaletteInput {
    pub workspace_id: String,
    #[serde(default)]
    pub worktree_id: Option<String>,
    #[serde(default)]
    pub anchor_slug: Option<String>,
    #[serde(default)]
    pub color_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedPaletteReport {
    pub unique_colors_before: u32,
    pub unique_colors_after: u32,
    pub frames_touched: u32,
    pub palette: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaintFrameAlphaInput {
    pub asset_id: String,
    pub strokes: Vec<BrushStamp>,
    /// erase | restore
    pub mode: String,
    #[serde(default)]
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaintFrameAlphaResult {
    pub asset_id: String,
    pub version_id: String,
    pub opaque_pixel_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreAssetVersionInput {
    pub asset_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegionRegenResult {
    pub job_id: String,
    pub frame_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductionScoreReport {
    pub workspace_id: String,
    pub worktree_id: String,
    pub anchor_slug: String,
    pub overall_score: f64,
    pub size_contract_score: f64,
    pub character_contract_score: f64,
    pub quality_score: f64,
    pub animation_count: u32,
    pub animations_with_quality: u32,
    pub size_contract_pass_rate: f64,
    pub character_contract_passed: bool,
    pub cross_facing_violation_count: u32,
}
