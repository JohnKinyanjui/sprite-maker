import { api } from "$lib/api";
import {
  type ActiveChatRequest, type GenerationViewHandoff, animationFrameAssets, buildFullRedrawPrompt,
  buildProviderOptions, buildRigPolishPrompt, conversationTitleFromPrompt, findGeneratedPack,
  generationViewHandoff, isFreshGenerationManifest, isRejectedStaticAnimation, manifestPlaybackFps,
  mergeAssistantGenerationMetadata, mergeGeneratedAssets, orderedGenerationAssets, packGenerationCard,
  relatedGenerationAssets, shouldAttachSpriteCard, shouldSaveGeneratedAnimation, spriteCardForOrderedAssets,
  stripFrameSuffix,
} from "$lib/chat-generation-finalize";
import { assetsFromManifestPaths, findAnimationWithOrderedFrames, latestCompletedAssistant } from "$lib/generation-reconcile";
import { reportsGenerationFailure } from "$lib/message-generations";
import {
  extractAssetPathsFromResponse, normalizeManifestPath, shouldRecoverAssetsFromResponse,
} from "$lib/manifest-path";
import {
  extractAnimateMotion, formatBlockingQualityNotice, orchestrateRigOnlyAnimation,
  resolveLatestCharacterAsset, resolveMasterFromManifest, validatePolishedFrame,
} from "$lib/rig-animation-orchestrator";
import type { Animation, AnimationPolishMode, Asset, AssetPack, Conversation, GenerationManifest, Message, ProviderRequestOptions, Rig, Worktree } from "$lib/types";

export type StartedChatRequest = {
  request: ActiveChatRequest;
  messages: Message[];
  renamed?: Conversation;
};

/** Start a provider request and optionally title a new conversation. */
export async function startChatRequest(input: {
  conversation: Conversation;
  workspaceId: string;
  worktree?: Worktree;
  prompt: string;
  context: string;
  options: ProviderRequestOptions;
  knownPackIds: string[];
  polishMode?: ActiveChatRequest["polishMode"];
  phase?: ActiveChatRequest["phase"];
  masterAssetId?: string;
  motion?: string;
}): Promise<StartedChatRequest> {
  const previousGenerationFingerprint = await api.getGenerationFingerprint(input.workspaceId).catch(() => null);
  const startedAt = Date.now();
  const requestId = await api.startProviderMessage(input.conversation.id, input.prompt, input.context, input.options);
  const request: ActiveChatRequest = {
    id: requestId,
    conversationId: input.conversation.id,
    workspaceId: input.workspaceId,
    worktreeId: input.worktree?.id,
    prompt: input.prompt,
    command: input.options.command,
    generation: input.options.generation,
    knownPackIds: input.knownPackIds,
    previousGenerationFingerprint: previousGenerationFingerprint ?? undefined,
    startedAt,
    polishMode: input.polishMode,
    phase: input.phase,
    masterAssetId: input.masterAssetId,
    motion: input.motion,
  };
  const messages = await api.listMessages(input.conversation.id);
  if (input.conversation.title !== "New conversation") return { request, messages };
  const title = conversationTitleFromPrompt(input.prompt);
  await api.renameConversation(input.conversation.id, title);
  return { request, messages, renamed: { ...input.conversation, title } };
}

export { normalizeManifestPath } from "$lib/manifest-path";

/** Match a polished manifest path back to its archived rough frame index. */
export function roughIndexForPolishedPath(roughFramePaths: string[], polished: string): number {
  const normalized = normalizeManifestPath(polished);
  return roughFramePaths.findIndex(rough => normalizeManifestPath(rough) === normalized);
}

/** Attach AI polish validation metadata to an active chat request. */
export function attachAiPolishHandoffMetadata(
  request: ActiveChatRequest,
  input: { masterPath?: string; frameAssets: Asset[]; roughBackups: string[] },
): ActiveChatRequest {
  const roughPaths = input.frameAssets.map(asset => asset.relativePath);
  return {
    ...request,
    polishMode: "ai-polish",
    masterPath: input.masterPath ?? input.frameAssets[0]?.relativePath,
    roughFramePaths: roughPaths,
    roughFrameBackupPaths: input.roughBackups,
  };
}

