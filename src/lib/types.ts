export type Workspace = { id: string; name: string; path: string; createdAt: string; lastOpenedAt: string };
export type ProjectBackup = { formatVersion: number; projectId: string; projectName: string; sourcePath: string; backupPath: string; createdAt: string; fileCount: number; totalBytes: number };
export type SidebarSnapshot = { workspaces: Workspace[]; worktrees: Worktree[]; conversations: Conversation[] };
export type WorktreeKind = "general" | "character" | "environment" | "creature" | "object" | "tileset" | "animation" | "vfx" | "ui";
export type Worktree = { id: string; projectId: string; name: string; slug: string; kind: WorktreeKind; description?: string; createdAt: string; updatedAt: string };
export type Conversation = { id: string; workspaceId: string; worktreeId?: string; title: string; provider: string; providerSessionId?: string; createdAt: string; updatedAt: string; archivedAt?: string };
export type Message = { id: string; conversationId: string; role: "user" | "assistant" | "system"; kind: string; content: string; status: "queued" | "running" | "completed" | "failed" | "cancelled"; metadata: Record<string, unknown>; createdAt: string };
export type Asset = { id: string; workspaceId: string; name: string; path: string; relativePath: string; category: string; format: string; width: number; height: number; fileSize: number; hasAlpha: boolean; createdAt: string };
export type AssetPack = { id: string; name: string; description: string; style: string; kind: string; files: string[]; createdAt: string };
export type AssetVersion = { id: string; assetId: string; versionNumber: number; parentVersionId?: string; generationId?: string; path: string; format: string; width: number; height: number; fileSize: number; hasAlpha: boolean; contentHash: string; changeKind: string; available: boolean; selected: boolean; createdAt: string };
export type ReferenceCategory = "character_appearance" | "clothing" | "face" | "weapon" | "pose" | "art_style" | "environment" | "palette" | "animation" | "vfx" | "anatomy" | "lighting" | "other";
export type ReferenceImage = { id: string; projectId: string; worktreeId: string; name: string; path: string; relativePath: string; category: ReferenceCategory; notes?: string; format: string; width: number; height: number; fileSize: number; contentHash: string; createdAt: string; updatedAt: string };
export type AnimationFrame = { assetId: string; durationMs?: number };
export type Animation = { id: string; workspaceId: string; worktreeId?: string; name: string; fps: number; looping: boolean; frames: AnimationFrame[]; motionPlan?: MotionPlan; createdAt: string; updatedAt: string };
export type AnimationInput = Omit<Animation, "id" | "createdAt" | "updatedAt"> & { id?: string };
export type AnimationTemplatePhase = { id: string; templateId: string; position: number; name: string; description: string; frameCount: number; timingWeight: number; movementOffsetX: number; movementOffsetY: number; weaponPosition?: string; poseReferenceId?: string };
export type AnimationTemplate = { id: string; projectId: string; sourceAnimationId?: string; name: string; intent: string; motionDescription: string; direction: string; looping: boolean; fps: number; width: number; height: number; pivotX?: number; pivotY?: number; frameMode: FrameMode; preferredFrames: number; minFrames: number; maxFrames: number; generationPrompt: string; negativePrompt: string; weaponBehavior?: string; phases: AnimationTemplatePhase[]; createdAt: string; updatedAt: string };
export type TemplateApplication = { template: AnimationTemplate; targetAsset: Asset; motionPlan: MotionPlan; prompt: string };
export type ProviderMode = { id: string; label: string; description: string; defaultReasoningEffort: string; reasoningEfforts: string[] };
export type ProviderCapabilities = { textInput: boolean; imageInput: boolean; multipleImageInput: boolean; imageEditing: boolean; masks: boolean; transparency: boolean; structuredOutput: boolean; videoAnimation: boolean; imageToImage: boolean; maximumReferenceImages: number };
export type ProviderStatus = { id: string; name: string; kind: "agent" | "image"; installed: boolean; executable?: string; status: string; detail: string; modes: ProviderMode[]; capabilities: ProviderCapabilities; configurable: boolean; hasApiKey: boolean; baseUrl?: string; model?: string };
export type ImageProviderInput = { id: string; name: string; providerType: "grok" | "openai-compatible"; baseUrl: string; apiKey: string; model: string };
export type ProviderConnectionTest = { ok: boolean; detail: string };
export type GenerationQuality = "low" | "mid" | "high" | "custom";
export type FrameMode = "fixed" | "auto";
export type ChatGenerationProfile = { profileVersion: number; quality: GenerationQuality; width: number; height: number; frames: number; fps: number; frameMode: FrameMode; minFrames: number; maxFrames: number; allowInterpolation: boolean; allowAutoAdjust: boolean; model: string; reasoningEffort: string; imageProviderId: string };
export type SpriteSlashCommand = "animate" | "sprite" | "character" | "effect" | "pack" | "rig";
export type ProviderRequestOptions = { model?: string; reasoningEffort?: string; command?: SpriteSlashCommand; generation: Pick<ChatGenerationProfile, "quality" | "width" | "height" | "frames" | "fps" | "frameMode" | "minFrames" | "maxFrames" | "allowInterpolation" | "allowAutoAdjust">; referenceIds?: string[]; imageProviderId?: string };
export type MotionPhase = { name: string; description: string; frameCount: number; timingWeight: number };
export type MotionPlan = { frameMode: FrameMode; selectedFrameCount: number; minimumFrameCount: number; maximumFrameCount: number; fps: number; looping: boolean; allowInterpolation: boolean; allowAutoAdjust: boolean; explanation: string; phases: MotionPhase[] };
export type ProviderEvent = { requestId: string; conversationId: string; eventType: "started" | "content" | "activity" | "completed" | "failed" | "cancelled"; content: string };
export type GenerationManifest = { kind?: "sprite" | "pack"; name: string; category: string; fps: number; files: string[]; generatedAt: string };
export type AnimationPolishMode = "rig" | "ai-polish" | "full-redraw";
export type SpriteGenerationMetadata = { kind: "sprite-generation"; name: string; category: string; fps: number; assetIds: string[]; animationId?: string };
export type PackGenerationMetadata = { kind: "pack-generation"; packId: string };
export type ExportResult = { pngPath: string; metadataPath: string; width: number; height: number };
export type TerrainRuleRole = "top_left" | "top" | "top_right" | "left" | "center" | "right" | "bottom_left" | "bottom" | "bottom_right";
export type TerrainRuleMode = "nine_slice" | "blob_47";
export type TerrainRuleInput = { role: TerrainRuleRole | `blob_${number}`; column: number; row: number };
export type TerrainExportInput = { projectId: string; worktreeId: string; assetId: string; name: string; tileWidth: number; tileHeight: number; marginX: number; marginY: number; separationX: number; separationY: number; includeEmpty: boolean; terrainName?: string; terrainMode?: TerrainRuleMode; terrainRules?: TerrainRuleInput[] };
export type TerrainExportResult = { directoryPath: string; texturePath: string; resourcePath: string; columns: number; rows: number; tileCount: number; occupiedTileCount: number; trailingX: number; trailingY: number; terrainRuleCount: number; terrainMode: "plain" | TerrainRuleMode };
export type JobStatus = "queued" | "running" | "analyzing" | "completed" | "failed" | "cancelled";
export type BackgroundJob = { id: string; projectId: string; worktreeId?: string; kind: string; targetType?: string; targetId?: string; status: JobStatus; progress: number; stage: string; errorMessage?: string; cancelRequested: boolean; resultPath?: string; createdAt: string; startedAt?: string; completedAt?: string; updatedAt: string };
export type JobEvent = { job: BackgroundJob };
export type SpriteSheetLayout = "horizontal" | "vertical" | "grid";
export type FrameAlignment = "top_left" | "center" | "bottom_center";
export type SpriteSheet = { id: string; projectId: string; worktreeId?: string; animationId: string; name: string; layout: SpriteSheetLayout; frameWidth: number; frameHeight: number; padding: number; spacing: number; rows: number; columns: number; scale: number; transparent: boolean; alignment: FrameAlignment; pivotX: number; pivotY: number; pngPath: string; metadataPath: string; width: number; height: number; frameCount: number; createdAt: string; updatedAt: string };
export type SpriteSheetInput = { projectId: string; worktreeId?: string; animationId: string; name: string; layout: SpriteSheetLayout; frameWidth: number; frameHeight: number; padding: number; spacing: number; columns: number; scale: number; transparent: boolean; alignment: FrameAlignment; pivotX: number; pivotY: number };
export type VfxEffectType = "fire" | "explosion" | "magic" | "slash" | "smoke" | "frost_lance" | "storm_lance" | "nova_beam" | "voltaic_snare";
export type VfxBlendMode = "normal" | "add" | "screen" | "multiply";
export type VfxEffect = { id: string; projectId: string; worktreeId: string; animationId?: string; name: string; effectType: VfxEffectType; blendMode: VfxBlendMode; centerX: number; centerY: number; opacity: number; looping: boolean; fps: number; createdAt: string; updatedAt: string };
export type ProceduralVfxInput = { projectId: string; worktreeId: string; name: string; effectType: VfxEffectType; blendMode: VfxBlendMode; width: number; height: number; frames: number; fps: number; looping: boolean; seed: number };
export type QualitySeverity = "info" | "warning" | "error";
export type QualityCheck = { id: string; reportId: string; position: number; checkType: string; frameIndex?: number; comparisonFrameIndex?: number; severity: QualitySeverity; score: number; message: string; metricValue?: number; metricUnit?: string; repairAction?: string; acknowledged: boolean; ignored: boolean; createdAt: string };
export type QualityReport = { id: string; projectId: string; worktreeId?: string; animationId: string; jobId?: string; status: "running" | "completed" | "failed" | "cancelled"; overallScore: number; characterConsistencyScore: number; motionContinuityScore: number; frameAlignmentScore: number; weaponConsistencyScore: number; loopQualityScore: number; transparencyScore: number; frameCount: number; analyzerVersion: string; checks: QualityCheck[]; createdAt: string; completedAt?: string; updatedAt: string };
export type FrameOptimizationResult = { animation: Animation; removedFrames: number; insertedFrames: number; replacedFrames: number; summary: string };

