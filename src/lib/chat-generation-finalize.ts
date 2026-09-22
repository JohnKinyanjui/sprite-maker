import { slashCommand } from "$lib/generation-profiles";
import type { CustomSkill } from "$lib/library-types";
import type {
  Animation,
  AnimationPolishMode,
  Asset,
  AssetPack,
  ChatGenerationProfile,
  GenerationManifest,
  Message,
  PackGenerationMetadata,
  ProviderRequestOptions,
  ReferenceImage,
  SpriteSlashCommand,
  Worktree,
} from "$lib/types";
import { spriteGenerationCard, stripFrameSuffix } from "$lib/generation-reconcile";

export { stripFrameSuffix } from "$lib/generation-reconcile";
import {
  type GenerationActivityEntry,
  type GenerationActivityLevel,
  activityLevelForLine,
} from "$lib/generation-status";
import { normalizeManifestPath } from "$lib/manifest-path";

export type ActiveChatRequest = {
  id: string;
  conversationId: string;
  workspaceId: string;
  worktreeId?: string;
  prompt: string;
  command?: SpriteSlashCommand;
  generation: ProviderRequestOptions["generation"];
  knownPackIds: string[];
  previousGenerationFingerprint?: string;
  startedAt: number;
  polishMode?: AnimationPolishMode;
  phase?: "master" | "rig";
  masterAssetId?: string;
  motion?: string;
  rigId?: string;
  masterPath?: string;
  roughFramePaths?: string[];
  roughFrameBackupPaths?: string[];
};

export type GenerationViewHandoff =
  | { kind: "animation"; animationId: string; rigId?: string }
  | { kind: "sprite"; asset: Asset; analyzeRig: boolean }
  | { kind: "unaccepted"; rejectedStaticAnimation: boolean }
  | { kind: "none" };

/** Provider generation payload copied from the chat profile. */
export function generationRequestFromProfile(profile: ChatGenerationProfile): ProviderRequestOptions["generation"] {
  return {
    quality: profile.quality,
    width: profile.width,
    height: profile.height,
    frames: profile.frames,
    fps: profile.fps,
    frameMode: profile.frameMode,
    minFrames: profile.minFrames,
    maxFrames: profile.maxFrames,
    allowInterpolation: profile.allowInterpolation,
    allowAutoAdjust: profile.allowAutoAdjust,
  };
}

/** Hidden context block sent with a chat generation request. */
export function buildChatContext(input: {
  worktree?: Worktree;
  focused?: ReferenceImage;
  selectedAsset?: Asset;
  styleName: string;
  stylePrompt: string;
  customSkills: CustomSkill[];
}): string {
  const enabledSkillContext = input.customSkills
    .filter(skill => skill.enabled)
    .map(skill => `USER SKILL — ${skill.name}: ${skill.instructions}`)
    .join("\n");
  return [
    input.worktree ? `Active project section: ${input.worktree.name}. ${input.worktree.description ?? ""}` : "",
    input.focused
      ? `FOCUSED CHAT REFERENCE: ${input.focused.path} (content hash ${input.focused.contentHash}). Treat this as the primary visual subject for this turn. The user may change or clear focus later; do not describe it as permanently locked.`
      : "",
    input.selectedAsset ? `Context asset: ${input.selectedAsset.relativePath}` : "",
    `Selected art direction: ${input.styleName}. ${input.stylePrompt}.`,
    enabledSkillContext,
  ].filter(Boolean).join("\n");
}

/** Slash command, or an implied pack request from prose. */
export function inferChatCommand(prompt: string): SpriteSlashCommand | undefined {
  return slashCommand(prompt) ?? (/\b(?:asset\s+|sprite\s+|art\s+)?packs?\b/i.test(prompt) ? "pack" : undefined);
}

/** Status lines shown while a generation is running. */
export function chatActivityLines(
  command: SpriteSlashCommand | undefined,
  prompt: string,
  generation: ProviderRequestOptions["generation"],
): string[] {
  if (command === "pack") {
    return ["AI is planning one coordinated asset pack", "Items will share one style and remain separate static sprites"];
  }
  if (/\b(?:terrain|tileset|tilemap|ground tiles?)\b/i.test(prompt)) {
    return ["AI is planning one complete terrain atlas", "Output will be one PNG containing compatible fills, edges, corners, strips, and transitions"];
  }
  if (generation.frameMode === "auto") {
    return ["AI is inspecting the reference and will recommend a frame count", `Allowed range: ${generation.minFrames}–${generation.maxFrames} frames`];
  }
  return [];
}

