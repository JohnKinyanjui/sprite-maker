use crate::{
    assets::list_assets_inner,
    error::CommandResult,
    jobs::{load_job, queue_procedural_vfx_inner, queue_sprite_sheet_inner},
    models::{HardenAnimationOptions, ProceduralVfxInput, ProviderStatus, SpriteSheetInput},
    packs::list_asset_packs_inner,
    providers::cancel_provider_request_inner,
    AppState,
};
use rmcp::{
    handler::server::wrapper::Parameters, schemars, tool, tool_handler, tool_router,
    transport::stdio, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub(crate) const DEFAULT_PROVIDER: &str = "codex";

#[derive(Clone)]
pub struct SpriteStudioMcp {
    pub(crate) state: AppState,
    pub(crate) db_path: PathBuf,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        tauri::async_runtime::set(tokio::runtime::Handle::current());
        let (state, db_path) = AppState::open_headless()?;
        crate::workspace::initialize_ffmpeg_runtime();
        let server = SpriteStudioMcp { state, db_path };
        let service = server.serve(stdio()).await?;
        let _ = service.waiting().await?;
        Ok(())
    })
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct OpenWorkspaceParams {
    pub(crate) path: String,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct EnsureConversationParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    pub(crate) provider: Option<String>,
    #[serde(rename = "stylePreset")]
    pub(crate) style_preset: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AttachReferencesParams {
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: String,
    pub(crate) paths: Option<Vec<String>>,
    #[serde(rename = "referenceIds")]
    pub(crate) reference_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct GenerateParams {
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: String,
    pub(crate) prompt: String,
    pub(crate) generation: Option<McpGenerationOptions>,
    pub(crate) command: Option<String>,
    #[serde(rename = "imageProviderId")]
    pub(crate) image_provider_id: Option<String>,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct McpGenerationOptions {
    pub(crate) quality: Option<String>,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) frames: Option<u32>,
    pub(crate) fps: Option<u32>,
    #[serde(rename = "frameMode")]
    pub(crate) frame_mode: Option<String>,
    #[serde(rename = "minFrames")]
    pub(crate) min_frames: Option<u32>,
    #[serde(rename = "maxFrames")]
    pub(crate) max_frames: Option<u32>,
    #[serde(rename = "allowInterpolation", default)]
    pub(crate) allow_interpolation: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct RequestIdParams {
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkspaceIdParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ExportParams {
    pub(crate) kind: String,
    pub(crate) id: String,
    pub(crate) destination: Option<String>,
    #[serde(rename = "exportFormat")]
    pub(crate) export_format: Option<String>,
    #[serde(rename = "projectId")]
    pub(crate) project_id: Option<String>,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(rename = "tileWidth")]
    pub(crate) tile_width: Option<u32>,
    #[serde(rename = "tileHeight")]
    pub(crate) tile_height: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct JobIdParams {
    #[serde(rename = "jobId")]
    pub(crate) job_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QualityParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    pub(crate) analyze: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct PromoteAnchorParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "assetId")]
    pub(crate) asset_id: String,
    pub(crate) slug: Option<String>,
    #[serde(rename = "autoOrient")]
    pub(crate) auto_orient: Option<bool>,
    pub(crate) view: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AnchorSlugParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    pub(crate) slug: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AssetIdParams {
    #[serde(rename = "assetId")]
    pub(crate) asset_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SuggestRigParams {
    #[serde(rename = "assetId")]
    pub(crate) asset_id: String,
    pub(crate) morphology: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct MirrorAnimationParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "targetFacing")]
    pub(crate) target_facing: String,
    #[serde(rename = "sourceFacing")]
    pub(crate) source_facing: Option<String>,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(rename = "rigId")]
    pub(crate) rig_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QueueDirectionSetParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: String,
    #[serde(rename = "sourceAnimationId")]
    pub(crate) source_animation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: String,
    pub(crate) motion: String,
    pub(crate) set: String,
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct CleanAlphaParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SnapToPixelGridParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "gridSize")]
    pub(crate) grid_size: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct HardenAnimationParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(rename = "sourcePath")]
    pub(crate) source_path: Option<String>,
    #[serde(rename = "frameCount")]
    pub(crate) frame_count: Option<u32>,
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: Option<String>,
    pub(crate) options: Option<HardenAnimationOptions>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct NormalizeAnimationParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(rename = "lockFirstFrame")]
    pub(crate) lock_first_frame: Option<bool>,
    #[serde(rename = "sharedScale")]
    pub(crate) shared_scale: Option<bool>,
    pub(crate) padding: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ExtractVideoFramesParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "videoPath")]
    pub(crate) video_path: String,
    pub(crate) fps: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SplitStripParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "sourcePath")]
    pub(crate) source_path: String,
    pub(crate) layout: String,
    #[serde(rename = "frameCount")]
    pub(crate) frame_count: u32,
    pub(crate) columns: Option<u32>,
    #[serde(rename = "recoverForeground")]
    pub(crate) recover_foreground: Option<bool>,
    pub(crate) category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ScoreStripParams {
    #[serde(rename = "sourcePath")]
    pub(crate) source_path: String,
    #[serde(rename = "frameCount")]
    pub(crate) frame_count: Option<u32>,
    pub(crate) layout: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct AlignFramesParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    pub(crate) deltas: Vec<crate::models::FrameNudgeDelta>,
    #[serde(rename = "applyToAll")]
    pub(crate) apply_to_all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SizeContractParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct CharacterContractParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct SetReviewStatusParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    pub(crate) status: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct RetrySizeContractParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: Option<String>,
    pub(crate) regenerate: Option<bool>,
    #[serde(rename = "maxDeterministicPasses")]
    pub(crate) max_deterministic_passes: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct QueueContractRetryParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "conversationId")]
    pub(crate) conversation_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(rename = "maxAiAttempts")]
    pub(crate) max_ai_attempts: Option<u32>,
    #[serde(rename = "maxDeterministicPasses")]
    pub(crate) max_deterministic_passes: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ProductionScoreParams {
    #[serde(rename = "workspaceId")]
    pub(crate) workspace_id: String,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: String,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ScoreAnimationFramesParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct FinalizeContractRetryParams {
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    #[serde(rename = "sourcePath")]
    pub(crate) source_path: String,
    #[serde(rename = "frameCount")]
    pub(crate) frame_count: u32,
    #[serde(rename = "anchorSlug")]
    pub(crate) anchor_slug: Option<String>,
    #[serde(default)]
    pub(crate) layout: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusPayload {
    pub(crate) version: &'static str,
    pub(crate) db_path: String,
    pub(crate) db_ok: bool,
    pub(crate) providers: Vec<ProviderStatus>,
}

#[tool_router]
impl SpriteStudioMcp {
    #[tool(
        description = "Sprite Studio version, database path, and detected CLI/image providers. Never returns API keys."
    )]
    fn studio_status(&self) -> String {
        json_result(super::handlers::studio_status(&self.state, &self.db_path))
    }

    #[tool(description = "Open or create a Sprite Studio workspace at a local folder path.")]
    fn open_workspace(&self, Parameters(params): Parameters<OpenWorkspaceParams>) -> String {
        json_result(super::handlers::open_or_create_workspace(
            &self.state,
            params,
        ))
    }

    #[tool(
        description = "Create a chat in a workspace. Defaults to Codex so a Cursor MCP client does not nest Cursor CLI."
    )]
    fn ensure_conversation(
        &self,
        Parameters(params): Parameters<EnsureConversationParams>,
    ) -> String {
        json_result(super::handlers::ensure_conversation(&self.state, params))
    }

    #[tool(description = "Attach local image files or existing reference IDs to a conversation.")]
    fn attach_references(&self, Parameters(params): Parameters<AttachReferencesParams>) -> String {
        json_result(super::handlers::attach_references(&self.state, params))
    }

    #[tool(
        description = "Start a harness-aware sprite generation. Returns requestId; poll with get_generation."
    )]
    fn generate(&self, Parameters(params): Parameters<GenerateParams>) -> String {
        json_result(super::handlers::generate(&self.state, params))
    }

    #[tool(
        description = "Poll a generation started in this MCP process. Cancel only works for jobs started here."
    )]
    fn get_generation(&self, Parameters(params): Parameters<RequestIdParams>) -> String {
        json_result(super::handlers::get_generation(
            &self.state,
            &params.request_id,
        ))
    }

    #[tool(description = "Cancel a generation started in this MCP process.")]
    fn cancel_generation(&self, Parameters(params): Parameters<RequestIdParams>) -> String {
        json_result(
            cancel_provider_request_inner(&params.request_id, &self.state)
                .map(|_| serde_json::json!({ "cancelled": true, "requestId": params.request_id })),
        )
    }

    #[tool(
        description = "Scan the latest generation manifest and list workspace-relative assets/ files."
    )]
    fn list_artifacts(&self, Parameters(params): Parameters<WorkspaceIdParams>) -> String {
        json_result(super::handlers::list_artifacts(
            &self.state,
            &params.workspace_id,
        ))
    }

    #[tool(
        description = "Export an asset PNG, animation spritesheet, or Godot tileset. kind is asset, animation, or godot_tileset."
    )]
    fn export(&self, Parameters(params): Parameters<ExportParams>) -> String {
        json_result(super::handlers::export_item(&self.state, params))
    }

    #[tool(description = "Queue a sprite sheet composite job.")]
    fn queue_sprite_sheet(&self, Parameters(input): Parameters<McpSpriteSheetInput>) -> String {
        json_result(queue_sprite_sheet_inner(
            input.into_input(),
            None,
            &self.state,
        ))
    }

    #[tool(description = "Queue a procedural VFX job. Requires a VFX worktree.")]
    fn queue_procedural_vfx(&self, Parameters(input): Parameters<McpProceduralVfxInput>) -> String {
        json_result(queue_procedural_vfx_inner(
            input.into_input(),
            None,
            &self.state,
        ))
    }

    #[tool(description = "Load a background job (sprite sheet, VFX, quality analysis, or contract retry).")]
    fn get_job(&self, Parameters(params): Parameters<JobIdParams>) -> String {
        json_result(load_job(&self.state, &params.job_id))
    }

    #[tool(
        description = "Get the latest quality report for an animation, or queue analysis when analyze is true."
    )]
    fn quality_report(&self, Parameters(params): Parameters<QualityParams>) -> String {
        json_result(super::handlers::quality_report(&self.state, params))
    }

    #[tool(description = "List indexed assets in a workspace.")]
    fn list_assets(&self, Parameters(params): Parameters<WorkspaceIdParams>) -> String {
        json_result(list_assets_inner(&params.workspace_id, &self.state))
    }

    #[tool(description = "List asset packs in a workspace.")]
    fn list_packs(&self, Parameters(params): Parameters<WorkspaceIdParams>) -> String {
        json_result(list_asset_packs_inner(&params.workspace_id, &self.state))
    }

    #[tool(description = "Promote an asset as the canonical character anchor for size and baseline contract.")]
    fn promote_anchor(&self, Parameters(params): Parameters<PromoteAnchorParams>) -> String {
        json_result(super::handlers::promote_anchor(&self.state, params))
    }

    #[tool(description = "Read-only facing report for a promoted anchor (side/top-down heuristics).")]
    fn check_anchor_facing(&self, Parameters(params): Parameters<AnchorSlugParams>) -> String {
        json_result(super::handlers::check_anchor_facing(&self.state, params))
    }

    #[tool(description = "Flip a promoted anchor to its canonical facing (side view → west).")]
    fn orient_anchor(&self, Parameters(params): Parameters<AnchorSlugParams>) -> String {
        json_result(super::handlers::orient_anchor(&self.state, params))
    }

    #[tool(description = "Read a promoted character anchor sidecar by slug.")]
    fn get_anchor(&self, Parameters(params): Parameters<AnchorSlugParams>) -> String {
        json_result(super::handlers::get_promoted_anchor(
            &self.state,
            &params.workspace_id,
            &params.slug,
        ))
    }

    #[tool(description = "List promoted character anchors in a workspace.")]
    fn list_anchors(&self, Parameters(params): Parameters<WorkspaceIdParams>) -> String {
        json_result(super::handlers::list_promoted_anchors(
            &self.state,
            &params.workspace_id,
        ))
    }

    #[tool(description = "Remove fringe and semi-opaque halos from every frame PNG in an animation.")]
    fn clean_alpha(&self, Parameters(params): Parameters<CleanAlphaParams>) -> String {
        json_result(super::handlers::clean_alpha(&self.state, params))
    }

    #[tool(
        description = "Snap animation metadata offsets to a pixel grid. When gridSize > 1, also quantizes opaque RGB channels in frame PNGs."
    )]
    fn snap_to_pixel_grid(&self, Parameters(params): Parameters<SnapToPixelGridParams>) -> String {
        json_result(super::handlers::snap_to_pixel_grid(&self.state, params))
    }

    #[tool(
        description = "Idempotent hardening chain: optional split, clean_alpha, normalize, snap grid, contract check, optional queue_contract_retry."
    )]
    fn harden_animation(&self, Parameters(params): Parameters<HardenAnimationParams>) -> String {
        json_result(super::handlers::harden_animation(&self.state, params))
    }

    #[tool(description = "Normalize animation frames to a fixed-cell contract using a promoted anchor.")]
    fn normalize_animation(
        &self,
        Parameters(params): Parameters<NormalizeAnimationParams>,
    ) -> String {
        json_result(super::handlers::normalize_animation(&self.state, params))
    }

    #[tool(
        description = "Split a sprite strip or grid into individual recovered frames. Layout auto infers frameCount when it is 0."
    )]
    fn split_strip(&self, Parameters(params): Parameters<SplitStripParams>) -> String {
        json_result(super::handlers::split_strip(&self.state, params))
    }

    #[tool(
        description = "Score a sprite strip before import: inferred frame count, grid ink, motion delta, and warnings."
    )]
    fn score_strip(&self, Parameters(params): Parameters<ScoreStripParams>) -> String {
        json_result(super::handlers::score_strip(&self.state, params))
    }

    #[tool(
        description = "Extract PNG frames from a video with bundled or system ffmpeg. Returns assetIds like split_strip."
    )]
    fn extract_video_frames(
        &self,
        Parameters(params): Parameters<ExtractVideoFramesParams>,
    ) -> String {
        json_result(super::handlers::extract_video_frames(&self.state, params))
    }

    #[tool(description = "Apply per-frame nudge offsets to an animation without rewriting PNGs.")]
    fn align_frames(&self, Parameters(params): Parameters<AlignFramesParams>) -> String {
        json_result(super::handlers::align_frames(&self.state, params))
    }

    #[tool(description = "Check whether an animation satisfies the promoted anchor size contract.")]
    fn check_size_contract(&self, Parameters(params): Parameters<SizeContractParams>) -> String {
        json_result(super::handlers::check_size_contract(&self.state, params))
    }

    #[tool(
        description = "Check every animation in a character worktree against the promoted anchor contract."
    )]
    fn check_character_contract(
        &self,
        Parameters(params): Parameters<CharacterContractParams>,
    ) -> String {
        json_result(super::handlers::check_character_contract(&self.state, params))
    }

    #[tool(
        description = "Retry a failing size contract: deterministic normalize/nudge passes, then optional AI strip regeneration with a corrective prompt."
    )]
    fn retry_size_contract(
        &self,
        Parameters(params): Parameters<RetrySizeContractParams>,
    ) -> String {
        json_result(super::handlers::retry_size_contract(&self.state, params))
    }

    #[tool(
        description = "Queue an autonomous contract-retry job: deterministic repair, AI strip regeneration with polling, import, and re-check until pass or max attempts."
    )]
    fn queue_contract_retry(
        &self,
        Parameters(params): Parameters<QueueContractRetryParams>,
    ) -> String {
        json_result(super::handlers::queue_contract_retry(&self.state, params))
    }

    #[tool(
        description = "After AI regenerates a strip, import it with profile split, normalize, and re-check the size contract."
    )]
    fn finalize_contract_retry(
        &self,
        Parameters(params): Parameters<FinalizeContractRetryParams>,
    ) -> String {
        json_result(super::handlers::finalize_contract_retry(&self.state, params))
    }

    #[tool(description = "Derive a mirrored facing animation (e.g. walk-e from walk-w) without AI regeneration.")]
    fn mirror_animation(&self, Parameters(params): Parameters<MirrorAnimationParams>) -> String {
        json_result(super::handlers::mirror_animation(&self.state, params))
    }

    #[tool(
        description = "Queue a direction-set job: mirror derivable facings from a canonical source animation and run character contract."
    )]
    fn queue_direction_set(
        &self,
        Parameters(params): Parameters<QueueDirectionSetParams>,
    ) -> String {
        json_result(super::handlers::queue_direction_set(&self.state, params))
    }

    #[tool(description = "Set animation review status to draft, accepted, or rejected.")]
    fn set_animation_review_status(
        &self,
        Parameters(params): Parameters<SetReviewStatusParams>,
    ) -> String {
        json_result(super::handlers::set_animation_review(&self.state, params))
    }

    #[tool(description = "Save or update a native rig spec linked to a workspace master asset.")]
    fn save_rig(&self, Parameters(input): Parameters<crate::rig::RigInput>) -> String {
        json_result(super::handlers::save_rig(&self.state, input))
    }

    #[tool(description = "Render a saved native rig into frame PNGs and a draft animation.")]
    fn render_rig_animation(&self, Parameters(input): Parameters<crate::rig::RigInput>) -> String {
        json_result(super::handlers::render_rig_animation(&self.state, input))
    }

    #[tool(description = "Suggest deterministic rig points and bones for a master sprite asset.")]
    fn suggest_rig_points(&self, Parameters(params): Parameters<SuggestRigParams>) -> String {
        json_result(super::handlers::suggest_rig_points(&self.state, params))
    }

    #[tool(description = "Analyze how well a master sprite fits the native rig templates.")]
    fn analyze_rig_fit(&self, Parameters(params): Parameters<AssetIdParams>) -> String {
        json_result(super::handlers::analyze_rig_fit(&self.state, params))
    }

    #[tool(description = "Read the character identity profile sidecar for a promoted anchor.")]
    fn get_character_profile(&self, Parameters(params): Parameters<AnchorSlugParams>) -> String {
        json_result(super::handlers::get_character_profile(&self.state, params))
    }

    #[tool(
        description = "Queue a motion batch job for missing or selected catalog motions on a character worktree. Poll get_job for progress."
    )]
    fn queue_motion_batch(
        &self,
        Parameters(input): Parameters<crate::models::QueueMotionBatchInput>,
    ) -> String {
        json_result(super::handlers::queue_motion_batch(&self.state, input))
    }

    #[tool(
        description = "Queue regional regeneration for one animation frame using mask rectangles and optional brush strokes. Requires conversationId."
    )]
    fn queue_region_regen(
        &self,
        Parameters(input): Parameters<crate::models::QueueRegionRegenInput>,
    ) -> String {
        json_result(super::handlers::queue_region_regen(&self.state, input))
    }

    #[tool(
        description = "Export a character pack (spritesheets, previews, manifest) to a workspace-relative destination. destination is required."
    )]
    fn export_character_pack(
        &self,
        Parameters(input): Parameters<crate::models::ExportCharacterPackInput>,
    ) -> String {
        json_result(super::handlers::export_character_pack(&self.state, input))
    }

    #[tool(
        description = "Aggregate production score for a character worktree: size contract, character contract, and quality."
    )]
    fn get_production_score(
        &self,
        Parameters(params): Parameters<ProductionScoreParams>,
    ) -> String {
        json_result(super::handlers::get_production_score(&self.state, params))
    }

    #[tool(
        description = "Insert interpolated rig keyframes between two pose indices and return preview paths."
    )]
    fn interpolate_rig_frames(
        &self,
        Parameters(input): Parameters<crate::rig::InterpolateRigFramesInput>,
    ) -> String {
        json_result(super::handlers::interpolate_rig_frames(&self.state, input))
    }

    #[tool(description = "Score per-frame motion deltas and stability for an animation.")]
    fn score_animation_frames(
        &self,
        Parameters(params): Parameters<ScoreAnimationFramesParams>,
    ) -> String {
        json_result(super::handlers::score_animation_frames(&self.state, params))
    }

    #[tool(description = "List facing-check history for promoted anchors in a workspace.")]
    fn list_facing_checks(
        &self,
        Parameters(input): Parameters<crate::pipeline::ListFacingChecksInput>,
    ) -> String {
        json_result(super::handlers::list_facing_checks(&self.state, input))
    }
}

