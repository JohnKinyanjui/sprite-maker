use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectBackup {
    pub format_version: u32,
    pub project_id: String,
    pub project_name: String,
    pub source_path: String,
    pub backup_path: String,
    pub created_at: String,
    pub file_count: u64,
    pub total_bytes: u64,
}

/// One-round-trip sidebar payload: every project plus the active project's
/// worktrees and chats, read under a single database lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarSnapshot {
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub conversations: Vec<Conversation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub slug: String,
    pub kind: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub title: String,
    pub provider: String,
    pub provider_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub kind: String,
    pub content: String,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
}

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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationFrame {
    pub asset_id: String,
    pub duration_ms: Option<u32>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub installed: bool,
    pub executable: Option<String>,
    pub status: String,
    pub detail: String,
    pub modes: Vec<ProviderMode>,
    pub capabilities: ProviderCapabilities,
    #[serde(default)]
    pub configurable: bool,
    #[serde(default)]
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageProviderInput {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionTest {
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub text_input: bool,
    pub image_input: bool,
    pub multiple_image_input: bool,
    pub image_editing: bool,
    pub masks: bool,
    pub transparency: bool,
    pub structured_output: bool,
    pub video_animation: bool,
    pub image_to_image: bool,
    pub maximum_reference_images: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMode {
    pub id: String,
    pub label: String,
    pub description: String,
    pub default_reasoning_effort: String,
    pub reasoning_efforts: Vec<String>,
}

fn default_frame_mode() -> String {
    "auto".into()
}

fn default_min_frames() -> u32 {
    8
}

fn default_max_frames() -> u32 {
    12
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationOptions {
    pub quality: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub fps: u32,
    #[serde(default = "default_frame_mode")]
    pub frame_mode: String,
    #[serde(default = "default_min_frames")]
    pub min_frames: u32,
    #[serde(default = "default_max_frames")]
    pub max_frames: u32,
    #[serde(default = "default_false")]
    pub allow_interpolation: bool,
    #[serde(default = "default_true")]
    pub allow_auto_adjust: bool,
}

#[cfg(test)]
mod generation_option_tests {
    use super::GenerationOptions;

    #[test]
    fn omitted_frame_policy_defaults_to_full_auto_range() {
        let options: GenerationOptions =
            serde_json::from_str(r#"{"quality":"mid","width":64,"height":64,"frames":6,"fps":8}"#)
                .expect("generation options should deserialize");

        assert_eq!(options.frame_mode, "auto");
        assert_eq!(options.min_frames, 8);
        assert_eq!(options.max_frames, 12);
        assert!(options.allow_auto_adjust);
        assert!(!options.allow_interpolation);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionPhase {
    pub name: String,
    pub description: String,
    pub frame_count: u32,
    pub timing_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionPlan {
    pub frame_mode: String,
    pub selected_frame_count: u32,
    pub minimum_frame_count: u32,
    pub maximum_frame_count: u32,
    pub fps: u32,
    pub looping: bool,
    pub allow_interpolation: bool,
    pub allow_auto_adjust: bool,
    pub explanation: String,
    pub phases: Vec<MotionPhase>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestOptions {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub command: Option<String>,
    pub generation: Option<GenerationOptions>,
    #[serde(default)]
    pub reference_ids: Vec<String>,
    pub image_provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEvent {
    pub request_id: String,
    pub conversation_id: String,
    pub event_type: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationManifest {
    #[serde(default)]
    pub kind: Option<String>,
    pub name: String,
    pub category: String,
    pub fps: f64,
    pub files: Vec<String>,
    #[serde(alias = "generation_time", alias = "generated_at")]
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetPack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub style: String,
    pub kind: String,
    pub files: Vec<String>,
    #[serde(alias = "created_at")]
    pub created_at: String,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub png_path: String,
    pub metadata_path: String,
    pub width: u32,
    pub height: u32,
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

#[cfg(test)]
mod tests {
    use super::GenerationManifest;

    #[test]
    fn generation_manifest_accepts_current_and_legacy_timestamp_keys() {
        for timestamp_key in ["generatedAt", "generation_time", "generated_at"] {
            let json = format!(
                r#"{{"name":"walk","category":"characters","fps":8,"files":["assets/characters/walk_01.png"],"{timestamp_key}":"2026-08-09T12:24:00Z"}}"#
            );
            let manifest: GenerationManifest =
                serde_json::from_str(&json).expect("manifest timestamp key should parse");
            assert_eq!(manifest.generated_at, "2026-08-09T12:24:00Z");
        }
    }
}