/** Archive rough rig frames and start the provider polish pass with validation metadata. */
export async function startAiPolishHandoff(input: {
  conversation: Conversation;
  workspaceId: string;
  worktree?: Worktree;
  animation: Animation;
  frameAssets: Asset[];
  masterPath?: string;
  profile: import("$lib/types").ChatGenerationProfile;
  referenceIds: string[];
  knownPackIds: string[];
  motion?: string;
  context: string;
}): Promise<StartedChatRequest> {
  const roughPaths = input.frameAssets.map(asset => asset.relativePath);
  const roughBackups = await api.archiveSpritePaths(input.workspaceId, roughPaths, input.animation.id.slice(0, 8));
  const polishPrompt = buildRigPolishPrompt(input.animation, input.frameAssets);
  const options = buildProviderOptions(input.profile, "animate", input.referenceIds);
  const started = await startChatRequest({
    conversation: input.conversation,
    workspaceId: input.workspaceId,
    worktree: input.worktree,
    prompt: polishPrompt,
    context: input.context,
    options,
    knownPackIds: input.knownPackIds,
    polishMode: "ai-polish",
    motion: input.motion,
  });
  started.request = attachAiPolishHandoffMetadata(started.request, {
    masterPath: input.masterPath,
    frameAssets: input.frameAssets,
    roughBackups,
  });
  return started;
}

export type NativeRigChatInput = {
  conversation: Conversation;
  workspaceId: string;
  worktree?: Worktree;
  prompt: string;
  asset: Asset;
  motion: string;
  profile: import("$lib/types").ChatGenerationProfile;
  providerId: string;
  model?: string;
  reasoningEffort?: string;
  knownPackIds: string[];
  currentAssets?: Asset[];
  /** When true, append only an assistant message (native rig phase after a master provider pass). */
  assistantOnly?: boolean;
  polishMode?: AnimationPolishMode;
  signal?: AbortSignal;
  onFitWarning?: (message: string) => void;
  onActivity?: (line: string) => void;
};