export type RigMorphology = "biped" | "quadruped" | "winged" | "serpentine" | "object" | "amorphous";
export type RigPointKind = "joint" | "pivot" | "anchor" | "contact";
export type RigPointSource = "auto" | "ai" | "user";
export type RigPoint = { id: string; name: string; kind: RigPointKind; x: number; y: number; confidence: number; source: RigPointSource; note?: string };
export type RigBone = { id: string; name: string; startPoint: string; endPoint: string; radius: number; parent?: string; z: number };
export type RigTransform = { bone: string; dx: number; dy: number; rotate: number; scaleX: number; scaleY: number };
export type RigContact = { bone: string; x: number; y: number; bend: number };
export type RigFrame = { phase?: string; hold: boolean; rootDx: number; rootDy: number; transforms: RigTransform[]; contacts: RigContact[] };
export type Rig = { id: string; workspaceId: string; worktreeId?: string; assetId?: string; name: string; morphology: RigMorphology; fps: number; looping: boolean; points: RigPoint[]; bones: RigBone[]; frames: RigFrame[]; createdAt: string; updatedAt: string };
export type RigInput = Omit<Rig, "id" | "createdAt" | "updatedAt"> & { id?: string };
export type RigSuggestion = { morphology: RigMorphology; points: RigPoint[]; bones: RigBone[]; frames: RigFrame[]; reasoning: string; source: RigPointSource };
export type MorphologyScore = { morphology: RigMorphology; confidence: number; reasoning: string };
export type RigFitReport = { detections: MorphologyScore[]; recommended: RigSuggestion; capsuleFit: number; warnings: string[] };
export type RigRenderResult = { animation: Animation; framePaths: string[]; assetIds: string[] };

export const RIG_MORPHOLOGIES: { id: RigMorphology; label: string }[] = [
  { id: "biped", label: "Biped (humanoid)" },
  { id: "quadruped", label: "Quadruped" },
  { id: "winged", label: "Winged" },
  { id: "serpentine", label: "Serpentine" },
  { id: "object", label: "Object" },
  { id: "amorphous", label: "Amorphous" },
];

export type StudioError = { code: string; message: string };

export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    try { return (JSON.parse(error) as StudioError).message ?? error; } catch { return error; }
  }
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return "Something unexpected happened";
}