#[tool_handler(
    name = "sprite-studio",
    instructions = "Headless Sprite Studio MCP. Default chat provider is Codex so a Cursor client does not nest Cursor CLI. This is not the per-workspace Python server at .sprite-studio/sprite_rig_mcp.py."
)]
impl ServerHandler for SpriteStudioMcp {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct McpSpriteSheetInput {
    #[serde(rename = "projectId")]
    pub(crate) project_id: String,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: Option<String>,
    #[serde(rename = "animationId")]
    pub(crate) animation_id: String,
    pub(crate) name: String,
    pub(crate) layout: String,
    #[serde(rename = "frameWidth")]
    pub(crate) frame_width: u32,
    #[serde(rename = "frameHeight")]
    pub(crate) frame_height: u32,
    pub(crate) padding: u32,
    pub(crate) spacing: u32,
    pub(crate) columns: u32,
    pub(crate) scale: u32,
    pub(crate) transparent: bool,
    pub(crate) alignment: String,
    #[serde(rename = "pivotX")]
    pub(crate) pivot_x: f64,
    #[serde(rename = "pivotY")]
    pub(crate) pivot_y: f64,
    #[serde(rename = "metadataFormat")]
    pub(crate) metadata_format: Option<String>,
}

impl McpSpriteSheetInput {
    fn into_input(self) -> SpriteSheetInput {
        SpriteSheetInput {
            project_id: self.project_id,
            worktree_id: self.worktree_id,
            animation_id: self.animation_id,
            name: self.name,
            layout: self.layout,
            frame_width: self.frame_width,
            frame_height: self.frame_height,
            padding: self.padding,
            spacing: self.spacing,
            columns: self.columns,
            scale: self.scale,
            transparent: self.transparent,
            alignment: self.alignment,
            pivot_x: self.pivot_x,
            pivot_y: self.pivot_y,
            metadata_format: self.metadata_format,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct McpProceduralVfxInput {
    #[serde(rename = "projectId")]
    pub(crate) project_id: String,
    #[serde(rename = "worktreeId")]
    pub(crate) worktree_id: String,
    pub(crate) name: String,
    #[serde(rename = "effectType")]
    pub(crate) effect_type: String,
    #[serde(rename = "blendMode")]
    pub(crate) blend_mode: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frames: u32,
    pub(crate) fps: u32,
    pub(crate) looping: bool,
    pub(crate) seed: u64,
}

impl McpProceduralVfxInput {
    fn into_input(self) -> ProceduralVfxInput {
        ProceduralVfxInput {
            project_id: self.project_id,
            worktree_id: self.worktree_id,
            name: self.name,
            effect_type: self.effect_type,
            blend_mode: self.blend_mode,
            width: self.width,
            height: self.height,
            frames: self.frames,
            fps: self.fps,
            looping: self.looping,
            seed: self.seed,
        }
    }
}

fn json_result<T: Serialize>(value: CommandResult<T>) -> String {
    match value {
        Ok(value) => serde_json::to_string_pretty(&value)
            .unwrap_or_else(|error| format!(r#"{{"error":{}}}"#, Value::String(error.to_string()))),
        Err(error) => serde_json::json!({
            "error": error.code,
            "message": error.message,
        })
        .to_string(),
    }
}

#[cfg(test)]
impl SpriteStudioMcp {
    pub(crate) fn listed_tool_names() -> Vec<String> {
        Self::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }
}