/** App-orchestrated rig-only animation without delegating mask-rig work to the agent CLI. */
export async function runNativeRigChatAnimation(input: NativeRigChatInput): Promise<{
  request: ActiveChatRequest;
  messages: Message[];
  completion: ChatGenerationCompletion;
  renamed?: Conversation;
}> {
  const startedAt = Date.now();
  const request: ActiveChatRequest = {
    id: `native-rig-${startedAt}`,
    conversationId: input.conversation.id,
    workspaceId: input.workspaceId,
    worktreeId: input.worktree?.id,
    prompt: input.prompt,
    command: "animate",
    generation: {
      quality: input.profile.quality,
      width: input.profile.width,
      height: input.profile.height,
      frames: input.profile.frames,
      fps: input.profile.fps,
      frameMode: input.profile.frameMode,
      minFrames: input.profile.minFrames,
      maxFrames: input.profile.maxFrames,
      allowInterpolation: input.profile.allowInterpolation,
      allowAutoAdjust: input.profile.allowAutoAdjust,
    },
    knownPackIds: input.knownPackIds,
    startedAt,
    polishMode: input.polishMode ?? "rig",
    motion: input.motion,
  };
  const orchestrated = await orchestrateRigOnlyAnimation({
    workspaceId: input.workspaceId,
    worktreeId: input.worktree?.id,
    assetId: input.asset.id,
    assetName: input.asset.name,
    motion: input.motion,
    profile: input.profile,
    providerId: input.providerId,
    model: input.model,
    reasoningEffort: input.reasoningEffort,
    signal: input.signal,
    onFitWarning: input.onFitWarning,
  }, input.onActivity);
  request.rigId = orchestrated.rig.id;
  const notices = [
    orchestrated.warning,
    orchestrated.fitReport?.warnings[0],
  ].filter(Boolean);
  const assistantText = notices.length
    ? `Rendered ${orchestrated.render.animation.frames.length} native rig frames for ${input.asset.name}. GENERATION_WARNING: ${notices.join(" ")}`
    : `Rendered ${orchestrated.render.animation.frames.length} native rig frames for ${input.asset.name}. Open the Rig tab to adjust points and poses.`;
  let messages = input.assistantOnly
    ? await api.recordChatAssistant(input.conversation.id, assistantText)
    : await api.recordChatTurn(input.conversation.id, input.prompt, assistantText);
  const library = await api.listAssets(input.workspaceId);
  const ordered = orchestrated.render.assetIds
    .map(id => library.find(asset => asset.id === id))
    .filter((asset): asset is Asset => Boolean(asset));
  const assets = mergeGeneratedAssets(input.currentAssets ?? [], ordered);
  const rigs = await api.listRigs(input.workspaceId, input.worktree?.id);
  const animations = await api.listAnimations(input.workspaceId, input.worktree?.id);
  if (input.worktree?.id && ordered.length) {
    await Promise.all(ordered.map(asset => api.linkAssetToWorktree(input.worktree!.id, asset.id)));
  }
  const assistant = messages.findLast(message => message.role === "assistant");
  if (assistant) {
    await api.updateMessageMetadata(assistant.id, {
      ...assistant.metadata,
      generation: spriteCardForOrderedAssets(ordered, orchestrated.render.animation.fps, orchestrated.render.animation.id),
    });
  }
  void api.queueQualityAnalysis(orchestrated.render.animation.id).catch(() => undefined);
  let renamed: Conversation | undefined;
  if (input.conversation.title === "New conversation") {
    const title = conversationTitleFromPrompt(input.prompt);
    await api.renameConversation(input.conversation.id, title);
    renamed = { ...input.conversation, title };
  }
  const completion: ChatGenerationCompletion = {
    assets,
    packs: await api.listAssetPacks(input.workspaceId).catch(() => []),
    rigs,
    worktreeAssetIds: input.worktree?.id ? await api.listWorktreeAssetIds(input.worktree.id) : undefined,
    animations,
    ordered,
    handoff: {
      kind: "animation",
      animationId: orchestrated.render.animation.id,
      rigId: orchestrated.rig.id,
    },
  };
  return { request, messages, completion, renamed };
}

/** After a master-only provider pass, continue with native rig orchestration on the new master. */
export async function continueNativeRigAfterMaster(
  prior: ActiveChatRequest,
  conversation: Conversation,
  providerId: string,
  worktreeId?: string,
  model?: string,
  reasoningEffort?: string,
  onActivity?: (line: string) => void,
  currentAssets?: Asset[],
  signal?: AbortSignal,
  onFitWarning?: (message: string) => void,
  masterResponse?: string,
  parallelGenerationActive = false,
): Promise<{ completion: ChatGenerationCompletion; messages: Message[] } | undefined> {
  const manifest = await api.getGenerationManifest(prior.workspaceId).catch(() => null);
  const fingerprint = manifest ? await api.getGenerationFingerprint(prior.workspaceId).catch(() => null) : null;
  if (!isFreshGenerationManifest(
    manifest,
    fingerprint,
    prior.previousGenerationFingerprint,
    prior.startedAt,
    masterResponse,
    parallelGenerationActive,
  )) return;
  const scanned = await api.scanGenerationAssets(prior.workspaceId);
  const master = resolveMasterFromManifest(manifest, scanned)
    ?? resolveLatestCharacterAsset(scanned);
  if (!master) return;
  const motion = prior.motion ?? extractAnimateMotion(prior.prompt);
  const profile = {
    profileVersion: 8,
    quality: prior.generation.quality,
    width: prior.generation.width,
    height: prior.generation.height,
    frames: prior.generation.frames,
    fps: prior.generation.fps,
    frameMode: prior.generation.frameMode,
    minFrames: prior.generation.minFrames,
    maxFrames: prior.generation.maxFrames,
    allowInterpolation: prior.generation.allowInterpolation,
    allowAutoAdjust: prior.generation.allowAutoAdjust,
    model: model ?? "",
    reasoningEffort: reasoningEffort ?? "",
    imageProviderId: "",
  };
  const result = await runNativeRigChatAnimation({
    conversation: { ...conversation, id: prior.conversationId },
    workspaceId: prior.workspaceId,
    worktree: worktreeId ? { id: worktreeId } as Worktree : undefined,
    prompt: prior.prompt,
    asset: master,
    motion,
    profile,
    providerId,
    model,
    reasoningEffort,
    knownPackIds: prior.knownPackIds,
    currentAssets,
    assistantOnly: true,
    polishMode: prior.polishMode ?? "rig",
    signal,
    onFitWarning,
    onActivity,
  });
  return { completion: result.completion, messages: result.messages };
}