/** Title taken from the first user prompt of a new chat. */
export function conversationTitleFromPrompt(prompt: string): string {
  return prompt.trim().replace(/^\/[a-z]+\s*/i, "").replace(/\s+/g, " ").slice(0, 42) || "New conversation";
}

/** Provider request options for the current chat profile. */
export function buildProviderOptions(
  profile: ChatGenerationProfile,
  command: SpriteSlashCommand | undefined,
  referenceIds: string[],
): ProviderRequestOptions {
  return {
    model: profile.model || undefined,
    reasoningEffort: profile.reasoningEffort || undefined,
    command,
    generation: generationRequestFromProfile(profile),
    referenceIds,
    imageProviderId: profile.imageProviderId,
  };
}

/** True when the provider response references the manifest it just wrote. */
export function manifestBelongsToResponse(manifest: GenerationManifest, response: string): boolean {
  const responseText = response.toLowerCase();
  if (responseText.includes(manifest.name.toLowerCase())) return true;
  return manifest.files.some(path => responseText.includes(normalizeManifestPath(path).toLowerCase()));
}

/** True when more than one chat in the same workspace is actively generating. */
export function parallelGenerationsInWorkspace(
  runningRequests: Record<string, ActiveChatRequest>,
  workspaceId: string,
): boolean {
  return Object.values(runningRequests).filter(request => request.workspaceId === workspaceId).length > 1;
}

/** True when the workspace manifest belongs to this request rather than a stale previous run. */
export function isFreshGenerationManifest(
  manifest: GenerationManifest | null,
  manifestFingerprint: string | null,
  previousFingerprint: string | undefined,
  startedAt: number,
  response?: string,
  requireResponseAttribution = false,
): boolean {
  const manifestTime = manifest ? Date.parse(manifest.generatedAt) : Number.NaN;
  const fresh = Boolean(
    manifest
    && manifestFingerprint
    && manifestFingerprint !== previousFingerprint
    && Number.isFinite(manifestTime)
    && manifestTime >= startedAt - 2_000
  );
  if (!fresh || !manifest) return false;
  if (!requireResponseAttribution) return true;
  if (!response?.trim()) return false;
  return manifestBelongsToResponse(manifest, response);
}

/** Merge newly scanned generation assets into the known library. */
export function mergeGeneratedAssets(assets: Asset[], generatedAssets: Asset[]): Asset[] {
  const assetMap = new Map(assets.map(asset => [asset.id, asset]));
  for (const asset of generatedAssets) assetMap.set(asset.id, asset);
  return [...assetMap.values()].sort((a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name));
}

/** Pack created by this request, if the command asked for one. */
export function findGeneratedPack(
  command: SpriteSlashCommand | undefined,
  nextPacks: AssetPack[],
  knownPackIds: string[],
  response: string,
): AssetPack | undefined {
  if (command !== "pack") return;
  const knownPacks = new Set(knownPackIds);
  const createdPacks = nextPacks.filter(pack => !knownPacks.has(pack.id));
  const responseText = response.toLowerCase();
  return createdPacks.find(pack =>
    responseText.includes(pack.name.toLowerCase())
    || responseText.includes(pack.id.toLowerCase())
    || pack.files.some(file => responseText.includes(file.toLowerCase()))
  ) ?? (createdPacks.length === 1 ? createdPacks[0] : undefined);
}

/** Assets whose paths appear in the given file list. */
export function assetsForPaths(assets: Asset[], files: string[]): Asset[] {
  return files
    .map(path => assets.find(asset => asset.relativePath === path))
    .filter((asset): asset is Asset => Boolean(asset));
}

/** `/animate` that only produced a static fallback must not be treated as complete. */
export function isRejectedStaticAnimation(command: SpriteSlashCommand | undefined, manifestAssets: Asset[]): boolean {
  return command === "animate" && manifestAssets.length > 0 && manifestAssets.length < 2;
}

/** FPS from the manifest when it is a positive number. */
export function manifestPlaybackFps(manifest: GenerationManifest | null, fallback: number): number {
  return manifest && Number.isFinite(manifest.fps) && manifest.fps > 0 ? manifest.fps : fallback;
}

/** Component handoff assets: pack files or accepted manifest sprites. */
export function relatedGenerationAssets(
  command: SpriteSlashCommand | undefined,
  packAssets: Asset[],
  acceptedManifestAssets: Asset[],
): Asset[] {
  return command === "pack" ? packAssets : acceptedManifestAssets;
}

