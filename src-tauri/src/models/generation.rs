use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstallResult {
    pub detail: String,
    pub installed: bool,
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
    /// When true, the app will rig and render natively after the master is saved.
    #[serde(default)]
    pub native_rig_master_only: bool,
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
    /// Workspace-relative mask-rig JSON (legacy Python path).
    #[serde(default)]
    pub rig: Option<String>,
    /// SQLite native rig id when rendered by the Rust rig engine.
    #[serde(default)]
    pub rig_id: Option<String>,
    /// Workspace-relative source master PNG used by the rig.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub quality: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirrored_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRigSpec {
    pub relative_path: String,
    pub name: String,
    pub source: Option<String>,
    pub fps: f64,
    pub frame_count: u32,
    pub updated_at: String,
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