export type MasterPhaseContinuation =
  | { kind: "rig-complete"; completion: ChatGenerationCompletion; messages: Message[] }
  | { kind: "handoff-started"; request: ActiveChatRequest; messages: Message[]; renamed?: Conversation }
  | { kind: "finalize-fallback" };

/** Continue native rig and optional polish handoff after a master-only provider pass. */
export async function continueAfterMasterProviderPhase(input: {
  prior: ActiveChatRequest;
  conversation: Conversation;
  providerId: string;
  worktree?: Worktree;
  profile: import("$lib/types").ChatGenerationProfile;
  referenceIds: string[];
  context: string;
  knownPackIds: string[];
  currentAssets?: Asset[];
  onActivity?: (line: string) => void;
  onFitWarning?: (message: string) => void;
  signal?: AbortSignal;
  masterResponse?: string;
  parallelGenerationActive?: boolean;
}): Promise<MasterPhaseContinuation> {
  const continued = await continueNativeRigAfterMaster(
    input.prior,
    input.conversation,
    input.providerId,
    input.worktree?.id,
    input.profile.model || undefined,
    input.profile.reasoningEffort || undefined,
    input.onActivity,
    input.currentAssets,
    input.signal,
    input.onFitWarning,
    input.masterResponse,
    input.parallelGenerationActive ?? false,
  );
  if (!continued) return { kind: "finalize-fallback" };

  const polishMode = input.prior.polishMode ?? "rig";
  const handoff = continued.completion.handoff;
  if (polishMode === "rig" || handoff.kind !== "animation") {
    return { kind: "rig-complete", completion: continued.completion, messages: continued.messages };
  }

  const generatedAnimation = continued.completion.animations?.find(animation => animation.id === handoff.animationId);
  const frameAssets = generatedAnimation
    ? animationFrameAssets(generatedAnimation, continued.completion.assets)
    : continued.completion.ordered;
  if (!generatedAnimation || !frameAssets.length) {
    return { kind: "rig-complete", completion: continued.completion, messages: continued.messages };
  }

  if (polishMode === "ai-polish") {
    const started = await startAiPolishHandoff({
      conversation: input.conversation,
      workspaceId: input.prior.workspaceId,
      worktree: input.worktree,
      animation: generatedAnimation,
      frameAssets,
      masterPath: continued.completion.ordered[0]?.relativePath,
      profile: input.profile,
      referenceIds: input.referenceIds,
      knownPackIds: input.knownPackIds,
      motion: input.prior.motion,
      context: input.context,
    });
    return { kind: "handoff-started", request: started.request, messages: started.messages, renamed: started.renamed };
  }

  const polishPrompt = buildFullRedrawPrompt(generatedAnimation, frameAssets, input.prior.motion);
  const options = buildProviderOptions(input.profile, "animate", input.referenceIds);
  const started = await startChatRequest({
    conversation: input.conversation,
    workspaceId: input.prior.workspaceId,
    worktree: input.worktree,
    prompt: polishPrompt,
    context: input.context,
    options,
    knownPackIds: input.knownPackIds,
    polishMode: "full-redraw",
    motion: input.prior.motion,
  });
  return { kind: "handoff-started", request: started.request, messages: started.messages, renamed: started.renamed };
}