/** Preserve manifest order when it was accepted; otherwise sort by path. */
export function orderedGenerationAssets(acceptedManifestAssets: Asset[], related: Asset[]): Asset[] {
  return acceptedManifestAssets.length
    ? [...related]
    : [...related].sort((a, b) => a.relativePath.localeCompare(b.relativePath, undefined, { numeric: true }));
}

/** Multi-frame non-pack generations should become looping animations. */
export function shouldSaveGeneratedAnimation(command: SpriteSlashCommand | undefined, ordered: Asset[]): boolean {
  return command !== "pack" && ordered.length > 1;
}

/** Non-pack generations with at least one sprite attach a chat card. */
export function shouldAttachSpriteCard(command: SpriteSlashCommand | undefined, ordered: Asset[]): boolean {
  return command !== "pack" && ordered.length > 0;
}

/** Chat card metadata for a finished sprite generation. */
export function spriteCardForOrderedAssets(
  ordered: Asset[],
  fps: number,
  animationId?: string,
): ReturnType<typeof spriteGenerationCard> {
  return spriteGenerationCard(ordered, stripFrameSuffix(ordered[0].name), ordered[0].category, ordered.length > 1 ? fps : 1, animationId);
}

/** Chat card metadata for a finished pack generation. */
export function packGenerationCard(packId: string): PackGenerationMetadata {
  return { kind: "pack-generation", packId };
}

/** Merge sprite and/or pack generation cards onto assistant metadata in one write. */
export function mergeAssistantGenerationMetadata(
  existing: Record<string, unknown>,
  input: {
    generation?: ReturnType<typeof spriteCardForOrderedAssets>;
    packGeneration?: PackGenerationMetadata;
  },
): Record<string, unknown> {
  let metadata = { ...existing };
  if (input.generation) metadata = { ...metadata, generation: input.generation };
  if (input.packGeneration) metadata = { ...metadata, packGeneration: input.packGeneration };
  return metadata;
}

/** Which studio tab should open after a completed generation. */
export function generationViewHandoff(input: {
  selectedConversationId?: string;
  requestConversationId: string;
  animationId?: string;
  rigId?: string;
  ordered: Asset[];
  command?: SpriteSlashCommand;
  rejectedStaticAnimation: boolean;
}): GenerationViewHandoff {
  if (input.requestConversationId !== input.selectedConversationId) return { kind: "none" };
  if (input.animationId) return { kind: "animation", animationId: input.animationId, rigId: input.rigId };
  if (input.ordered[0]) return { kind: "sprite", asset: input.ordered[0], analyzeRig: input.ordered.length === 1 };
  if (input.command !== "pack") return { kind: "unaccepted", rejectedStaticAnimation: input.rejectedStaticAnimation };
  return { kind: "none" };
}

/** Error shown when a generation finished without an accepted sprite handoff. */
export function unacceptedGenerationNotice(rejectedStatic: boolean): string {
  return rejectedStatic
    ? "Animation needs at least two fresh frames. The static fallback was kept as an asset but was not accepted as a completed animation."
    : "Generation completed without a fresh valid sprite manifest, so it was not accepted as a component update.";
}

/** Append activity lines for a conversation without mutating the previous record. */
export function appendConversationActivity(
  activityByConversation: Record<string, GenerationActivityEntry[]>,
  conversationId: string,
  lines: string[],
  level?: GenerationActivityLevel,
): Record<string, GenerationActivityEntry[]> {
  const current = activityByConversation[conversationId] ?? [];
  const at = Date.now();
  const entries = lines.map(text => ({
    text,
    level: level ?? activityLevelForLine(text),
    at,
  }));
  return { ...activityByConversation, [conversationId]: [...current, ...entries] };
}

/** Append streamed tokens onto the running assistant message. */
export function appendAssistantDelta(messages: Message[], content: string): Message[] | undefined {
  const index = messages.findLastIndex(message => message.role === "assistant" && message.status === "running");
  if (index < 0) return;
  const next = [...messages];
  next[index] = { ...next[index], content: next[index].content + content };
  return next;
}

/** Provider events that close a running request. */
export function isTerminalProviderEvent(eventType: string): boolean {
  return ["completed", "failed", "cancelled"].includes(eventType);
}

/** Drop a finished request from the running map. */
export function clearRunningRequest(
  runningRequests: Record<string, ActiveChatRequest>,
  conversationId: string,
  requestId: string,
): Record<string, ActiveChatRequest> {
  const next = { ...runningRequests };
  if (next[conversationId]?.id === requestId) delete next[conversationId];
  return next;
}

