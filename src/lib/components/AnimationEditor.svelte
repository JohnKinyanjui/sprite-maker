<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Play, Pause, SkipBack, SkipForward, Plus, Copy, Trash2, GripVertical, Save, Download, Clapperboard, Repeat2, FileKey2, ShieldCheck, Bone, FolderOpen, Move, RotateCcw, BadgeCheck, Layers, Scissors, Film, RefreshCw, Sparkles, X, Hammer, Eraser, Grid3x3, Package, Gauge, Paintbrush } from "lucide-svelte";
  import { api, assetUrl } from "$lib/api";
  import TemplateDialog from "$lib/components/TemplateDialog.svelte";
  import TemplateApplyDialog from "$lib/components/TemplateApplyDialog.svelte";
  import QualityPanel from "$lib/components/QualityPanel.svelte";
  import ContractRetryDialog from "$lib/components/ContractRetryDialog.svelte";
  import VideoImportWizard from "$lib/components/VideoImportWizard.svelte";
  import MotionBatchPanel from "$lib/components/MotionBatchPanel.svelte";
  import CharacterPackExportDialog from "$lib/components/CharacterPackExportDialog.svelte";
  import FrameCleanupToolbar from "$lib/components/FrameCleanupToolbar.svelte";
  import FrameMaskOverlay from "$lib/components/FrameMaskOverlay.svelte";
  import { errorMessage, type Animation, type AnimationDirectionMeta, type AnimationExportFormat, type AnimationFrame, type AnimationFrameScoreReport, type AnimationReviewStatus, type AnimationTemplate, type Asset, type AssetVersion, type BackgroundJob, type BrushStamp, type CharacterAnchorSummary, type CharacterContractReport, type FrameMode, type GenerationManifest, type HardenAnimationReport, type JobEvent, type MotionBatchProgressEntry, type MotionPreset, type ProductionScoreReport, type QualityCheck, type QualityReport, type RegionMaskRect, type SizeContractReport, type StripScoreReport, type TemplateApplication, type WorkspaceRigSpec } from "$lib/types";

  let { workspaceId, workspacePath, worktreeId, conversationId, assets, animations, templates, selectedAnimation, linkedRigId, active = true, onAnimations, onAssetsRefresh, onTemplates, onSelected, onOpenRig, onTemplateApplication, onError, onNotice }: {
    workspaceId: string; workspacePath?: string; worktreeId?: string; conversationId?: string; assets: Asset[]; animations: Animation[]; templates: AnimationTemplate[]; selectedAnimation?: Animation;
    linkedRigId?: string; active?: boolean; onAnimations: (animations: Animation[]) => void; onAssetsRefresh?: () => void | Promise<void>;
    onTemplates: (templates: AnimationTemplate[]) => void; onSelected: (animation: Animation) => void; onOpenRig?: (rigId: string) => void;
    onTemplateApplication: (application:TemplateApplication)=>void; onError: (message: string) => void; onNotice: (message: string) => void;
  } = $props();
  let selectedPropId = $state<string | undefined>();
  let animationId = $state<string | undefined>();
  let name = $state("New animation");
  let fps = $state(10);
  let looping = $state(true);
  let frames = $state<AnimationFrame[]>([]);
  let activeFrame = $state(0);
  let playing = $state(false);
  let scale = $state(2);
  let onionSkin = $state(false);
  let onionOpacity = $state(0.24);
  let saving = $state(false);
  let draggedIndex = $state<number | undefined>();
  let templateDialog = $state(false);
  let applyingTemplate = $state<AnimationTemplate>();
  let templateBusy = $state(false);
  let qualityOpen = $state(false);
  let qualityReport = $state<QualityReport>();
  let qualityJob = $state<BackgroundJob>();
  let repairing = $state(false);
  let optimizing = $state(false);
  let previewEpoch = $state(0);
  let generationManifest = $state<GenerationManifest | null>();
  let workspaceRigs = $state<WorkspaceRigSpec[]>([]);
  let alignerOpen = $state(false);
  let reviewStatus = $state<AnimationReviewStatus>("draft");
  let sizeContract = $state<SizeContractReport>();
  let alignerBusy = $state(false);
  let pipelineBusy = $state(false);
  let contractRetryJob = $state<BackgroundJob>();
  let anchorSummaries = $state<CharacterAnchorSummary[]>([]);
  let showAlignGhosts = $state(true);
  let nudgeAllFrames = $state(false);
  let exportFormat = $state<AnimationExportFormat>("sprite-studio");
  let exportMetadataPreview = $state("");
  let characterContract = $state<CharacterContractReport>();
  let directionSetBusy = $state(false);
  let stripDialog = $state(false);
  let stripDialogMode = $state<"import" | "finalize">("import");
  let stripScore = $state<StripScoreReport>();
  let stripSourcePath = $state("");
  let videoWizardPath = $state<string>();
  let frameScoreReport = $state<AnimationFrameScoreReport>();
  let directionSetJob = $state<BackgroundJob>();
  let directionSet = $state<"4" | "8">("4");
  let directionMetas = $state<AnimationDirectionMeta[]>([]);
  let hardenCleanAlpha = $state(true);
  let hardenNormalize = $state(true);
  let hardenSnapGrid = $state(true);
  let hardenGridSize = $state(1);
  let hardenQueueRetry = $state(false);
  let lastHardenReport = $state<HardenAnimationReport>();
  let productionScore = $state<ProductionScoreReport>();
  let productionScoreOpen = $state(false);
  let motionPresets = $state<MotionPreset[]>([]);
  let selectedMotionIds = $state<string[]>([]);
  let motionBatchBusy = $state(false);
  let motionBatchJob = $state<BackgroundJob>();
  let motionBatchEntries = $state<MotionBatchProgressEntry[]>([]);
  let motionBatchCategory = $state("locomotion");
  let motionBatchSearch = $state("");
  let motionOnlyMissing = $state(false);
  let missingMotionIds = $state<string[]>([]);
  let packExportOpen = $state(false);
  let regionX = $state(0);
  let regionY = $state(0);
  let regionW = $state(16);
  let regionH = $state(16);
  let regionPrompt = $state("");
  let regionRegenBusy = $state(false);
  let regionRegenJob = $state<BackgroundJob>();
  let maskRegions = $state<RegionMaskRect[]>([]);
  let maskBrushStrokes = $state<BrushStamp[]>([]);
  let cleanupBrushStrokes = $state<BrushStamp[]>([]);
  let maskOverlayMode = $state<"regen" | "erase">("regen");
  let currentAssetVersions = $state<AssetVersion[]>([]);
  let hardenQuantizePalette = $state(false);
  let hardenPaletteColorCount = $state(32);
  let paletteBusy = $state(false);
  const directionFacings = $derived(directionSet === "8"
    ? ["n", "ne", "e", "se", "s", "sw", "w", "nw"]
    : ["n", "e", "s", "w"]);
  const facingFromName = (animationName: string) => {
    const lower = animationName.toLowerCase();
    for (const facing of ["nw", "ne", "sw", "se", "n", "s", "e", "w"]) {
      if (lower.endsWith(`-${facing}`) || lower.includes(`_${facing}`)) return facing;
    }
    return undefined;
  };
  const inferDirectionFamily = (animationName: string) => {
    const lower = animationName.toLowerCase();
    for (const token of ["walk", "run", "idle", "attack", "hurt", "jump", "cast"]) {
      if (lower.includes(token)) return token;
    }
    const slug = lower.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
    return slug || "motion";
  };
  const metaForAnimation = (animation: Animation) => directionMetas.find(meta => meta.animationId === animation.id);
  const animationFamily = (animation: Animation) => metaForAnimation(animation)?.directionFamily ?? inferDirectionFamily(animation.name);
  const animationFacing = (animation: Animation) => metaForAnimation(animation)?.facing ?? facingFromName(animation.name);
  const currentDirectionFamily = $derived(animationId
    ? (directionMetas.find(meta => meta.animationId === animationId)?.directionFamily ?? inferDirectionFamily(name))
    : inferDirectionFamily(name));
  const facingIsFilled = (facing: string) => directionMetas.some(meta => meta.directionFamily === currentDirectionFamily && meta.facing === facing)
    || animations.some(animation => animationFamily(animation) === currentDirectionFamily && animationFacing(animation) === facing);
  const estimatePendingFacings = () => {
    const sourceFacing = animationFacing({ id: animationId ?? "", name, workspaceId, fps, looping, frames, createdAt: "", updatedAt: "" });
    const allFacings = directionSet === "8" ? ["n", "ne", "e", "se", "s", "sw", "w", "nw"] : ["n", "e", "s", "w"];
    return allFacings.filter(facing => facing !== sourceFacing && !facingIsFilled(facing));
  };
  const currentAnimationFacing = $derived(animationFacing({ id: animationId ?? "", name, workspaceId, fps, looping, frames, createdAt: "", updatedAt: "" }));
  const directionSetNeedsConversation = $derived(estimatePendingFacings().length > 0 && !conversationId);
  const facingHasContractIssue = (facing: string) => characterContract?.crossFacingViolations?.some(violation => violation.message.toLowerCase().includes(`facing ${facing}`)) ?? false;
  const facingMetaFor = (facing: string) => directionMetas.find(meta => meta.directionFamily === currentDirectionFamily && meta.facing === facing);
  const facingBadge = (facing: string) => {
    if (!facingIsFilled(facing)) return "pending";
    const meta = facingMetaFor(facing);
    if (meta?.mirroredFrom) return "mirrored";
    if (meta && meta.animationId !== animationId) return "ai";
    return "source";
  };
  const directionSetActive = $derived(Boolean(directionSetJob && animationId && directionSetJob.targetId === animationId && ["queued", "running", "analyzing"].includes(directionSetJob.status)));
  const motionBatchActive = $derived(Boolean(motionBatchJob && worktreeId && motionBatchJob.worktreeId === worktreeId && ["queued", "running", "analyzing"].includes(motionBatchJob.status)));
  const motionHasWestFacing = (motion: string) => directionMetas.some(meta => meta.directionFamily === motion && meta.facing === "w");
  const motionBatchNeedsConversation = $derived(selectedMotionIds.some(id => {
    const preset = motionPresets.find(entry => entry.id === id);
    return preset ? !motionHasWestFacing(preset.motion) : false;
  }) && !conversationId);

  function previewSrc(path: string) {
    const url = assetUrl(path);
    return previewEpoch ? `${url}${url.includes("?") ? "&" : "?"}v=${previewEpoch}` : url;
  }

  async function refreshAfterRepair(repaired: Animation) {
    await onAssetsRefresh?.();
    loadAnimation(repaired);
    previewEpoch += 1;
  }

  $effect(() => {
    if (!active || !workspaceId) return;
    void api.getGenerationManifest(workspaceId).then(value => generationManifest = value).catch(() => generationManifest = null);
    void api.listWorkspaceRigSpecs(workspaceId).then(value => workspaceRigs = value).catch(() => workspaceRigs = []);
    void api.listAnchors(workspaceId).then(value => anchorSummaries = value).catch(() => anchorSummaries = []);
    void refreshMotionPresets();
    if (worktreeId) { void refreshDirectionMeta(); void refreshProductionScore(); void refreshMotionBatchJob(); void refreshMissingMotions(); }
  });
  async function refreshMotionPresets() {
    try {
      const catalog = await api.listMotionPresets(workspaceId);
      motionPresets = catalog.presets.filter(preset => preset.enabled);
      if (!selectedMotionIds.length) selectedMotionIds = motionPresets.map(preset => preset.id);
    } catch { motionPresets = []; }
  }
  function toggleMotionPreset(id: string) {
    selectedMotionIds = selectedMotionIds.includes(id) ? selectedMotionIds.filter(value => value !== id) : [...selectedMotionIds, id];
  }
  async function refreshMissingMotions() {
    if (!worktreeId || !guideAnchor?.slug) { missingMotionIds = []; return; }
    try {
      const result = await api.listMissingMotions(workspaceId, worktreeId, guideAnchor.slug);
      missingMotionIds = result.missingPresetIds;
    } catch { missingMotionIds = []; }
  }
  function selectAllMissingMotions() {
    selectedMotionIds = [...new Set([...selectedMotionIds, ...missingMotionIds])];
  }
  function parseMotionBatchEntries(job: BackgroundJob) {
    if (!job.metadataJson) return [] as MotionBatchProgressEntry[];
    try {
      const parsed = JSON.parse(job.metadataJson) as { facingEntries?: MotionBatchProgressEntry[]; entries?: MotionBatchProgressEntry[] };
      if (parsed.facingEntries?.length) return parsed.facingEntries;
      return parsed.entries ?? [];
    } catch { return [] as MotionBatchProgressEntry[]; }
  }
  async function refreshMotionBatchJob() {
    if (!worktreeId) { motionBatchJob = undefined; motionBatchEntries = []; return; }
    try {
      const jobs = await api.listJobs(workspaceId, worktreeId);
      motionBatchJob = jobs.find(job => job.kind === "motion_batch" && job.worktreeId === worktreeId && ["queued", "running", "analyzing"].includes(job.status)) ?? motionBatchJob;
      if (motionBatchJob) motionBatchEntries = parseMotionBatchEntries(motionBatchJob);
    } catch { /* ignore */ }
  }
  async function handleMotionBatchJobFinished(job: BackgroundJob, failed = false) {
    motionBatchJob = job;
    motionBatchEntries = parseMotionBatchEntries(job);
    await onAssetsRefresh?.();
    await refreshDirectionMeta();
    onAnimations(await api.listAnimations(workspaceId, worktreeId));
    await refreshCharacterContract();
    await refreshProductionScore();
    if (!failed) onNotice(job.status === "completed" ? "Motion batch finished" : "Motion batch finished with issues");
  }
  async function queueMotionBatchJob() {
    if (!worktreeId || !guideAnchor?.slug || !selectedMotionIds.length) return;
    motionBatchBusy = true;
    try {
      const result = await api.queueMotionBatch({
        workspaceId,
        worktreeId,
        anchorSlug: guideAnchor.slug,
        motions: selectedMotionIds,
        set: directionSet,
        conversationId,
        hardenAfter: true,
        seedAnimationId: animationId,
        onlyMissing: motionOnlyMissing,
      });
      const jobs = await api.listJobs(workspaceId, worktreeId);
      motionBatchJob = jobs.find(job => job.id === result.jobId) ?? {
        id: result.jobId,
        projectId: workspaceId,
        worktreeId,
        kind: "motion_batch",
        targetType: "worktree",
        targetId: worktreeId,
        status: "queued",
        progress: 0,
        stage: "Queued",
        cancelRequested: false,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      motionBatchEntries = result.entries;
      onNotice(`Motion batch queued (job ${result.jobId}) — ${result.motions.join(", ")}`);
    } catch (error) { onError(errorMessage(error)); }
    finally { motionBatchBusy = false; }
  }
  async function refreshProductionScore() {
    if (!worktreeId) { productionScore = undefined; return; }
    try { productionScore = await api.getProductionScore(workspaceId, worktreeId, guideAnchor?.slug); }
    catch { productionScore = undefined; }
  }
  async function refreshDirectionMeta() {
    if (!worktreeId) { directionMetas = []; return; }
    try { directionMetas = await api.listDirectionMeta(workspaceId, worktreeId); }
    catch { directionMetas = []; }
  }

  const linkedWorkspaceRig = $derived(workspaceRigs.find(spec => generationManifest?.rig === spec.relativePath));
  const effectiveLinkedRigId = $derived(generationManifest?.rigId ?? linkedRigId);
  const nativeRigLabel = $derived(effectiveLinkedRigId ? "Native rig saved in Rig editor" : linkedWorkspaceRig ? `Workspace rig: ${linkedWorkspaceRig.name}` : undefined);

  async function revealWorkspaceRig() {
    if (!workspacePath || !linkedWorkspaceRig) return;
    const separator = workspacePath.includes("\\") ? "\\" : "/";
    const jsonPath = `${workspacePath}${workspacePath.endsWith(separator) ? "" : separator}${linkedWorkspaceRig.relativePath.replace(/\//g, separator)}`;
    try { await revealItemInDir(jsonPath); }
    catch (error) { onError(errorMessage(error)); }
  }

  $effect(() => {
    if (selectedAnimation && selectedAnimation.id !== selectedPropId) loadAnimation(selectedAnimation);
  });
  $effect(() => {
    if (!active || !playing || !frames.length) return;
    const duration = frames[activeFrame]?.durationMs ?? 1000 / fps;
    const timer = window.setTimeout(() => {
      if (activeFrame < frames.length - 1) activeFrame += 1;
      else if (looping) activeFrame = 0;
      else playing = false;
    }, duration);
    return () => window.clearTimeout(timer);
  });

  let currentAsset = $derived(assets.find(asset => asset.id === frames[activeFrame]?.assetId));
  $effect(() => {
    if (!currentAsset) return;
    regionW = Math.max(4, Math.floor(currentAsset.width / 2));
    regionH = Math.max(4, Math.floor(currentAsset.height / 2));
    regionX = Math.max(0, Math.floor((currentAsset.width - regionW) / 2));
    regionY = Math.max(0, Math.floor((currentAsset.height - regionH) / 2));
  });
  let previousAsset = $derived(activeFrame > 0 ? assets.find(asset => asset.id === frames[activeFrame-1]?.assetId) : looping && frames.length > 1 ? assets.find(asset => asset.id === frames.at(-1)?.assetId) : undefined);
  let nextAsset = $derived(activeFrame < frames.length-1 ? assets.find(asset => asset.id === frames[activeFrame+1]?.assetId) : looping && frames.length > 1 ? assets.find(asset => asset.id === frames[0]?.assetId) : undefined);
  const frameAsset = (frame: AnimationFrame) => assets.find(asset => asset.id === frame.assetId);
  const frameSeverity = (index:number) => qualityReport?.checks.some(check=>check.frameIndex===index&&check.severity==="error"&&!check.ignored)?"error":qualityReport?.checks.some(check=>check.frameIndex===index&&check.severity==="warning"&&!check.ignored)?"warning":qualityReport?"good":"";
  let canOptimize = $derived(Boolean(animationId&&(
    qualityReport?.checks.some(check=>!check.ignored&&check.severity!=="info"&&["remove_duplicate","regenerate_transition"].includes(check.repairAction??"")) ||
    sizeContract?.violations.some(violation=>["motion_still","identity_drift","identity_warning"].includes(violation.code))
  )));
  const guideAnchor = $derived(anchorSummaries[0]);
  const guideFrameHeight = $derived(guideAnchor?.frameHeight ?? currentAsset?.height ?? 64);
  const guideBaselineY = $derived(guideAnchor?.baselineY ?? guideFrameHeight - 1);
  const guidePivotX = $derived(guideAnchor?.pivot.x ?? (currentAsset?.width ?? 64) / 2);

  const contractRetryActive = $derived(Boolean(contractRetryJob&&animationId&&contractRetryJob.targetId===animationId&&["queued","running","analyzing"].includes(contractRetryJob.status)));
  async function refreshContractRetryJob(id:string){try{const jobs=await api.listJobs(workspaceId,worktreeId);contractRetryJob=jobs.find(job=>job.kind==="contract_retry"&&job.targetId===id&&["queued","running","analyzing"].includes(job.status))??(contractRetryJob?.targetId===id?contractRetryJob:undefined);}catch{/* ignore */}}
  async function cancelContractRetryJob(){if(!contractRetryJob)return;pipelineBusy=true;try{contractRetryJob=await api.cancelJob(contractRetryJob.id);onNotice("Autonomous contract retry cancelled");}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  function loadAnimation(animation:Animation,preserveFrame=false){const previousFrame=activeFrame;selectedPropId=animation.id;animationId=animation.id;name=animation.name;fps=animation.fps;looping=animation.looping;frames=animation.frames.map(frame=>({...frame}));reviewStatus=animation.reviewStatus ?? "draft";activeFrame=preserveFrame&&animation.frames.length?Math.min(previousFrame,animation.frames.length-1):0;playing=false;qualityReport=undefined;qualityJob=undefined;contractRetryJob=contractRetryJob?.targetId===animation.id?contractRetryJob:undefined;directionSetJob=directionSetJob?.targetId===animation.id?directionSetJob:undefined;sizeContract=undefined;characterContract=undefined;lastHardenReport=undefined;void loadQuality(animation.id);if(animation.id){void refreshSizeContract(animation.id);void refreshContractRetryJob(animation.id);void refreshDirectionSetJob(animation.id);void refreshCharacterContract();}}
  async function refreshSizeContract(id:string){try{sizeContract=await api.checkSizeContract(id);}catch(error){onError(errorMessage(error));}}
  async function refreshCharacterContract(){if(!workspaceId||!worktreeId)return;try{characterContract=await api.checkCharacterContract(workspaceId,worktreeId,guideAnchor?.slug);}catch(error){onError(errorMessage(error));}}
  async function mirrorEastFromWest(){if(!animationId||!guideAnchor?.slug)return;directionSetBusy=true;try{const result=await api.mirrorAnimation({animationId,targetFacing:"e",sourceFacing:"w",anchorSlug:guideAnchor.slug});await onAssetsRefresh?.();await refreshDirectionMeta();onAnimations(await api.listAnimations(workspaceId,worktreeId));sizeContract=result.contractReport;onNotice(`Mirrored animation created (${result.mirroredAnimationId})`);await refreshCharacterContract();}catch(error){onError(errorMessage(error));}finally{directionSetBusy=false;}}
  async function refreshDirectionSetJob(id:string){try{const jobs=await api.listJobs(workspaceId,worktreeId);directionSetJob=jobs.find(job=>job.kind==="direction_set"&&job.targetId===id&&["queued","running","analyzing"].includes(job.status))??(directionSetJob?.targetId===id?directionSetJob:undefined);}catch{/* ignore */}}
  async function handleDirectionSetJobFinished(job:BackgroundJob,failed=false){if(!animationId||job.targetId&&job.targetId!==animationId)return;directionSetJob=job;await onAssetsRefresh?.();await refreshDirectionMeta();const updated=await api.listAnimations(workspaceId,worktreeId);onAnimations(updated);await refreshCharacterContract();if(!failed)onNotice(job.status==="completed"?"Direction set finished — character contract refreshed":"Direction set finished with contract issues");}
  async function queueDirectionSetJob(){if(!animationId||!worktreeId||!guideAnchor?.slug)return;directionSetBusy=true;try{const motion=name.toLowerCase().includes("run")?"run":name.toLowerCase().includes("idle")?"idle":"walk";const result=await api.queueDirectionSet({workspaceId,worktreeId,sourceAnimationId:animationId,anchorSlug:guideAnchor.slug,motion,set:directionSet,conversationId});const jobs=await api.listJobs(workspaceId,worktreeId);directionSetJob=jobs.find(job=>job.id===result.jobId)??{id:result.jobId,projectId:workspaceId,worktreeId,kind:"direction_set",targetType:"animation",targetId:animationId,status:"queued",progress:0,stage:"Queued",cancelRequested:false,createdAt:new Date().toISOString(),updatedAt:new Date().toISOString()};onNotice(result.pendingFacings.length?`Direction set queued (job ${result.jobId}). Pending facings: ${result.pendingFacings.join(", ")}`:`Direction set queued (job ${result.jobId})`);}catch(error){onError(errorMessage(error));}finally{directionSetBusy=false;}}
  async function runHarden(){if(!animationId)return;if(hardenQueueRetry&&!conversationId){onError("Apri una conversazione chat prima di accodare il contract retry automatico");return;}pipelineBusy=true;try{const report=await api.hardenAnimation(animationId,guideAnchor?.slug,undefined,frames.length,conversationId,{cleanAlpha:hardenCleanAlpha,normalize:hardenNormalize,snapGrid:hardenSnapGrid,gridSize:hardenGridSize,queueContractRetry:hardenQueueRetry,quantizePalette:hardenQuantizePalette,paletteColorCount:hardenPaletteColorCount});lastHardenReport=report;sizeContract=report.contractReport;await onAssetsRefresh?.();const updated=await api.listAnimations(workspaceId,worktreeId);onAnimations(updated);const current=updated.find(animation=>animation.id===animationId);if(current){loadAnimation(current,true);onSelected(current);}await refreshDirectionMeta();if(report.jobId){const jobs=await api.listJobs(workspaceId,worktreeId);contractRetryJob=jobs.find(job=>job.id===report.jobId);}onNotice(`Production harden: ${report.steps.join(" → ")}${report.contractReport.passed?" · contract passed":" · review contract"}`);}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function runCleanAlpha(){if(!animationId)return;pipelineBusy=true;try{const report=await api.cleanAlphaAnimation(animationId);await onAssetsRefresh?.();previewEpoch+=1;onNotice(`Alpha cleanup: ${report.framesChanged}/${report.framesProcessed} frame(s) updated`);}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function runSnapGrid(){if(!animationId)return;pipelineBusy=true;try{const updated=await api.snapToPixelGrid(animationId,hardenGridSize);loadAnimation(updated,true);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshSizeContract(updated.id);onNotice(`Snapped frame offsets to ${hardenGridSize}px grid`);}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function queueRegionRegen(){if(!animationId||!conversationId){onError("Apri una conversazione chat prima di rigenerare una regione");return;}if(!currentAsset)return;const regions=maskRegions.length?maskRegions:[{x:regionX,y:regionY,width:regionW,height:regionH}];regionRegenBusy=true;try{const result=await api.queueRegionRegen({animationId,frameIndex:activeFrame,regions,brushStrokes:maskBrushStrokes,conversationId,prompt:regionPrompt.trim()||undefined});const jobs=await api.listJobs(workspaceId,worktreeId);regionRegenJob=jobs.find(job=>job.id===result.jobId)??{id:result.jobId,projectId:workspaceId,worktreeId,kind:"region_regen",targetType:"animation",targetId:animationId,status:"queued",progress:0,stage:"Queued",cancelRequested:false,createdAt:new Date().toISOString(),updatedAt:new Date().toISOString()};onNotice(`Regional regen accodato (job ${result.jobId}) per il frame ${activeFrame+1}`);}catch(error){onError(errorMessage(error));}finally{regionRegenBusy=false;}}
  async function refreshCurrentAssetVersions(){if(!currentAsset){currentAssetVersions=[];return;}try{currentAssetVersions=await api.listAssetVersions(currentAsset.id);}catch{currentAssetVersions=[];}}
  async function runSharedPalette(){if(!workspaceId||!worktreeId)return;paletteBusy=true;try{const report=await api.quantizeWorktreePalette({workspaceId,worktreeId,anchorSlug:guideAnchor?.slug,colorCount:hardenPaletteColorCount});await onAssetsRefresh?.();previewEpoch+=1;onNotice(`Palette condivisa: ${report.uniqueColorsBefore} → ${report.uniqueColorsAfter} colori su ${report.framesTouched} frame`);}catch(error){onError(errorMessage(error));}finally{paletteBusy=false;}}
  $effect(()=>{void refreshCurrentAssetVersions();});
  function openCharacterPackExport(){if(!worktreeId)return;packExportOpen=true;}
  async function exportCharacterPack(options:{includeAnimatedPreviews:boolean}){if(!worktreeId)return;const destination=await open({directory:true,multiple:false,title:"Export character pack"});if(typeof destination!=="string")return;pipelineBusy=true;try{const result=await api.exportCharacterPack({workspaceId,worktreeId,destination,anchorSlug:guideAnchor?.slug,metadataFormat:exportFormat,includeAnimatedPreviews:options.includeAnimatedPreviews});onNotice(`Exported character pack (${result.animationCount} animation${result.animationCount===1?"":"s"})`);await revealItemInDir(result.manifestPath);packExportOpen=false;}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  const productionScoreTone = (score: number) => score >= 85 ? "good" : score >= 65 ? "warning" : "error";
  async function nudgeFrame(dx:number,dy:number,all=false){if(!animationId)return;alignerBusy=true;try{const updated=await api.nudgeAnimationFrames({animationId,deltas:[{frameIndex:activeFrame,offsetX:dx,offsetY:dy}],applyToAll:all});loadAnimation(updated,true);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshSizeContract(updated.id);}catch(error){onError(errorMessage(error));}finally{alignerBusy=false;}}
  async function resetActiveFrame(){const frame=frames[activeFrame];if(!frame)return;const offsetX=frame.offsetX ?? 0;const offsetY=frame.offsetY ?? 0;if(!offsetX && !offsetY)return;await nudgeFrame(-offsetX,-offsetY);}
  async function resetAlignment(){if(!animationId)return;alignerBusy=true;try{const updated=await api.resetAnimationAlignmentOffsets(animationId);loadAnimation(updated,true);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));onNotice("Frame offsets reset");}catch(error){onError(errorMessage(error));}finally{alignerBusy=false;}}
  async function applyActiveOffsetToAll(){if(!animationId||!frames.length)return;const source=frames[activeFrame];const offsetX=source.offsetX ?? 0;const offsetY=source.offsetY ?? 0;alignerBusy=true;try{const updated=await api.nudgeAnimationFrames({animationId,deltas:frames.map((frame,index)=>({frameIndex:index,offsetX:offsetX-(frame.offsetX ?? 0),offsetY:offsetY-(frame.offsetY ?? 0)}))});loadAnimation(updated,true);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshSizeContract(updated.id);onNotice("Copied active frame offset to every frame");}catch(error){onError(errorMessage(error));}finally{alignerBusy=false;}}
  async function normalizeFrames(){if(!animationId)return;pipelineBusy=true;try{const updated=await api.normalizeAnimation({animationId,anchorSlug:guideAnchor?.slug,lockFirstFrame:true,sharedScale:true});await onAssetsRefresh?.();await refreshAfterRepair(updated);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshSizeContract(updated.id);onNotice("Normalized frames to anchor contract");}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function persistImportedFrames(assetIds: string[], label: string, warning?: string){await onAssetsRefresh?.();frames=[...frames,...assetIds.map(assetId=>({assetId}))];activeFrame=Math.max(0,frames.length-assetIds.length);if(animationId){const animation=await api.saveAnimation({id:animationId,workspaceId,worktreeId,name,fps:Number(fps),looping,frames});loadAnimation(animation,true);onSelected(animation);onAnimations(await api.listAnimations(workspaceId,worktreeId));}onNotice(warning?`Imported ${assetIds.length} ${label} — ${warning}`:`Imported ${assetIds.length} ${label}${animationId?" and saved":""}`);}
  async function importStrip(){const source=await open({multiple:false,title:"Import sprite strip",filters:[{name:"Image",extensions:["png","webp","jpg","jpeg"]}]});if(typeof source!=="string")return;pipelineBusy=true;try{stripScore=await api.scoreSpriteStrip({sourcePath:source,layout:"auto"});stripSourcePath=source;stripDialogMode="import";stripDialog=true;}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function importVideo(){const source=await open({multiple:false,title:"Import video frames",filters:[{name:"Video",extensions:["mp4","webm","mov","mkv","avi"]}]});if(typeof source!=="string")return;videoWizardPath=source;}
  async function confirmStripImport(frameCount:number,layout:string){if(!stripSourcePath)return;pipelineBusy=true;try{const result=await api.splitSpriteStrip({workspaceId,sourcePath:stripSourcePath,layout,frameCount,recoverForeground:true});const warning=result.warnings?.[0]??(result.suggestedFrameCount&&result.frameCountUsed&&result.suggestedFrameCount!==result.frameCountUsed?`Used ${result.frameCountUsed} frames (detected ${result.suggestedFrameCount})`:stripScore?.warnings?.[0]);await persistImportedFrames(result.assetIds,"frames from strip",warning);if(animationId)await refreshFrameScores(animationId);stripDialog=false;stripSourcePath="";}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function finalizeRegeneratedStrip(){if(!animationId)return;const source=await open({multiple:false,title:"Finalize regenerated strip",filters:[{name:"Image",extensions:["png","webp","jpg","jpeg"]}]});if(typeof source!=="string")return;pipelineBusy=true;try{stripScore=await api.scoreSpriteStrip({sourcePath:source,layout:"auto"});stripSourcePath=source;stripDialogMode="finalize";stripDialog=true;}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function confirmFinalizeStrip(frameCount:number,layout:string){if(!animationId||!stripSourcePath)return;pipelineBusy=true;try{const result=await api.finalizeContractRetry({animationId,sourcePath:stripSourcePath,frameCount,anchorSlug:guideAnchor?.slug,layout});sizeContract=result.finalReport;await onAssetsRefresh?.();const updated=await api.listAnimations(workspaceId,worktreeId);onAnimations(updated);const current=updated.find(animation=>animation.id===animationId);if(current){loadAnimation(current,true);onSelected(current);}await refreshFrameScores(animationId);onNotice(result.passed?"Regenerated strip passed the contract":result.nextStep ?? "Strip finalized — contract still has violations");stripDialog=false;stripSourcePath="";}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function completeVideoImport(assetIds:string[],options:{normalized:boolean;hardened:boolean;fps:number}){pipelineBusy=true;try{await persistImportedFrames(assetIds,`frames from video (${options.fps} FPS)`);if(animationId&&options.normalized){const updated=await api.normalizeAnimation({animationId,anchorSlug:guideAnchor?.slug,lockFirstFrame:true,sharedScale:true});await onAssetsRefresh?.();await refreshAfterRepair(updated);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshSizeContract(updated.id);}if(animationId&&options.hardened){const report=await api.hardenAnimation(animationId,guideAnchor?.slug,undefined,frames.length,conversationId,{cleanAlpha:hardenCleanAlpha,normalize:hardenNormalize,snapGrid:hardenSnapGrid,gridSize:hardenGridSize,queueContractRetry:hardenQueueRetry,quantizePalette:hardenQuantizePalette,paletteColorCount:hardenPaletteColorCount});lastHardenReport=report;sizeContract=report.contractReport;await onAssetsRefresh?.();}if(animationId)await refreshFrameScores(animationId);videoWizardPath=undefined;}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function refreshFrameScores(id:string){try{frameScoreReport=await api.scoreAnimationFrames(id);}catch{frameScoreReport=undefined;}}
  async function applyContractRetry(regenerate=false){if(!animationId)return;pipelineBusy=true;try{const result=await api.retrySizeContract({animationId,anchorSlug:guideAnchor?.slug,conversationId:regenerate?conversationId:undefined,regenerate});sizeContract=result.finalReport;await onAssetsRefresh?.();const updated=await api.listAnimations(workspaceId,worktreeId);onAnimations(updated);const current=updated.find(animation=>animation.id===animationId);if(current){loadAnimation(current,true);onSelected(current);}if(result.passed){onNotice("Size contract passed after retry");}else if(result.generationRequestId){onNotice(`Strip regeneration started in chat. ${result.nextStep ?? ""}`);}else{onNotice(result.nextStep ?? "Contract retry finished — review violations");}}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function handleContractRetryJobFinished(job:BackgroundJob,queueQuality=true){if(!animationId||job.targetId&&job.targetId!==animationId)return;contractRetryJob=job;await onAssetsRefresh?.();const report=await api.checkSizeContract(animationId,guideAnchor?.slug);sizeContract=report;const updated=await api.listAnimations(workspaceId,worktreeId);onAnimations(updated);const current=updated.find(animation=>animation.id===animationId);if(current){frames=current.frames.map(frame=>({...frame}));reviewStatus=current.reviewStatus ?? "draft";onSelected(current);}if(queueQuality&&job.status==="completed"){qualityOpen=true;qualityJob=await api.queueQualityAnalysis(animationId);}onNotice(report.passed?"Autonomous contract retry passed — quality analysis queued":job.errorMessage??"Autonomous contract retry finished — review violations");}
  async function queueAutonomousContractRetry(){if(!animationId||!conversationId){onError("Open a chat conversation before running autonomous contract retry");return;}pipelineBusy=true;try{const result=await api.queueContractRetry({animationId,conversationId,anchorSlug:guideAnchor?.slug});sizeContract=result.finalReport;if(result.jobId){const jobs=await api.listJobs(workspaceId,worktreeId);contractRetryJob=jobs.find(job=>job.id===result.jobId)??{id:result.jobId,projectId:workspaceId,worktreeId,kind:"contract_retry",targetType:"animation",targetId:animationId,status:"queued",progress:0,stage:"Queued",cancelRequested:false,createdAt:new Date().toISOString(),updatedAt:new Date().toISOString()};if(["completed","failed","cancelled"].includes(contractRetryJob.status))await handleContractRetryJobFinished(contractRetryJob);}onNotice(result.nextStep ?? "Autonomous contract retry queued");}catch(error){onError(errorMessage(error));}finally{pipelineBusy=false;}}
  async function acceptAnimation(){if(!animationId)return;alignerBusy=true;try{const updated=await api.setAnimationReviewStatus(animationId,"accepted");loadAnimation(updated,true);onSelected(updated);onAnimations(await api.listAnimations(workspaceId,worktreeId));onNotice("Animation accepted for export");}catch(error){onError(errorMessage(error));}finally{alignerBusy=false;}}
  let exportBlocked = $derived(Boolean(sizeContract && !sizeContract.passed && reviewStatus !== "accepted"));
  function startNewAnimationDraft() { animationId=undefined;name="New animation";fps=10;looping=true;frames=[];reviewStatus="draft";sizeContract=undefined;contractRetryJob=undefined;alignerOpen=false;activeFrame=0;playing=false;qualityOpen=false;qualityReport=undefined;qualityJob=undefined; }
  function selectAnimation(animation:Animation){loadAnimation(animation);onSelected(animation);}
  function addFrame(assetId: string) { frames = [...frames, {assetId}]; activeFrame = frames.length - 1; }
  function removeFrame(index: number) { frames = frames.filter((_, i) => i !== index); activeFrame = Math.max(0, Math.min(activeFrame, frames.length - 1)); }
  function duplicate(index: number) { frames = [...frames.slice(0,index+1), {...frames[index]}, ...frames.slice(index+1)]; activeFrame=index+1; }
  function move(from: number, to: number) { if(from===to||to<0||to>=frames.length)return;const next=[...frames];const [item]=next.splice(from,1);next.splice(to,0,item);frames=next;activeFrame=to; }
  function drop(event: DragEvent, index?: number) {
    event.preventDefault();
    if (draggedIndex !== undefined && index !== undefined) { move(draggedIndex,index); draggedIndex=undefined; return; }
    const id = event.dataTransfer?.getData("application/x-sprite-studio-asset") || event.dataTransfer?.getData("text/plain");
    if (id && assets.some(asset => asset.id === id)) {
      if(index === undefined) addFrame(id); else { frames=[...frames.slice(0,index),{assetId:id},...frames.slice(index)];activeFrame=index; }
    }
  }
  async function save() {
    saving=true;
    try {
      const animation=await api.saveAnimation({id:animationId,workspaceId,worktreeId,name,fps:Number(fps),looping,frames});
      selectedPropId=animation.id;animationId=animation.id;onSelected(animation);onAnimations(await api.listAnimations(workspaceId,worktreeId));onNotice("Animation saved");
    } catch(error){onError(errorMessage(error));throw error;} finally{saving=false;}
  }
  async function exportSheet() {
    if(!animationId){onError("Save the animation before exporting");return;}
    if(exportBlocked){onError("Resolve size-contract violations or accept the animation before exporting");return;}
    const destination=await open({directory:true,multiple:false,title:"Export spritesheet"});
    if(typeof destination!=="string")return;
    try{
      const result=await api.exportAnimation(animationId,destination,exportFormat);
      try{const response=await fetch(assetUrl(result.metadataPath));exportMetadataPreview=await response.text();}catch{exportMetadataPreview="";}
      onNotice(`Exported ${result.width}×${result.height} spritesheet (${result.format})`);
      await revealItemInDir(result.pngPath);
    }
    catch(error){onError(errorMessage(error));}
  }
  async function exportGifPreview() {
    if(!animationId){onError("Save the animation before exporting");return;}
    if(exportBlocked){onError("Resolve size-contract violations or accept the animation before exporting");return;}
    const destination=await open({directory:true,multiple:false,title:"Export animated GIF preview"});
    if(typeof destination!=="string")return;
    try{
      const result=await api.exportAnimationPreview({animationId,destination,format:"gif"});
      onNotice(`Exported GIF preview (${result.frameCount} frame${result.frameCount===1?"":"s"}, ${result.width}×${result.height})`);
      await revealItemInDir(result.gifPath);
    }
    catch(error){onError(errorMessage(error));}
  }
  async function createTemplate(value:{name:string;intent:string;motionDescription:string;frameMode:FrameMode;minFrames:number;maxFrames:number;generationPrompt:string;negativePrompt:string}) {
    if(!animationId)return;
    templateBusy=true;
    try{await api.createAnimationTemplate(animationId,value.name,value.intent,value.motionDescription,value.frameMode,value.minFrames,value.maxFrames,value.generationPrompt,value.negativePrompt);onTemplates(await api.listAnimationTemplates(workspaceId));templateDialog=false;onNotice("Reusable motion template saved");}
    catch(error){onError(errorMessage(error));}finally{templateBusy=false;}
  }
  async function prepareTemplate(targetAssetId:string){if(!applyingTemplate)return;templateBusy=true;try{const application=await api.applyAnimationTemplate(applyingTemplate.id,targetAssetId);onTemplateApplication(application);applyingTemplate=undefined;onNotice(`Prepared ${application.motionPlan.selectedFrameCount}-frame template application`);}catch(error){onError(errorMessage(error));}finally{templateBusy=false;}}
  async function loadQuality(id:string){try{qualityReport=(await api.getQualityReport(id))??undefined;await refreshFrameScores(id);await refreshSizeContract(id);await refreshProductionScore();}catch(error){onError(errorMessage(error));}}
  async function analyze(){if(!animationId){onError("Save the animation before analyzing it");return;}try{qualityOpen=true;void refreshFrameScores(animationId);qualityJob=await api.queueQualityAnalysis(animationId);onNotice("Quality analysis queued");}catch(error){onError(errorMessage(error));}}
  async function ignoreCheck(check:QualityCheck){try{await api.acknowledgeQualityCheck(check.id,true);if(animationId)await loadQuality(animationId);}catch(error){onError(errorMessage(error));}}
  async function repairCheck(check:QualityCheck){
    if(repairing||optimizing)return;
    if(check.frameIndex!==undefined)activeFrame=check.frameIndex;
    if(check.repairAction==="remove_duplicate"&&check.frameIndex!==undefined){
      removeFrame(check.frameIndex);
      if(animationId){
        saving=true;
        try{await save();onNotice(`Removed duplicate Frame ${check.frameIndex+1} and saved`);}
        catch(error){onError(errorMessage(error));}finally{saving=false;}
      }else{onNotice(`Removed duplicate Frame ${check.frameIndex+1} from the unsaved timeline`);}
      return;
    }
    if(animationId&&check.repairAction==="inspect_transparency"){
      repairing=true;qualityReport=undefined;
      try{const repaired=await api.repairAnimationTransparency(animationId);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshAfterRepair(repaired);onSelected(repaired);qualityOpen=true;qualityJob=await api.queueQualityAnalysis(animationId);onNotice("Repaired frame transparency and started re-analysis");}
      catch(error){onError(errorMessage(error));}finally{repairing=false;}return;
    }
    if(animationId&&["auto_align","add_padding","normalize_dimensions"].includes(check.repairAction??"")){
      repairing=true;qualityReport=undefined;
      try{const repaired=await api.repairAnimationAlignment(animationId);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshAfterRepair(repaired);onSelected(repaired);qualityOpen=true;qualityJob=await api.queueQualityAnalysis(repaired.id);onNotice("Created a preserved, aligned animation revision and started re-analysis");}
      catch(error){onError(errorMessage(error));}finally{repairing=false;}return;
    }
    if(animationId&&check.repairAction==="contract_auto_fix"){
      await applyContractRetry(false);
      return;
    }
    if(animationId&&check.repairAction==="regenerate_transition"){
      await optimizeFrames();
      return;
    }
    onNotice("Selected the affected frame. Regeneration guidance is available in the warning details.");
  }
  async function optimizeFrames(){if(!animationId||repairing||optimizing)return;optimizing=true;qualityReport=undefined;try{const result=await api.optimizeAnimationFrames(animationId,3);onAnimations(await api.listAnimations(workspaceId,worktreeId));await refreshAfterRepair(result.animation);onSelected(result.animation);qualityOpen=true;qualityJob=await api.queueQualityAnalysis(result.animation.id);onNotice(`${result.summary}. Created a preserved revision and started re-analysis`);}catch(error){onError(errorMessage(error));}finally{optimizing=false;}}
  onMount(()=>{const unlistenPromise=listen<JobEvent>("job-event",async({payload})=>{if(payload.job.kind==="quality_analysis"&&payload.job.id===qualityJob?.id){qualityJob=payload.job;if(payload.job.status==="completed"&&animationId){await loadQuality(animationId);onNotice("Animation quality report completed");}if(payload.job.status==="failed")onError(payload.job.errorMessage??"Quality analysis failed");return;}if(payload.job.kind==="motion_batch"){if(payload.job.worktreeId!==worktreeId&&payload.job.id!==motionBatchJob?.id)return;motionBatchJob=payload.job;motionBatchEntries=parseMotionBatchEntries(payload.job);if(payload.job.status==="completed")await handleMotionBatchJobFinished(payload.job);if(payload.job.status==="failed"){await handleMotionBatchJobFinished(payload.job,true);onError(payload.job.errorMessage??"Motion batch failed");}if(payload.job.status==="cancelled")onNotice("Motion batch cancelled");return;}if(payload.job.kind==="direction_set"){if(payload.job.targetId!==animationId&&payload.job.id!==directionSetJob?.id)return;directionSetJob=payload.job;if(payload.job.status==="completed")await handleDirectionSetJobFinished(payload.job);if(payload.job.status==="failed"){await handleDirectionSetJobFinished(payload.job,true);onError(payload.job.errorMessage??"Direction set failed");}if(payload.job.status==="cancelled")onNotice("Direction set cancelled");return;}if(payload.job.kind==="region_regen"){if(payload.job.targetId!==animationId&&payload.job.id!==regionRegenJob?.id)return;regionRegenJob=payload.job;if(payload.job.status==="completed"){await onAssetsRefresh?.();previewEpoch+=1;onNotice("Regional regen completato — frame aggiornato");}if(payload.job.status==="failed")onError(payload.job.errorMessage??"Regional regen failed");if(payload.job.status==="cancelled")onNotice("Regional regen annullato");return;}if(payload.job.kind!=="contract_retry")return;if(payload.job.targetId!==animationId&&payload.job.id!==contractRetryJob?.id)return;contractRetryJob=payload.job;if(payload.job.status==="completed")await handleContractRetryJobFinished(payload.job,true);if(payload.job.status==="failed"){await handleContractRetryJobFinished(payload.job,false);onError(payload.job.errorMessage??"Autonomous contract retry failed");}if(payload.job.status==="cancelled")onNotice("Autonomous contract retry cancelled");});return()=>{unlistenPromise.then(unlisten=>unlisten());};});
</script>

<section class="editor">
  <header><div><h1>Animation editor</h1><p>{animations.length} animation{animations.length===1?"":"s"} · {templates.length} template{templates.length===1?"":"s"} · {frames.length} frame{frames.length===1?"":"s"} · <span class={`status ${reviewStatus}`}>{reviewStatus}</span></p></div><div class="actions"><label class="export-format"><span>Format</span><select bind:value={exportFormat} disabled={exportBlocked||!animationId||!frames.length}><option value="sprite-studio">Sprite Studio JSON</option><option value="aseprite-json">Aseprite JSON</option><option value="texturepacker">TexturePacker</option><option value="godot-spriteframes">Godot SpriteFrames</option></select></label><button type="button" onclick={() => startNewAnimationDraft()}><Plus size={13}/> New</button><button onclick={()=>templateDialog=true} disabled={!animationId||!frames.length}><FileKey2 size={13}/> Save template</button><button onclick={()=>alignerOpen=!alignerOpen} disabled={!animationId||!frames.length}><Move size={13}/> Align</button>{#if productionScore}<button class={`production-score ${productionScoreTone(productionScore.overallScore)}`} onclick={()=>productionScoreOpen=!productionScoreOpen} disabled={!worktreeId} title="Production score"><Gauge size={13}/>{Math.round(productionScore.overallScore)}</button>{/if}<button onclick={()=>{qualityOpen=true;if(!qualityReport&&!qualityJob)void analyze()}} disabled={!animationId||!frames.length||repairing}><ShieldCheck size={13}/>{repairing?"Repairing…":qualityReport?Math.round(qualityReport.overallScore):"Quality"}</button><button onclick={openCharacterPackExport} disabled={!worktreeId||pipelineBusy||!animations.length} title="Export character pack"><Package size={13}/> Pack</button><button onclick={acceptAnimation} disabled={!animationId||alignerBusy||reviewStatus==="accepted"}><BadgeCheck size={13}/> Accept</button><button onclick={save} disabled={saving}><Save size={13}/>{saving?"Saving…":"Save"}</button><button onclick={exportGifPreview} disabled={!animationId || !frames.length || exportBlocked} title="Export animated GIF preview"><Film size={13}/> GIF</button><button class="primary" onclick={exportSheet} disabled={!animationId || !frames.length || exportBlocked}><Download size={13}/> Export</button></div></header>
  <div class="body" class:with-quality={qualityOpen || alignerOpen || Boolean(exportMetadataPreview)}>
    <aside class="animation-list"><div class="label">ANIMATIONS</div>{#each animations as animation}<button class:active={animation.id===animationId} onclick={()=>selectAnimation(animation)}><Clapperboard size={13}/><span>{animation.name}</span><small>{animation.frames.length}</small></button>{/each}{#if !animations.length}<p>No saved animations</p>{/if}<div class="label templates-label">MOTION TEMPLATES</div>{#each templates as template}<button class="template" onclick={()=>applyingTemplate=template}><FileKey2 size={13}/><span>{template.name}</span><small>{template.frameMode==="auto"?`${template.minFrames}–${template.maxFrames}`:template.preferredFrames}</small></button>{/each}{#if !templates.length}<p>Save an animation as reusable motion</p>{/if}</aside>
    <div class="workspace">
      <div class="properties"><label>Name<input bind:value={name}/></label>
        {#if nativeRigLabel}<div class="rig-link"><Bone size={12}/><span>{nativeRigLabel}{#if linkedWorkspaceRig} · {linkedWorkspaceRig.frameCount} poses{/if}</span>{#if effectiveLinkedRigId && onOpenRig}<button type="button" onclick={()=>onOpenRig(effectiveLinkedRigId)}>Open rig</button>{:else if linkedWorkspaceRig && workspacePath}<button type="button" onclick={revealWorkspaceRig}><FolderOpen size={11}/> Show JSON</button>{/if}</div>{/if}<label>FPS<input type="number" min="1" max="60" bind:value={fps}/></label><label class="check"><input type="checkbox" bind:checked={looping}/><Repeat2 size={12}/> Loop</label><label>Preview<select bind:value={scale}><option value={1}>1×</option><option value={2}>2×</option><option value={3}>3×</option><option value={4}>4×</option></select></label><label class="check"><input type="checkbox" bind:checked={onionSkin}/> Onion skin</label>{#if onionSkin}<label class="onion-opacity">Opacity<input aria-label="Onion skin opacity" type="range" min="0.08" max="0.55" step="0.01" bind:value={onionOpacity}/></label>{/if}</div>
      <div class="preview-area">
        <div class="preview-stage" style={`--guide-baseline:${(guideBaselineY / guideFrameHeight) * 100}%`}>
          {#if currentAsset}
            {#if alignerOpen && showAlignGhosts}
              {#each frames as frame, index}
                {@const asset = frameAsset(frame)}
                {#if asset && index !== activeFrame}
                  <img class="align-ghost" src={previewSrc(asset.path)} alt="" style={`transform:scale(${scale}) translate(${frame.offsetX ?? 0}px, ${frame.offsetY ?? 0}px)`}/>
                {/if}
              {/each}
            {/if}
            {#if alignerOpen}
              <span class="guide baseline" aria-hidden="true"></span>
              <span class="guide pivot" style={`left:calc(50% + ${((guidePivotX / (guideAnchor?.frameWidth ?? currentAsset.width)) - 0.5) * 45}% )`} aria-hidden="true"></span>
            {/if}
            {#if onionSkin && previousAsset}<img class="onion previous" src={previewSrc(previousAsset.path)} alt="Previous frame onion skin" style={`transform:scale(${scale}) translate(${frames[activeFrame > 0 ? activeFrame - 1 : looping ? frames.length - 1 : 0]?.offsetX ?? 0}px, ${frames[activeFrame > 0 ? activeFrame - 1 : looping ? frames.length - 1 : 0]?.offsetY ?? 0}px);opacity:${onionOpacity}`}/>{/if}
            {#if onionSkin && nextAsset}<img class="onion next" src={previewSrc(nextAsset.path)} alt="Next frame onion skin" style={`transform:scale(${scale}) translate(${frames[activeFrame < frames.length - 1 ? activeFrame + 1 : looping ? 0 : activeFrame]?.offsetX ?? 0}px, ${frames[activeFrame < frames.length - 1 ? activeFrame + 1 : looping ? 0 : activeFrame]?.offsetY ?? 0}px);opacity:${onionOpacity}`}/>{/if}
            <img class="current" src={previewSrc(currentAsset.path)} alt={currentAsset.name} style={`transform:scale(${scale}) translate(${frames[activeFrame]?.offsetX ?? 0}px, ${frames[activeFrame]?.offsetY ?? 0}px)`}/>
            {#if alignerOpen}
              <div class="mask-overlay-wrap">
                {#if maskOverlayMode === "regen"}
                  <FrameMaskOverlay imageSrc={previewSrc(currentAsset.path)} frameWidth={currentAsset.width} frameHeight={currentAsset.height} {scale} mode="regen" bind:regions={maskRegions} bind:brushStrokes={maskBrushStrokes} bind:regionX bind:regionY bind:regionW bind:regionH />
                {:else}
                  <FrameMaskOverlay imageSrc={previewSrc(currentAsset.path)} frameWidth={currentAsset.width} frameHeight={currentAsset.height} {scale} mode="erase" bind:brushStrokes={cleanupBrushStrokes} bind:regionX bind:regionY bind:regionW bind:regionH />
                {/if}
              </div>
            {/if}
          {:else}<div class="preview-empty"><Clapperboard size={25}/><span>Drop image assets into the timeline</span></div>{/if}
        </div>
        <div class="playback"><button onclick={()=>activeFrame=0} title="First frame"><SkipBack size={14}/></button><button class="play" onclick={()=>playing=!playing} disabled={!frames.length} title={playing?"Pause animation":"Play animation"}>{#if playing}<Pause size={15}/>{:else}<Play size={15} fill="currentColor"/>{/if}</button><button onclick={()=>activeFrame=Math.min(frames.length-1,activeFrame+1)} title="Next frame"><SkipForward size={14}/></button><span>Frame {frames.length ? activeFrame+1 : 0} / {frames.length}</span></div>
      </div>
      <div class="timeline" role="region" aria-label="Animation timeline" ondragover={(event)=>event.preventDefault()} ondrop={(event)=>drop(event)}>
        <div class="timeline-head"><span>TIMELINE</span><small>Drop assets here · default {Math.round(1000/fps)} ms/frame</small></div>
        <div class="frames">
          {#each frames as frame,index}
            {@const asset=frameAsset(frame)}
            <button class="frame" class:active={index===activeFrame} class:quality-warning={frameSeverity(index)==="warning"} class:quality-error={frameSeverity(index)==="error"} class:quality-good={frameSeverity(index)==="good"} onclick={()=>activeFrame=index} draggable="true" ondragstart={()=>draggedIndex=index} ondragover={(event)=>event.preventDefault()} ondrop={(event)=>{event.stopPropagation();drop(event,index)}}>
              <span class="number">{String(index+1).padStart(2,"0")}{#if frameSeverity(index)}<i class={frameSeverity(index)}></i>{/if}</span><span class="frame-image">{#if asset}<img src={previewSrc(asset.path)} alt={asset.name}/>{:else}<span class="missing">!</span>{/if}</span>
              <span class="duration"><input type="number" min="16" max="5000" value={frame.durationMs ?? Math.round(1000/fps)} onchange={(event)=>{const next=[...frames];next[index]={...frame,durationMs:Number(event.currentTarget.value)};frames=next;}}/> ms</span>
              <span class="frame-actions"><span role="button" tabindex="0" title="Duplicate" onclick={(event)=>{event.stopPropagation();duplicate(index)}} onkeydown={(event)=>{if(event.key==="Enter"){event.stopPropagation();duplicate(index)}}}><Copy size={11}/></span><span role="button" tabindex="0" title="Remove" onclick={(event)=>{event.stopPropagation();removeFrame(index)}} onkeydown={(event)=>{if(event.key==="Enter"){event.stopPropagation();removeFrame(index)}}}><Trash2 size={11}/></span><GripVertical size={11}/></span>
            </button>
          {/each}
          {#if !frames.length}<div class="drop-target">Drop frames from the asset browser or choose below</div>{/if}
        </div>
        <div class="asset-tray"><span>ADD FRAME</span><div>{#each assets as asset}<button onclick={()=>addFrame(asset.id)} title={`Add ${asset.name}`}><img src={assetUrl(asset.path)} alt={asset.name}/></button>{/each}{#if !assets.length}<small>Import assets first</small>{/if}</div></div>
      </div>
    </div>
    {#if exportMetadataPreview}<aside class="export-preview"><header><span>Export metadata</span><button onclick={()=>exportMetadataPreview=""}>Close</button></header><pre>{exportMetadataPreview}</pre></aside>{/if}
    {#if alignerOpen}<aside class="aligner-panel"><header><span>Frame aligner</span><button onclick={()=>alignerOpen=false}>Close</button></header>
      <div class="direction-set"><strong>Direction set</strong><label>Set<select bind:value={directionSet}><option value="4">4-way</option><option value="8">8-way</option></select></label><div class="facing-grid">{#each directionFacings as facing}<span class:filled={facingIsFilled(facing)} class:current={currentAnimationFacing===facing} class:issue={facingHasContractIssue(facing)} class:pending={facingBadge(facing)==="pending"} class:mirrored={facingBadge(facing)==="mirrored"} class:ai={facingBadge(facing)==="ai"} title={facingMetaFor(facing)?.mirroredFrom?`Mirrored from ${facingMetaFor(facing)?.mirroredFrom}`:facingBadge(facing)}>{facing.toUpperCase()}{#if facingBadge(facing)==="pending"}<small>?</small>{:else if facingBadge(facing)==="mirrored"}<small>M</small>{:else if facingBadge(facing)==="ai"}<small>AI</small>{/if}</span>{/each}</div>{#if directionSetNeedsConversation}<p class="warn">Facings mancanti ({estimatePendingFacings().join(", ")}) richiedono una conversazione chat per la generazione AI.</p>{/if}<button class="wide" disabled={!animationId||directionSetBusy} onclick={mirrorEastFromWest}>Mirror E from W</button><button class="wide" disabled={!animationId||!worktreeId||directionSetBusy||directionSetActive} onclick={queueDirectionSetJob}>{directionSetActive?"Direction set running…":"Queue direction set"}</button>{#if directionSetActive&&directionSetJob}<article class="contract-job"><div><strong>{directionSetJob.stage}</strong><span>{Math.round(directionSetJob.progress*100)}%</span></div><i><b style={`width:${directionSetJob.progress*100}%`}></b></i></article>{/if}
        <MotionBatchPanel presets={motionPresets} selectedIds={selectedMotionIds} missingIds={missingMotionIds} activeCategory={motionBatchCategory} search={motionBatchSearch} onlyMissing={motionOnlyMissing} busy={motionBatchBusy} active={motionBatchActive} needsConversation={motionBatchNeedsConversation} job={motionBatchJob} entries={motionBatchEntries} disabled={!worktreeId || !guideAnchor?.slug} onToggle={toggleMotionPreset} onSelectAllMissing={selectAllMissingMotions} onCategory={(value)=>motionBatchCategory=value} onSearch={(value)=>motionBatchSearch=value} onOnlyMissing={(value)=>motionOnlyMissing=value} onQueue={queueMotionBatchJob} />
        <button class="wide" disabled={!worktreeId||directionSetBusy} onclick={refreshCharacterContract}>Check character contract</button>{#if characterContract}<p class={characterContract.passed?"good":"warn"}>{characterContract.passed?"Worktree contract passed":`${characterContract.crossFacingViolations?.length ?? 0} cross-facing issue(s)`}</p>{/if}</div>
      <div class="region-regen"><strong>Regen regione</strong><p class="muted tiny">Overlay rect/brush sul frame attivo ({activeFrame + 1}). Richiede chat attiva.</p><div class="mask-mode"><button class:active={maskOverlayMode==="regen"} onclick={()=>maskOverlayMode="regen"}>Regen mask</button><button class:active={maskOverlayMode==="erase"} onclick={()=>maskOverlayMode="erase"}>Cleanup</button></div><div class="region-grid"><label>X<input type="number" min="0" bind:value={regionX}/></label><label>Y<input type="number" min="0" bind:value={regionY}/></label><label>W<input type="number" min="1" bind:value={regionW}/></label><label>H<input type="number" min="1" bind:value={regionH}/></label></div>{#if maskOverlayMode==="erase"&&currentAsset}<FrameCleanupToolbar assetId={currentAsset.id} strokes={cleanupBrushStrokes} versions={currentAssetVersions} busy={pipelineBusy} onApplied={async()=>{await onAssetsRefresh?.();previewEpoch+=1;await refreshCurrentAssetVersions();}} onError={onError} onNotice={onNotice}/>{/if}<label class="region-prompt">Prompt opzionale<textarea rows="2" bind:value={regionPrompt} placeholder="Es. correggi l'artiglio sinistro"></textarea></label><button class="wide" disabled={!animationId||!conversationId||!currentAsset||regionRegenBusy||maskOverlayMode!=="regen"} onclick={queueRegionRegen}><Paintbrush size={12}/>{regionRegenBusy?"Regen…":"Regen region"}</button>{#if regionRegenJob&&animationId&&regionRegenJob.targetId===animationId&&["queued","running","analyzing"].includes(regionRegenJob.status)}<article class="contract-job"><div><strong>{regionRegenJob.stage}</strong><span>{Math.round(regionRegenJob.progress*100)}%</span></div><i><b style={`width:${regionRegenJob.progress*100}%`}></b></i></article>{/if}</div>
      <div class="production-tools"><strong>Production</strong><label class="check"><input type="checkbox" bind:checked={hardenCleanAlpha}/> Clean alpha</label><label class="check"><input type="checkbox" bind:checked={hardenNormalize}/> Normalize</label><label class="check"><input type="checkbox" bind:checked={hardenSnapGrid}/> Snap grid</label><label class="grid-size">Grid<select bind:value={hardenGridSize}><option value={1}>1px</option><option value={2}>2px</option><option value={4}>4px</option></select></label><label class="check"><input type="checkbox" bind:checked={hardenQuantizePalette}/> Shared palette on harden</label><label class="grid-size">Palette<select bind:value={hardenPaletteColorCount}><option value={8}>8</option><option value={16}>16</option><option value={32}>32</option></select></label><label class="check"><input type="checkbox" bind:checked={hardenQueueRetry}/> Auto contract retry</label><button class="wide primary-production" disabled={!animationId||!frames.length||pipelineBusy} onclick={runHarden}><Hammer size={12}/>{pipelineBusy?"Production…":"Run production harden"}</button><button class="wide" disabled={!worktreeId||paletteBusy||pipelineBusy} onclick={runSharedPalette}>Palette worktree ({hardenPaletteColorCount})</button><button class="wide" disabled={!animationId||!frames.length||pipelineBusy} onclick={runCleanAlpha}><Eraser size={12}/> Clean alpha only</button><button class="wide" disabled={!animationId||!frames.length||pipelineBusy} onclick={runSnapGrid}><Grid3x3 size={12}/> Snap offsets</button>{#if lastHardenReport}<p class="muted tiny">Steps: {lastHardenReport.steps.join(" → ")}</p>{/if}</div>
      <p class="muted">Nudge the active frame by 1px. Offsets are stored in metadata, not baked into PNGs.</p><label class="check"><input type="checkbox" bind:checked={showAlignGhosts}/> Ghost all frames</label><label class="check"><input type="checkbox" bind:checked={nudgeAllFrames}/> Apply nudge to all frames</label><div class="nudge-grid"><button disabled={alignerBusy} onclick={()=>nudgeFrame(0,-1,nudgeAllFrames)}>↑</button><button disabled={alignerBusy} onclick={()=>nudgeFrame(-1,0,nudgeAllFrames)}>←</button><button disabled={alignerBusy} onclick={()=>nudgeFrame(1,0,nudgeAllFrames)}>→</button><button disabled={alignerBusy} onclick={()=>nudgeFrame(0,1,nudgeAllFrames)}>↓</button></div><button class="wide" disabled={alignerBusy} onclick={applyActiveOffsetToAll}>Apply active offset to all</button><button class="wide" disabled={alignerBusy} onclick={resetActiveFrame}>Reset active frame</button><button class="wide" disabled={alignerBusy} onclick={resetAlignment}><RotateCcw size={12}/> Reset all to auto-align</button><div class="pipeline-tools"><strong>Pipeline</strong><button class="wide" disabled={!animationId||pipelineBusy} onclick={normalizeFrames}><Layers size={12}/> Normalize to anchor</button><button class="wide" disabled={pipelineBusy} onclick={importStrip}><Scissors size={12}/> Import strip</button><button class="wide" disabled={pipelineBusy} onclick={importVideo}><Film size={12}/> Import video</button><button class="wide" disabled={!animationId||pipelineBusy} onclick={()=>applyContractRetry(false)}><RefreshCw size={12}/> Fix contract</button><button class="wide" disabled={!animationId||!conversationId||pipelineBusy||contractRetryActive} onclick={queueAutonomousContractRetry}><Sparkles size={12}/>{contractRetryActive?"Auto-fix running…":"Auto-fix contract"}</button>{#if contractRetryActive&&contractRetryJob}<article class="contract-job"><div><strong>{contractRetryJob.stage}</strong><span>{Math.round(contractRetryJob.progress*100)}%</span></div><i><b style={`width:${contractRetryJob.progress*100}%`}></b></i><button class="cancel-job" disabled={pipelineBusy} onclick={cancelContractRetryJob}><X size={10}/>Cancel</button></article>{/if}<button class="wide" disabled={!animationId||pipelineBusy} onclick={finalizeRegeneratedStrip}>Finalize AI strip</button><p class="muted tiny">Fix runs normalize/nudge only. Auto-fix queues repair, AI regeneration, import, and re-check in the background. Poll stages in MCP with get_job. Finalize is for manual strip import.</p></div>{#if sizeContract}<div class="contract"><strong>Size contract</strong>{#if sizeContract.passed}<p class="good">Passed</p>{:else}<ul>{#each sizeContract.violations as violation}<li class:block={violation.blocking}>{violation.message}</li>{/each}</ul>{/if}</div>{/if}</aside>{/if}
    {#if productionScoreOpen&&productionScore}<aside class="production-score-panel"><header><span>Production score</span><button onclick={()=>productionScoreOpen=false}>Close</button></header><div class={`score ${productionScoreTone(productionScore.overallScore)}`}><strong>{Math.round(productionScore.overallScore)}</strong><span>/ 100</span></div><dl><div><dt>Size contract</dt><dd>{Math.round(productionScore.sizeContractScore)}</dd></div><div><dt>Character contract</dt><dd>{Math.round(productionScore.characterContractScore)}</dd></div><div><dt>Quality avg</dt><dd>{Math.round(productionScore.qualityScore)}</dd></div><div><dt>Animations</dt><dd>{productionScore.animationCount}</dd></div><div><dt>With quality report</dt><dd>{productionScore.animationsWithQuality}</dd></div></dl><button class="wide" disabled={!worktreeId||pipelineBusy} onclick={openCharacterPackExport}><Package size={12}/> Export character pack</button></aside>{/if}
    {#if qualityOpen}<QualityPanel report={qualityReport} job={qualityJob} contractReport={sizeContract} productionScore={productionScore} frameScoreReport={frameScoreReport} contractRetryActive={contractRetryActive} {canOptimize} {optimizing} repairing={repairing} onAnalyze={analyze} onOptimize={optimizeFrames} onFrame={(index)=>activeFrame=index} onIgnore={ignoreCheck} onRepair={repairCheck} onContractAutoFix={()=>applyContractRetry(false)} onQueueAutonomousRetry={queueAutonomousContractRetry} onClose={()=>qualityOpen=false}/>{/if}
  </div>
</section>
{#if templateDialog}<TemplateDialog animationName={name} frameCount={frames.length} busy={templateBusy} onCreate={createTemplate} onClose={()=>templateDialog=false}/>{/if}
{#if applyingTemplate}<TemplateApplyDialog template={applyingTemplate} {assets} busy={templateBusy} onApply={prepareTemplate} onClose={()=>applyingTemplate=undefined}/>{/if}
{#if stripDialog}<ContractRetryDialog title={stripDialogMode==="finalize"?"Finalize regenerated strip":"Import sprite strip"} subtitle={stripDialogMode==="finalize"?"Confirm frame count for contract retry import.":"Split the strip into workspace frames."} score={stripScore} defaultFrameCount={stripScore?.suggestedFrameCount ?? (frames.length || 4)} busy={pipelineBusy} onClose={()=>{stripDialog=false;stripSourcePath="";}} onConfirm={(frameCount, layout)=>stripDialogMode==="finalize"?confirmFinalizeStrip(frameCount, layout):confirmStripImport(frameCount, layout)}/>{/if}
{#if videoWizardPath}<VideoImportWizard {workspaceId} videoPath={videoWizardPath} busy={pipelineBusy} onClose={()=>videoWizardPath=undefined} onError={onError} onComplete={completeVideoImport}/>{/if}
{#if packExportOpen}<CharacterPackExportDialog busy={pipelineBusy} onClose={()=>packExportOpen=false} onExport={exportCharacterPack}/>{/if}

<style>
  .editor{height:100%;display:flex;flex-direction:column;background:var(--bg);min-width:0}header{height:49px;box-sizing:border-box;border-bottom:1px solid var(--border);display:flex;align-items:center;justify-content:space-between;padding:0 13px 0 17px}header h1{font-size:12px;margin:0}header p{font-size:11px;color:var(--faint);margin:3px 0 0}.actions{display:flex;gap:5px;align-items:center}.export-format{display:flex;align-items:center;gap:6px;font-size:10px;color:var(--faint)}.export-format select{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;font:inherit;font-size:10px;padding:0 6px}.actions button{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:5px;display:flex;align-items:center;gap:5px;padding:0 8px;font:inherit;font-size:11px;cursor:pointer}.actions button.primary{background:var(--text);color:var(--bg);border-color:var(--text)}.actions button.production-score.good{color:#5ead7b}.actions button.production-score.warning{color:#c89a4b}.actions button.production-score.error{color:#cc6863}button:disabled{opacity:.4;cursor:not-allowed}.production-score-panel{border-left:1px solid var(--border);background:var(--sidebar);padding:12px;overflow:auto;max-width:260px}.production-score-panel header{display:flex;justify-content:space-between;align-items:center;font-size:11px;color:var(--muted);margin-bottom:10px}.production-score-panel header button{border:0;background:transparent;color:var(--faint);cursor:pointer}.production-score-panel .score{text-align:center;margin-bottom:12px}.production-score-panel .score strong{font-size:28px}.production-score-panel .score span{font-size:10px;color:var(--faint)}.production-score-panel .score.good strong{color:#5ead7b}.production-score-panel .score.warning strong{color:#c89a4b}.production-score-panel .score.error strong{color:#cc6863}.production-score-panel dl{margin:0 0 12px;display:flex;flex-direction:column;gap:8px}.production-score-panel dl div{display:flex;justify-content:space-between;font-size:11px;color:var(--muted)}.production-score-panel dt{color:var(--faint)}.production-score-panel .wide{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;gap:6px;align-items:center;justify-content:center;font:inherit;font-size:11px;cursor:pointer;width:100%}.export-preview{border-left:1px solid var(--border);background:var(--sidebar);padding:12px;overflow:auto;max-width:360px}.export-preview header{display:flex;justify-content:space-between;align-items:center;font-size:11px;color:var(--muted);margin-bottom:8px}.export-preview header button{border:0;background:transparent;color:var(--faint);cursor:pointer}.export-preview pre{margin:0;font-size:10px;line-height:1.45;color:var(--muted);white-space:pre-wrap;word-break:break-word}
  .body{flex:1;min-height:0;display:grid;grid-template-columns:190px minmax(0,1fr)}.body.with-quality{grid-template-columns:190px minmax(0,1fr) 285px}header .status{text-transform:uppercase;font-size:10px}.status.accepted{color:#5ead7b}.status.rejected{color:#cc6863}.status.draft{color:var(--faint)}
  .aligner-panel{border-left:1px solid var(--border);background:var(--sidebar);padding:12px;display:flex;flex-direction:column;gap:10px;overflow:auto}.aligner-panel .tiny{font-size:10px;margin:0}.aligner-panel header{display:flex;justify-content:space-between;align-items:center;font-size:11px;color:var(--muted)}.aligner-panel header button{border:0;background:transparent;color:var(--faint);cursor:pointer}.nudge-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:6px;max-width:120px}.nudge-grid button{height:30px;border:1px solid var(--border);background:var(--surface);color:var(--text);border-radius:4px;cursor:pointer}.nudge-grid button:nth-child(1){grid-column:2}.nudge-grid button:nth-child(2){grid-column:1;grid-row:2}.nudge-grid button:nth-child(3){grid-column:3;grid-row:2}.nudge-grid button:nth-child(4){grid-column:2;grid-row:2}.aligner-panel .wide{height:28px;border:1px solid var(--border);background:var(--surface);color:var(--muted);border-radius:4px;display:flex;gap:6px;align-items:center;justify-content:center;font:inherit;font-size:11px;cursor:pointer}.contract{border-top:1px solid var(--border);padding-top:10px;font-size:11px;color:var(--muted)}.contract ul{margin:8px 0 0;padding-left:16px}.contract li.block{color:#cc6863}.contract .good{color:#5ead7b;margin:6px 0 0}.animation-list{border-right:1px solid var(--border);background:var(--sidebar);padding:12px 8px;overflow:auto}.label{font-size:10px;color:var(--faint);letter-spacing:.13em;font-weight:700;padding:4px 7px 7px}.templates-label{border-top:1px solid var(--border);margin-top:10px;padding-top:14px}.animation-list>button{width:100%;height:29px;border:0;background:transparent;color:var(--muted);border-radius:4px;display:grid;grid-template-columns:14px minmax(0,1fr) 24px;gap:6px;align-items:center;text-align:left;padding:0 7px;font:inherit;font-size:12px;cursor:pointer}.animation-list>button.active,.animation-list>button:hover{background:var(--selected);color:var(--text)}.animation-list>button.template :global(svg){color:var(--accent)}.animation-list button span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.animation-list button small{font-size:9px;color:var(--faint);text-align:right}.animation-list p{font-size:10px;line-height:1.45;color:var(--faint);padding:3px 7px}
  .workspace{min-width:0;min-height:0;display:grid;grid-template-rows:46px minmax(220px,1fr) 255px}.properties{border-bottom:1px solid var(--border);display:flex;align-items:center;gap:14px;padding:0 14px}.properties label{font-size:10px;color:var(--faint);display:flex;align-items:center;gap:6px}.properties label:first-child input{width:150px}.properties input[type="number"]{width:48px}.properties input,.properties select{height:25px;box-sizing:border-box;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:11px;padding:0 6px;outline:0}.properties .check{color:var(--muted)}.properties .check input{height:auto}.properties select{width:50px}.properties .onion-opacity{gap:4px}.properties .onion-opacity input{width:58px;height:auto;padding:0;accent-color:var(--accent)}.rig-link{display:flex;align-items:center;gap:6px;margin-left:auto;padding:4px 8px;border:1px solid var(--border);border-radius:5px;background:var(--surface);font-size:10px;color:var(--muted)}.rig-link :global(svg){color:var(--accent)}.rig-link button{height:22px;border:1px solid var(--border);border-radius:4px;background:var(--bg);color:var(--text);font:inherit;font-size:9px;padding:0 6px;cursor:pointer}
  .preview-area{min-height:0;display:flex;flex-direction:column;align-items:center;justify-content:center;background:var(--bg);overflow:hidden}.preview-stage{position:relative;width:330px;height:230px;display:grid;place-items:center;background-color:var(--preview);background-image:linear-gradient(45deg,var(--checker) 25%,transparent 25%),linear-gradient(-45deg,var(--checker) 25%,transparent 25%),linear-gradient(45deg,transparent 75%,var(--checker) 75%),linear-gradient(-45deg,transparent 75%,var(--checker) 75%);background-size:16px 16px;background-position:0 0,0 8px,8px -8px,-8px 0;border:1px solid var(--border-strong);box-shadow:0 14px 36px #0005}.preview-stage img{position:absolute;max-width:45%;max-height:45%;object-fit:contain;image-rendering:pixelated;transform-origin:center}.preview-stage img.current{z-index:2}.preview-stage img.onion{z-index:1;filter:saturate(.35)}.preview-stage img.onion.previous{mix-blend-mode:screen;filter:sepia(1) saturate(5) hue-rotate(160deg)}.preview-stage img.onion.next{mix-blend-mode:screen;filter:sepia(1) saturate(5) hue-rotate(285deg)}.preview-stage img.align-ghost{z-index:1;opacity:.16;filter:saturate(.2)}.preview-stage .guide{position:absolute;pointer-events:none;z-index:3}.preview-stage .guide.baseline{left:8%;right:8%;bottom:calc(50% - var(--guide-baseline) * 0.45%);height:1px;background:#f3be6288;box-shadow:0 0 0 1px #0003}.preview-stage .guide.pivot{top:8%;bottom:8%;width:1px;background:#64b5df88;transform:translateX(-50%)}.aligner-panel .check{font-size:11px;color:var(--muted);display:flex;align-items:center;gap:6px}.aligner-panel .check input{height:auto}.direction-set{border:1px solid var(--border);border-radius:6px;padding:10px;display:flex;flex-direction:column;gap:8px;font-size:11px;color:var(--muted)}.direction-set label{display:flex;justify-content:space-between;align-items:center;gap:8px}.direction-set select{height:24px;border:1px solid var(--border);background:var(--surface);color:var(--text);border-radius:4px;font:inherit;font-size:10px}.facing-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:4px}.facing-grid span{border:1px solid var(--border);border-radius:4px;text-align:center;padding:4px 0;font-size:9px;color:var(--faint)}.facing-grid span.filled{border-color:var(--accent);color:var(--text)}.facing-grid span.current{background:var(--accent-dim)}.facing-grid span.issue{border-color:#94504c;color:#cc6863}.facing-grid span.pending{opacity:.55;border-style:dashed}.facing-grid span.mirrored{border-color:#5ead7b}.facing-grid span.ai{border-color:#c89a4b}.facing-grid span small{display:block;font-size:7px;margin-top:2px;color:var(--faint)}.direction-set .good{color:#5ead7b;margin:0}.direction-set .warn{color:#c89a4b;margin:0}.motion-batch{border-top:1px solid var(--border);padding-top:10px;display:flex;flex-direction:column;gap:8px}.preset-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:4px}.preset-check{display:flex;align-items:center;gap:4px;font-size:10px;color:var(--muted);cursor:pointer}.preset-check input{height:auto}.preset-check small{margin-left:auto;color:var(--faint)}.batch-table{width:100%;border-collapse:collapse;font-size:9px;margin-top:4px}.batch-table th,.batch-table td{border:1px solid var(--border);padding:3px 4px;text-align:left;vertical-align:top}.batch-table tr.completed td:last-child{color:#5ead7b}.batch-table tr.failed td:last-child,.batch-table tr.skipped td:last-child{color:#c89a4b}.batch-table small{display:block;color:var(--faint);margin-top:2px}.mask-overlay-wrap{position:absolute;inset:0;z-index:4;display:grid;place-items:center}.mask-mode{display:flex;gap:6px}.mask-mode button{height:24px;border:1px solid var(--border);border-radius:4px;background:var(--surface);color:var(--muted);font:inherit;font-size:10px;padding:0 8px;cursor:pointer}.mask-mode button.active{border-color:var(--accent);color:var(--text)}.region-regen{border:1px solid var(--border);border-radius:6px;padding:10px;display:flex;flex-direction:column;gap:8px;font-size:11px;color:var(--muted)}.region-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:6px}.region-grid label{font-size:9px;color:var(--faint)}.region-grid input{width:100%;height:24px;box-sizing:border-box;margin-top:3px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:0 4px}.region-prompt{display:flex;flex-direction:column;gap:4px;font-size:9px;color:var(--faint)}.region-prompt textarea{resize:vertical;min-height:42px;background:var(--surface);border:1px solid var(--border);border-radius:4px;color:var(--text);font:inherit;font-size:10px;padding:6px}.production-tools{border:1px solid var(--border);border-radius:6px;padding:10px;display:flex;flex-direction:column;gap:8px;font-size:11px;color:var(--muted)}.production-tools .grid-size{display:flex;justify-content:space-between;align-items:center;gap:8px}.production-tools select{height:24px;border:1px solid var(--border);background:var(--surface);color:var(--text);border-radius:4px;font:inherit;font-size:10px}.production-tools .primary-production{background:var(--text);color:var(--bg);border-color:var(--text)}.pipeline-tools{border-top:1px solid var(--border);padding-top:10px;display:flex;flex-direction:column;gap:8px;font-size:11px;color:var(--muted)}.contract-job{padding:8px 0;border-top:1px solid var(--border)}.contract-job>div{display:flex;justify-content:space-between;gap:8px}.contract-job strong,.contract-job span{font-size:10px}.contract-job span{color:var(--faint)}.contract-job i{display:block;height:3px;background:var(--selected);margin-top:6px}.contract-job b{display:block;height:100%;background:var(--accent)}.cancel-job{border:0;background:transparent;color:var(--faint);font:inherit;font-size:10px;display:flex;gap:4px;align-items:center;margin-top:6px;cursor:pointer}.cancel-job:disabled{opacity:.4;cursor:not-allowed}.preview-empty{display:flex;flex-direction:column;gap:10px;align-items:center;color:var(--faint);font-size:11px}.playback{display:flex;align-items:center;gap:5px;margin-top:14px}.playback button{width:27px;height:27px;border:1px solid var(--border-strong);background:var(--surface);color:var(--muted);border-radius:4px;display:grid;place-items:center;cursor:pointer}.playback .play{width:34px;background:var(--text);color:var(--bg)}.playback>span{font-size:11px;color:var(--faint);margin-left:8px}
  .timeline{border-top:1px solid var(--border);background:var(--sidebar);min-width:0;overflow:hidden}.timeline-head{height:30px;display:flex;align-items:center;justify-content:space-between;padding:0 13px}.timeline-head span,.asset-tray>span{font-size:10px;color:var(--faint);font-weight:700;letter-spacing:.12em}.timeline-head small{font-size:10px;color:var(--faint)}.frames{height:143px;display:flex;gap:6px;padding:0 12px 7px;overflow-x:auto}.frame{width:98px;min-width:98px;height:138px;border:1px solid var(--border);background:var(--surface);color:var(--muted);padding:0;border-radius:5px;display:grid;grid-template-rows:20px 72px 22px 20px;cursor:pointer;overflow:hidden}.frame.active{border-color:var(--accent);box-shadow:0 0 0 1px var(--accent-dim)}.frame.quality-warning:not(.active){border-color:#8f6c36}.frame.quality-error:not(.active){border-color:#94504c}.number{font-size:10px;color:var(--faint);display:flex;align-items:center;padding:0 6px}.number i{width:6px;height:6px;border-radius:50%;margin-left:auto}.number i.good{background:#5ead7b}.number i.warning{background:#c89a4b}.number i.error{background:#cc6863}.frame-image{display:grid;place-items:center;background:var(--preview)}.frame-image img{max-width:86%;max-height:86%;object-fit:contain;image-rendering:pixelated}.missing{color:#c56f6b}.duration{font-size:7px;color:var(--faint);display:flex;align-items:center;justify-content:center;gap:2px}.duration input{width:38px;border:0;border-bottom:1px solid var(--border);background:transparent;color:var(--muted);font:inherit;font-size:10px;text-align:right;outline:0}.frame-actions{border-top:1px solid var(--border);display:flex;align-items:center;justify-content:flex-end;gap:6px;padding:0 5px;color:var(--faint)}.frame-actions span{display:grid;place-items:center}.drop-target{height:136px;min-width:250px;border:1px dashed var(--border-strong);border-radius:5px;display:grid;place-items:center;color:var(--faint);font-size:11px}.asset-tray{height:72px;border-top:1px solid var(--border);padding:8px 12px;box-sizing:border-box;display:flex;gap:13px}.asset-tray>span{padding-top:5px}.asset-tray>div{display:flex;gap:5px;overflow-x:auto}.asset-tray button{width:45px;height:45px;min-width:45px;border:1px solid var(--border);background:var(--preview);border-radius:4px;padding:3px;cursor:pointer}.asset-tray button:hover{border-color:var(--accent)}.asset-tray img{width:100%;height:100%;object-fit:contain;image-rendering:pixelated}.asset-tray small{font-size:10px;color:var(--faint);padding:5px}
</style>