export type ChatGenerationCompletion = {
  assets: Asset[];
  packs: AssetPack[];
  rigs: Rig[];
  worktreeAssetIds?: string[];
  animations?: Animation[];
  handoff: GenerationViewHandoff;
  ordered: Asset[];
  polishWarning?: string;
};

/** Reject invalid AI polish frames and restore archived rough frames when sprite_polish fails. */
export async function reconcileAiPolishedFrames(
  request: ActiveChatRequest,
  manifest: GenerationManifest | null,
): Promise<string | undefined> {
  if (request.polishMode !== "ai-polish" || !manifest || !request.roughFramePaths?.length) return;
  const master = request.masterPath ?? manifest.source;
  if (!master) return "AI polish finished without a source master path for validation.";
  const backups = request.roughFrameBackupPaths ?? [];
  const rejected: string[] = [];
  const unmatched: string[] = [];
  for (const polished of manifest.files) {
    const roughIndex = roughIndexForPolishedPath(request.roughFramePaths, polished);
    const rough = roughIndex >= 0 ? request.roughFramePaths[roughIndex] : undefined;
    const backup = roughIndex >= 0 ? backups[roughIndex] : undefined;
    if (!polished) continue;
    if (!rough) {
      unmatched.push(polished);
      continue;
    }
    const validation = await validatePolishedFrame({
      workspaceId: request.workspaceId,
      masterPath: master,
      roughPath: rough,
      polishedPath: polished,
      outputPath: polished,
    });
    if (validation === "accepted") continue;
    if (validation === "rejected") {
      if (backup) {
        await api.restoreSpritePaths(request.workspaceId, [backup], [polished]);
      }
      rejected.push(polished);
      continue;
    }
    console.warn(`Skipped polish validation restore for ${polished} because validation tooling failed.`);
  }
  const parts: string[] = [];
  if (rejected.length) {
    parts.push(`AI polish rejected ${rejected.length} frame(s); kept the deterministic rough render instead.`);
  }
  if (unmatched.length) {
    console.warn(`AI polish manifest paths did not match rough frames: ${unmatched.join(", ")}`);
    parts.push(`${unmatched.length} polished path(s) did not match any rough frame.`);
  }
  if (!parts.length) return;
  return parts.join(" ");
}

