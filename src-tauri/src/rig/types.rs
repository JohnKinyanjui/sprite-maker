use rmcp::schemars;
use serde::{Deserialize, Serialize};

pub const MORPHOLOGIES: [&str; 6] = [
    "biped",
    "quadruped",
    "winged",
    "serpentine",
    "object",
    "amorphous",
];

pub(crate) fn normalize_morphology(value: Option<&str>) -> String {
    let lowered = value.unwrap_or("biped").trim().to_ascii_lowercase();
    if MORPHOLOGIES.contains(&lowered.as_str()) {
        lowered
    } else {
        "biped".to_string()
    }
}

fn default_one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigPoint {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub confidence: f64,
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigBone {
    pub id: String,
    pub name: String,
    pub start_point: String,
    pub end_point: String,
    pub radius: f64,
    #[serde(default)]
    pub parent: Option<String>,
    pub z: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigTransform {
    pub bone: String,
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    #[serde(default)]
    pub rotate: f64,
    #[serde(default = "default_one")]
    pub scale_x: f64,
    #[serde(default = "default_one")]
    pub scale_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigContact {
    pub bone: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_one")]
    pub bend: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigFrame {
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub root_dx: f64,
    #[serde(default)]
    pub root_dy: f64,
    #[serde(default)]
    pub transforms: Vec<RigTransform>,
    #[serde(default)]
    pub contacts: Vec<RigContact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct RigSpec {
    #[serde(default)]
    pub(super) points: Vec<RigPoint>,
    #[serde(default)]
    pub(super) bones: Vec<RigBone>,
    #[serde(default)]
    pub(super) frames: Vec<RigFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rig {
    pub id: String,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub asset_id: Option<String>,
    pub name: String,
    pub morphology: String,
    pub fps: f64,
    pub looping: bool,
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub frames: Vec<RigFrame>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RigInput {
    pub id: Option<String>,
    pub workspace_id: String,
    pub worktree_id: Option<String>,
    pub asset_id: Option<String>,
    pub name: String,
    pub morphology: String,
    pub fps: f64,
    pub looping: bool,
    #[serde(default)]
    pub points: Vec<RigPoint>,
    #[serde(default)]
    pub bones: Vec<RigBone>,
    #[serde(default)]
    pub frames: Vec<RigFrame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigSuggestion {
    pub morphology: String,
    pub points: Vec<RigPoint>,
    pub bones: Vec<RigBone>,
    pub frames: Vec<RigFrame>,
    pub reasoning: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigRenderResult {
    pub animation: crate::models::Animation,
    pub frame_paths: Vec<String>,
    pub asset_ids: Vec<String>,
    pub rig_id: String,
}

pub(super) struct TemplatePoint {
    pub(super) name: &'static str,
    pub(super) kind: &'static str,
    pub(super) nx: f64,
    pub(super) ny: f64,
}

pub(super) struct TemplateBone {
    pub(super) name: &'static str,
    pub(super) start: &'static str,
    pub(super) end: &'static str,
    pub(super) radius_factor: f64,
    pub(super) parent: Option<&'static str>,
    pub(super) z: i64,
}

pub(super) struct Template {
    pub(super) points: &'static [TemplatePoint],
    pub(super) bones: &'static [TemplateBone],
}