/** Drop any active request for a conversation (e.g. after native-rig id swap). */
export function clearConversationRunningRequest(
  runningRequests: Record<string, ActiveChatRequest>,
  conversationId: string,
): Record<string, ActiveChatRequest> {
  const next = { ...runningRequests };
  delete next[conversationId];
  return next;
}

const POLISH_MODE_PATTERN = /Polish mode:[^.]+\./gi;

/** Finish instruction sentence embedded in `/animate` prompts. */
export function polishModeInstruction(polishMode: AnimationPolishMode): string {
  if (polishMode === "ai-polish") {
    return "Polish mode: AI polish. Build and validate the deterministic rig first, render all rough frames, then repair only small joint, seam, and outline defects while preserving each rough pose exactly.";
  }
  if (polishMode === "full-redraw") {
    return "Polish mode: Full redraw (experimental). Build and validate the deterministic rig first, then use each rough rig frame as the exact pose, timing, scale, and canvas authority for its redraw.";
  }
  return "Polish mode: Rig only. The app orchestrates native rigging (ai_suggest_rig_points, save_rig, render_rig_animation) without mask-rig Python. Do not use ImageGen for animation frames.";
}

/** Replace or append the polish-mode sentence so typed `/animate` matches the chat dropdown. */
export function applyAnimationPolishModeToPrompt(prompt: string, polishMode: AnimationPolishMode): string {
  const instruction = polishModeInstruction(polishMode);
  if (/Polish mode:[^.]+\./i.test(prompt)) {
    return prompt.replace(POLISH_MODE_PATTERN, instruction);
  }
  const trimmed = prompt.trimEnd();
  return trimmed.endsWith(".") ? `${trimmed} ${instruction}` : `${trimmed}. ${instruction}`;
}

/** `/animate` prompt that uses a source sprite as the exact motion master. */
export function buildMotionPrompt(
  asset: Pick<Asset, "relativePath">,
  motion: string,
  polishMode: AnimationPolishMode,
  profile: ChatGenerationProfile,
): string {
  const frameBudget = profile.frameMode === "fixed"
    ? `${profile.frames} frames`
    : `between ${profile.minFrames} and ${profile.maxFrames} frames, choosing the smallest mechanically complete count`;
  const finishInstruction = polishModeInstruction(polishMode);
  return `/animate Use ${asset.relativePath} as the exact source master. Motion: ${motion}. Plan a repeatable ${frameBudget} loop at ${profile.fps} FPS. ${finishInstruction} Preserve the source anatomy, markings, palette, proportions, facing direction, pivot, and ground line. Keep near/far limb identity and layer order stable through crossings, preview at least three cycles, save the rig and playback manifest, and run native quality analysis before reporting success.`;
}

/** Chat prompt for an experimental full redraw after deterministic rig frames exist. */
export function buildFullRedrawPrompt(animation: Animation, frameAssets: Asset[], motion?: string): string {
  const paths = frameAssets.map(asset => asset.relativePath).join(", ");
  const motionHint = motion?.trim() || animation.name;
  return `Redraw every frame of the rig-rendered animation "${animation.name}" (${motionHint}) with experimental full AI redraw. Canonical rig frames: ${paths}. Generate one new high-quality transparent frame per rig frame in playback order, using each rig frame only as pose and timing reference. Keep timing, canvas placement, limb layering, and proportions stable. Write output under assets/${frameAssets[0].category}/, update the generation manifest, and run quality analysis before reporting success.`;
}

/** Chat prompt that polishes pose-canonical rig frames without swapping limb identity. */
export function buildRigPolishPrompt(animation: Animation, frameAssets: Asset[]): string {
  const paths = frameAssets.map(asset => asset.relativePath);
  return `Enhance the rig-rendered animation "${animation.name}" with AI polish. These frames were rendered deterministically from a joint rig, so their poses, limb layering, and contact timing are canonical: ${paths.join(", ")}. For each rig frame, generate one high-quality transparent AI frame using the corresponding rig frame as the exact pose reference. Keep the identical pose, timing, and canvas placement; keep the NEAR limb always occluding the FAR limb and the FAR limb about 20–30% darker — never swap leg identity or shading roles. Fix only rendering detail and quality, never retime, reorder, or reinterpret the poses. Write the polished frames in playback order under assets/${frameAssets[0].category}/, update the generation manifest, and run quality analysis before reporting success.`;
}

/** Frame assets for an animation, skipping missing library entries. */
export function animationFrameAssets(animation: Animation, assets: Asset[]): Asset[] {
  return animation.frames
    .map(frame => assets.find(asset => asset.id === frame.assetId))
    .filter((asset): asset is Asset => Boolean(asset));
}