/** Apply the renderer manifest, save animations/cards, and decide which studio tab to open. */
export async function completeChatGeneration(
  request: ActiveChatRequest,
  response: string,
  current: {
    assets: Asset[];
    packs: AssetPack[];
    rigs: Rig[];
    selectedWorktreeId?: string;
    selectedConversationId?: string;
    parallelGenerationActive?: boolean;
  },
): Promise<ChatGenerationCompletion> {
  const manifest = await api.getGenerationManifest(request.workspaceId).catch(() => null);
  const manifestFingerprint = manifest ? await api.getGenerationFingerprint(request.workspaceId).catch(() => null) : null;
  const freshManifest = isFreshGenerationManifest(
    manifest,
    manifestFingerprint,
    request.previousGenerationFingerprint,
    request.startedAt,
    response,
    current.parallelGenerationActive ?? false,
  );
  let polishWarning: string | undefined;
  if (freshManifest) {
    try {
      polishWarning = await reconcileAiPolishedFrames(request, manifest);
    } catch (error) {
      console.warn("AI polish reconciliation failed:", error);
      polishWarning = "AI polish finished but frame validation could not complete — review the animation frames.";
    }
  }
  const generatedAssets = freshManifest ? await api.scanGenerationAssets(request.workspaceId) : [];
  let nextAssets = mergeGeneratedAssets(current.assets, generatedAssets);
  const nextPacks = await api.listAssetPacks(request.workspaceId).catch(() => current.packs);
  const generatedPack = findGeneratedPack(request.command, nextPacks, request.knownPackIds, response);
  let manifestAssets = freshManifest && manifest ? assetsFromManifestPaths(nextAssets, manifest.files) : [];
  const responseAssetPaths = extractAssetPathsFromResponse(response);
  const generationFailed = reportsGenerationFailure(response);
  if (shouldRecoverAssetsFromResponse(manifestAssets.length, response, generationFailed)) {
    const scanned = await api.scanAssets(request.workspaceId);
    nextAssets = mergeGeneratedAssets(nextAssets, scanned);
    manifestAssets = assetsFromManifestPaths(nextAssets, responseAssetPaths);
  }
  const rejectedStatic = isRejectedStaticAnimation(request.command, manifestAssets);
  const acceptedManifestAssets = rejectedStatic ? [] : manifestAssets;
  const manifestFps = manifestPlaybackFps(manifest, request.generation.fps);
  const packAssets = generatedPack ? assetsFromManifestPaths(nextAssets, generatedPack.files) : [];
  const related = relatedGenerationAssets(request.command, packAssets, acceptedManifestAssets);
  if (request.worktreeId && related.length) {
    await Promise.all(related.map(asset => api.linkAssetToWorktree(request.worktreeId!, asset.id)));
  }
  let animationId: string | undefined;
  const ordered = orderedGenerationAssets(acceptedManifestAssets, related);
  if (shouldSaveGeneratedAnimation(request.command, ordered)) {
    const existing = await api.listAnimations(request.workspaceId, request.worktreeId);
    const match = findAnimationWithOrderedFrames(existing, ordered);
    if (match) animationId = match.id;
    else {
      const actualGeneration = { ...request.generation, frameMode: "fixed" as const, frames: ordered.length, minFrames: ordered.length, maxFrames: ordered.length, fps: manifestFps };
      const motionPlan = await api.planMotion(request.prompt, actualGeneration).catch(() => undefined);
      const createdAnimation = await api.saveAnimation({
        workspaceId: request.workspaceId, worktreeId: request.worktreeId, name: stripFrameSuffix(ordered[0].name),
        fps: manifestFps, looping: true, frames: ordered.map(asset => ({ assetId: asset.id })), motionPlan,
      });
      animationId = createdAnimation.id;
      void api.queueQualityAnalysis(createdAnimation.id).catch(() => undefined);
    }
  }
  const attachSpriteCard = shouldAttachSpriteCard(request.command, ordered);
  if (attachSpriteCard || generatedPack) {
    const requestMessages = await api.listMessages(request.conversationId);
    const assistant = latestCompletedAssistant(requestMessages);
    if (assistant) {
      await api.updateMessageMetadata(assistant.id, mergeAssistantGenerationMetadata(assistant.metadata, {
        generation: attachSpriteCard
          ? spriteCardForOrderedAssets(ordered, manifestFps, animationId)
          : undefined,
        packGeneration: generatedPack ? packGenerationCard(generatedPack.id) : undefined,
      }));
    }
  }
  const rigs = await api.listRigs(request.workspaceId, request.worktreeId).catch(() => current.rigs);
  const worktreeMatches = current.selectedWorktreeId === request.worktreeId;
  const worktreeAssetIds = worktreeMatches
    ? (request.worktreeId ? await api.listWorktreeAssetIds(request.worktreeId) : [])
    : undefined;
  const animations = worktreeMatches ? await api.listAnimations(request.workspaceId, request.worktreeId) : undefined;
  const rigId = manifest?.rigId ?? request.rigId;
  const handoff = worktreeMatches
    ? generationViewHandoff({
      selectedConversationId: current.selectedConversationId,
      requestConversationId: request.conversationId,
      animationId,
      rigId,
      ordered,
      command: request.command,
      rejectedStaticAnimation: rejectedStatic,
    })
    : { kind: "none" as const };
  return { assets: nextAssets, packs: nextPacks, rigs, worktreeAssetIds, animations, handoff, ordered, polishWarning };
}

/** Poll quality report after animation save and return a user-facing notice when needed. */
export async function qualityNoticeForAnimation(animationId: string): Promise<string | undefined> {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await new Promise(resolve => window.setTimeout(resolve, 750));
    const report = await api.getQualityReport(animationId).catch(() => null);
    if (report?.status === "completed") return formatBlockingQualityNotice(report.checks);
    if (report?.status === "failed") return "Quality analysis failed — open Quality to inspect the animation.";
  }
  return "Quality analysis is still running — open Quality shortly to review blocking issues.";
}
