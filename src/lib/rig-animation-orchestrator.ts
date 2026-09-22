import { api } from "$lib/api";
import { assetsFromManifestPaths } from "$lib/generation-reconcile";
import { findAssetByManifestPath } from "$lib/manifest-path";
import { cloneSuggestionDraft } from "$lib/rig-draft";
import type {
  AnimationPolishMode,
  Asset,
  ChatGenerationProfile,
  GenerationManifest,
  Rig,
  RigFitReport,
  RigInput,
  RigMorphology,
  RigRenderResult,
  RigSuggestion,
} from "$lib/types";

export type RigOrchestrationInput = {
  workspaceId: string;
  worktreeId?: string;
  assetId: string;
  assetName: string;
  motion: string;
  profile: ChatGenerationProfile;
  providerId: string;
  model?: string;
  reasoningEffort?: string;
  signal?: AbortSignal;
  onFitWarning?: (message: string) => void;
};

export type RigOrchestrationResult = {
  render: RigRenderResult;
  rig: Rig;
  fitReport?: RigFitReport;
  warning?: string;
};

function rigNameFromAsset(assetName: string, motion: string): string {
  const base = assetName.replace(/\.[^.]+$/, "").replace(/[_-]+/g, " ").trim() || "sprite";
  const motionHint = motion.split(/\s+/).slice(0, 3).join(" ");
  return `${base} — ${motionHint}`.slice(0, 72);
}

function buildRigInput(
  workspaceId: string,
  worktreeId: string | undefined,
  assetId: string,
  assetName: string,
  motion: string,
  profile: ChatGenerationProfile,
  suggestion: RigSuggestion,
): RigInput {
  const draft = cloneSuggestionDraft(suggestion);
  const morphology: RigMorphology = suggestion.morphology;
  const frameCount = profile.frameMode === "fixed"
    ? Math.max(2, profile.frames)
    : Math.max(2, Math.min(profile.maxFrames, draft.frames.length || profile.minFrames));
  const frames = draft.frames.length >= frameCount
    ? draft.frames.slice(0, frameCount)
    : draft.frames;
  return {
    workspaceId,
    worktreeId,
    assetId,
    name: rigNameFromAsset(assetName, motion),
    morphology,
    fps: profile.fps,
    looping: true,
    points: draft.points,
    bones: draft.bones,
    frames: frames.length >= 2 ? frames : draft.frames,
  };
}

function throwIfAborted(signal?: AbortSignal) {
  if (signal?.aborted) throw new DOMException("Orchestration cancelled", "AbortError");
}

/** True when animate should run the native rig renderer before any agent polish pass. */
export function shouldRunNativeRigFirst(
  command: string | undefined,
  polishMode: AnimationPolishMode,
  asset?: Asset,
): boolean {
  return command === "animate"
    && (polishMode === "rig" || polishMode === "ai-polish" || polishMode === "full-redraw")
    && Boolean(asset);
}

/** Run native rig-only animation: fit check, AI rig suggestion, save, render, optional retry. */
export async function orchestrateRigOnlyAnimation(
  input: RigOrchestrationInput,
  onActivity?: (line: string) => void,
): Promise<RigOrchestrationResult> {
  const emit = (line: string) => onActivity?.(line);
  throwIfAborted(input.signal);
  emit("Analyzing sprite fit for native rigging");
  const fitReport = await api.analyzeRigFit(input.assetId).catch(() => undefined);
  const fitWarning = fitReport?.warnings[0];
  if (fitWarning) {
    emit(fitWarning);
    input.onFitWarning?.(fitWarning);
  }
  throwIfAborted(input.signal);

  const suggest = async (motion: string) => {
    throwIfAborted(input.signal);
    emit("Suggesting joint points and pose frames");
    return api.aiSuggestRigPoints({
      assetId: input.assetId,
      motion,
      morphology: fitReport?.detections[0]?.morphology,
      providerId: input.providerId,
      model: input.model,
      reasoningEffort: input.reasoningEffort,
    });
  };

  let suggestion = await suggest(input.motion);
  let rigInput = buildRigInput(
    input.workspaceId,
    input.worktreeId,
    input.assetId,
    input.assetName,
    input.motion,
    input.profile,
    suggestion,
  );

  const renderAttempt = async (spec: RigInput) => {
    throwIfAborted(input.signal);
    emit("Saving rig and rendering frames deterministically");
    const saved = await api.saveRig(spec);
    throwIfAborted(input.signal);
    const render = await api.renderRigAnimation({ ...spec, id: saved.id });
    return { saved, render };
  };

  let warning: string | undefined;
  try {
    const { saved, render } = await renderAttempt(rigInput);
    return { render, rig: saved, fitReport, warning: warning ?? fitWarning };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!/imperceptible|static|duplicate|missing_body|root_loop/i.test(message)) throw error;
    emit("First rig pass was too static — retrying with simplified motion");
    const simplifiedMotion = `${input.motion}. Use fewer frames, larger joint rotations (at least 12 degrees), visible torso compression, and a clear looping gait.`;
    suggestion = await suggest(simplifiedMotion);
    rigInput = buildRigInput(
      input.workspaceId,
      input.worktreeId,
      input.assetId,
      input.assetName,
      simplifiedMotion,
      input.profile,
      suggestion,
    );
    try {
      const { saved, render } = await renderAttempt(rigInput);
      warning = "Animation published with simplified motion after the first rig pass was too static.";
      return { render, rig: saved, fitReport, warning: warning ?? fitWarning };
    } catch (retryError) {
      throw retryError;
    }
  }
}

function normalizeAssetPathToken(token: string): string {
  return token.replace(/^["']|["']$/g, "").replace(/[.,;]+$/g, "").replace(/\\/g, "/");
}

/** Extract a workspace-relative master path from an animate prompt or context line. */
export function extractMasterPathFromPrompt(prompt: string): string | undefined {
  const useMaster = prompt.match(/Use\s+(\S+)\s+as the exact source master/i);
  if (useMaster?.[1]) return normalizeAssetPathToken(useMaster[1]);
  const looseUse = prompt.match(/Use\s+(assets\/[^\s,]+)/i);
  if (looseUse?.[1]) return normalizeAssetPathToken(looseUse[1]);
  const contextAsset = prompt.match(/Context asset:\s*(\S+)/i);
  if (contextAsset?.[1]) return normalizeAssetPathToken(contextAsset[1]);
  const bareAsset = prompt.match(/\b(assets\/[^\s,]+\.(?:png|webp|gif|jpg|jpeg))\b/i);
  if (bareAsset?.[1]) return normalizeAssetPathToken(bareAsset[1]);
  return undefined;
}

function findAssetByRelativePath(assets: Asset[], relativePath: string): Asset | undefined {
  return findAssetByManifestPath(assets, relativePath);
}

/** Resolve the animate master from selection, prompt path, or chat context. */
export function resolveAnimateMasterAsset(
  prompt: string,
  assets: Asset[],
  selectedAsset?: Asset,
  contextAssetPath?: string,
): Asset | undefined {
  if (selectedAsset) return selectedAsset;
  const promptPath = extractMasterPathFromPrompt(prompt);
  if (promptPath) {
    const match = findAssetByRelativePath(assets, promptPath);
    if (match) return match;
  }
  if (contextAssetPath) {
    const match = findAssetByRelativePath(assets, contextAssetPath);
    if (match) return match;
  }
  return undefined;
}

/** Resolve the animate master after a master-only provider pass. */
export function resolveMasterAfterProviderMasterPhase(input: {
  manifest: GenerationManifest | null;
  freshManifest: boolean;
  manifestChanged: boolean;
  scanned: Asset[];
  library: Asset[];
  responsePaths: string[];
}): Asset | undefined {
  let master = (input.freshManifest || input.manifestChanged) && input.manifest
    ? resolveMasterFromManifest(input.manifest, input.scanned)
    : undefined;
  if (!master && input.responsePaths.length) {
    master = input.responsePaths
      .map(path => findAssetByManifestPath(input.library, path))
      .find((asset): asset is Asset => Boolean(asset));
  }
  if (!master && input.scanned.length) {
    master = resolveLatestCharacterAsset(input.scanned);
  }
  const hasGenerationSignal = input.freshManifest
    || input.manifestChanged
    || input.scanned.length > 0
    || input.responsePaths.length > 0;
  if (!master && hasGenerationSignal) {
    master = resolveLatestCharacterAsset(input.library);
  }
  return master;
}

/** Pick the master asset from a fresh generation manifest. */
export function resolveMasterFromManifest(manifest: GenerationManifest | null, assets: Asset[]): Asset | undefined {
  if (!manifest) return undefined;
  const orderedPaths = [
    ...(manifest.source ? [manifest.source] : []),
    ...manifest.files,
  ];
  const matches = assetsFromManifestPaths(assets, orderedPaths);
  if (manifest.source) {
    const sourceAsset = findAssetByManifestPath(assets, manifest.source);
    if (sourceAsset) return sourceAsset;
  }
  if (matches.length === 1) return matches[0];
  const singleFrame = matches.find(asset => !/_\d{2,}$/i.test(asset.name.replace(/\.[^.]+$/, "")));
  return singleFrame ?? matches[0];
}

/** Fallback when manifest paths do not resolve: newest character/creature asset. */
export function resolveLatestCharacterAsset(assets: Asset[]): Asset | undefined {
  const candidates = assets.filter(asset => asset.category === "characters" || asset.category === "creatures");
  const pool = candidates.length ? candidates : assets;
  if (!pool.length) return undefined;
  return [...pool].sort((left, right) => right.createdAt.localeCompare(left.createdAt))[0];
}

/** True when chat should use app-orchestrated native rig instead of agent mask-rig. */
export function shouldOrchestrateNativeRig(
  command: string | undefined,
  polishMode: AnimationPolishMode,
  asset?: Asset,
): boolean {
  return command === "animate" && polishMode === "rig" && Boolean(asset);
}

/** True when /animate rig-only was sent without any resolvable master asset. */
export function isAnimateWithoutResolvableMaster(
  command: string | undefined,
  polishMode: AnimationPolishMode,
  prompt: string,
  assets: Asset[],
  selectedAsset?: Asset,
  contextAssetPath?: string,
): boolean {
  const isAnimate = command === "animate" || /^\/animate\b/i.test(prompt.trim());
  if (!isAnimate) return false;
  return !resolveAnimateMasterAsset(prompt, assets, selectedAsset, contextAssetPath);
}

/** Parse polish mode embedded in a motion prompt. */
export function parsePolishModeFromPrompt(prompt: string): AnimationPolishMode {
  if (/Polish mode:\s*AI polish/i.test(prompt)) return "ai-polish";
  if (/Polish mode:\s*Full redraw/i.test(prompt)) return "full-redraw";
  return "rig";
}

/** Prefer the chat-selected animation mode for /animate; fall back to prompt text elsewhere. */
export function resolvePolishMode(
  prompt: string,
  command: string | undefined,
  selected: AnimationPolishMode,
): AnimationPolishMode {
  if (command === "animate" || /^\/animate\b/i.test(prompt.trim())) return selected;
  return parsePolishModeFromPrompt(prompt);
}

/** Extract motion text from `/animate` prompts. */
export function extractAnimateMotion(prompt: string): string {
  const stripped = prompt.replace(/^\/animate\s*/i, "").trim();
  const motionMatch = stripped.match(/Motion:\s*([^.]+(?:\.[^A-Z][^.]*)*)/i);
  if (motionMatch?.[1]) return motionMatch[1].trim();
  const useMaster = stripped.match(/Use\s+[^\s]+\s+as the exact source master\.\s*Motion:\s*(.+?)(?:\.|$)/i);
  if (useMaster?.[1]) return useMaster[1].trim();
  return stripped
    .replace(/Use\s+\S+\s+as the exact source master\.?/i, "")
    .replace(/Polish mode:[^.]+\./gi, "")
    .replace(/Plan a repeatable[^.]+\./i, "")
    .replace(/Preserve the source[^.]+success\.?/i, "")
    .trim() || "natural looping motion";
}

const motionIntentPattern = /\b(?:walk(?:s|ing)?|run(?:s|ning)?|hop(?:s|ping)?|animate|animation|idle|fly(?:ing)?|crawl(?:s|ing)?|loop(?:ing)?|cycle)\b/i;
const newCharacterPattern = /\b(?:create|generate|make|design|draw|build)\b/i;

/** Animated new-character requests without a master yet need a master-only provider pass first. */
export function needsNativeRigMasterPhase(
  prompt: string,
  command: string | undefined,
  asset?: Asset,
  selectedPolishMode?: AnimationPolishMode,
): boolean {
  if (asset) return false;
  const polishMode = selectedPolishMode ?? parsePolishModeFromPrompt(prompt);
  if (polishMode !== "rig" && polishMode !== "ai-polish" && polishMode !== "full-redraw") return false;
  if (command === "animate" || /^\/animate\b/i.test(prompt.trim())) return false;
  if (command && command !== "animate") return false;
  return motionIntentPattern.test(prompt) && newCharacterPattern.test(prompt);
}

/** Blocking quality issues that should surface after rig-only chat completion. */
export function formatBlockingQualityNotice(checks: { severity: string; checkType: string; message: string; ignored: boolean; repairAction?: string }[]): string | undefined {
  const blocking = checks.filter(check =>
    !check.ignored
    && (check.severity === "error" || (check.severity === "warning" && ["duplicate", "leg_separation", "dimensions", "transparency", "boundary"].includes(check.checkType))),
  );
  if (!blocking.length) return;
  const lead = blocking[0];
  const transparency = blocking.find(check => check.checkType === "transparency" || check.repairAction === "inspect_transparency");
  if (transparency) {
    const extra = blocking.length > 1 ? ` (+${blocking.length - 1} more issue${blocking.length > 2 ? "s" : ""})` : "";
    return `Quality check: ${transparency.message} Open Quality and choose Repair transparency.${extra}`;
  }
  const extra = blocking.length > 1 ? ` (+${blocking.length - 1} more)` : "";
  return `Quality check: ${lead.message}${extra}`;
}

export type PolishedFrameValidation = "accepted" | "rejected" | "failed";

function polishValidationErrorCode(error: unknown): string | undefined {
  if (typeof error === "string") {
    try {
      const parsed = JSON.parse(error) as { code?: string };
      if (parsed.code) return parsed.code;
    } catch {
      if (error.includes("polish_rejected")) return "polish_rejected";
    }
    return undefined;
  }
  if (error && typeof error === "object") {
    if ("code" in error && (error as { code?: string }).code) {
      return String((error as { code?: string }).code);
    }
    if ("message" in error) {
      const message = String((error as { message?: string }).message);
      if (message.includes("polish_rejected")) return "polish_rejected";
      try {
        const parsed = JSON.parse(message) as { code?: string };
        if (parsed.code) return parsed.code;
      } catch {
        return undefined;
      }
    }
  }
  return undefined;
}

/** Validate one polished frame through bundled sprite_polish.py. */
export async function validatePolishedFrame(input: {
  workspaceId: string;
  masterPath: string;
  roughPath: string;
  polishedPath: string;
  outputPath: string;
}): Promise<PolishedFrameValidation> {
  try {
    await api.runSpritePolish(input.workspaceId, input.masterPath, input.roughPath, input.polishedPath, input.outputPath);
    return "accepted";
  } catch (error) {
    console.warn(`Polished frame validation failed for ${input.polishedPath}:`, error);
    return polishValidationErrorCode(error) === "polish_rejected" ? "rejected" : "failed";
  }
}
