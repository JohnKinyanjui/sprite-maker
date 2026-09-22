import { api } from "$lib/api";
import { findAssetByManifestPath, normalizeManifestPath } from "$lib/manifest-path";
import type { Animation, Asset, GenerationManifest, Message, MotionPlan, SpriteGenerationMetadata } from "$lib/types";

/** Assets that appeared in a scan but were not already known to the shell. */
export function createdAssetsFromScan(knownIds: Set<string>, nextAssets: Asset[]): Asset[] {
  return nextAssets.filter(asset => !knownIds.has(asset.id));
}

/** Prefer the renderer manifest order; otherwise keep the created scan order. */
export function orderCreatedAssets(created: Asset[], manifest: GenerationManifest | null): Asset[] {
  return manifest?.files
    .map(path => findAssetByManifestPath(created, path))
    .filter((asset): asset is Asset => Boolean(asset)) ?? created;
}

/** Resolve workspace assets that match every manifest path, or nothing if any path is missing. */
export function completeManifestAssets(assets: Asset[], files: string[]): Asset[] | undefined {
  const ordered = files.map(path => findAssetByManifestPath(assets, path));
  if (ordered.some(asset => !asset)) return;
  return ordered as Asset[];
}

/** Assets whose relative paths appear in the manifest, skipping gaps. */
export function assetsFromManifestPaths(assets: Asset[], files: string[]): Asset[] {
  return files
    .map(path => findAssetByManifestPath(assets, path))
    .filter((asset): asset is Asset => Boolean(asset));
}

/** Animation whose frames are exactly the given assets in order. */
export function findAnimationWithOrderedFrames(animations: Animation[], cardAssets: Asset[]): Animation | undefined {
  return animations.find(animation =>
    animation.frames.length === cardAssets.length
    && cardAssets.every((asset, index) => animation.frames[index]?.assetId === asset.id)
  );
}

/** Animation that contains every given asset somewhere in its frames. */
export function findAnimationCoveringAssets(animations: Animation[], cardAssets: Asset[]): Animation | undefined {
  return animations.find(item => cardAssets.every(asset => item.frames.some(frame => frame.assetId === asset.id)));
}

/** Strip a trailing `_01` / `-02` frame suffix from a generated sprite name. */
export function stripFrameSuffix(name: string): string {
  return name.replace(/[_-]?\d+$/i, "");
}

/** Chat card payload for a completed sprite or animation generation. */
export function spriteGenerationCard(
  cardAssets: Asset[],
  name: string,
  category: string,
  fps: number,
  animationId?: string,
): SpriteGenerationMetadata {
  return { kind: "sprite-generation", name, category, fps, assetIds: cardAssets.map(asset => asset.id), animationId };
}

/** Latest completed assistant turn in a conversation. */
export function latestCompletedAssistant(messages: Message[]): Message | undefined {
  return messages.findLast(message => message.role === "assistant" && message.status === "completed");
}

/** Multi-frame sprite manifests should become looping animations. */
export function shouldCreateAnimationFromManifest(manifest: GenerationManifest | null, ordered: Asset[]): boolean {
  return manifest?.kind !== "pack" && ordered.length > 1;
}

/** Multi-frame sprite manifests are eligible for animation reconciliation. */
export function shouldReconcileManifest(manifest: GenerationManifest | null): boolean {
  return Boolean(manifest && manifest.kind !== "pack" && manifest.files.length >= 2);
}

/** Attach a generation card when the latest assistant turn mentions this manifest and has none yet. */
export function shouldHydrateGeneration(
  assistant: Message | undefined,
  manifest: GenerationManifest | null,
  cardAssets: Asset[],
): boolean {
  if (!assistant || assistant.metadata.generation) return false;
  if (!manifest || manifest.kind === "pack" || !cardAssets.length) return false;
  const content = assistant.content.toLowerCase();
  return manifest.files.some(path =>
    content.includes(normalizeManifestPath(path).toLowerCase())
  ) || content.includes(manifest.name.toLowerCase());
}

/** Display name for a newly saved animation created from a scan. */
export function animationNameFromScan(manifest: GenerationManifest | null, ordered: Asset[]): string {
  return manifest?.name ?? ordered[0].name.replace(/[_-]?\d+$/, "");
}

/** Playback rate for a newly saved animation created from a scan. */
export function animationFpsFromScan(manifest: GenerationManifest | null, fallback: number): number {
  return manifest?.fps ?? fallback;
}

/** Category stored on the chat generation card. */
export function generationCardCategory(manifest: GenerationManifest | null, ordered: Asset[]): string {
  return manifest?.category ?? ordered[0].category;
}

/** Write a sprite-generation card onto the latest completed assistant turn. */
export async function persistGenerationCard(
  conversationId: string | undefined,
  messages: Message[],
  cardAssets: Asset[],
  name: string,
  category: string,
  fps: number,
  animationId?: string,
): Promise<Message[] | undefined> {
  if (!conversationId) return;
  const assistant = latestCompletedAssistant(messages);
  if (!assistant) return;
  await api.updateMessageMetadata(assistant.id, { ...assistant.metadata, generation: spriteGenerationCard(cardAssets, name, category, fps, animationId) });
  return api.listMessages(conversationId);
}

export type ReconcileResult = {
  worktreeAssetIds?: string[];
  selectedAnimation?: Animation;
  animations?: Animation[];
};

/** Link manifest frames to the worktree and create a looping animation when none exists. */
export async function reconcileManifestData(
  workspaceId: string,
  assets: Asset[],
  animations: Animation[],
  selectedWorktreeId: string | undefined,
  worktreeQueryId: string | undefined,
  motionPlan: MotionPlan | undefined,
): Promise<ReconcileResult | undefined> {
  const manifest = await api.getGenerationManifest(workspaceId).catch(() => null);
  if (!shouldReconcileManifest(manifest) || !manifest) return;
  const cardAssets = completeManifestAssets(assets, manifest.files);
  if (!cardAssets) return;
  let worktreeAssetIds: string[] | undefined;
  if (selectedWorktreeId) {
    await Promise.all(cardAssets.map(asset => api.linkAssetToWorktree(selectedWorktreeId, asset.id)));
    worktreeAssetIds = await api.listWorktreeAssetIds(selectedWorktreeId);
  }
  const existing = findAnimationWithOrderedFrames(animations, cardAssets);
  if (existing) return { worktreeAssetIds, selectedAnimation: existing };
  const selectedAnimation = await api.saveAnimation({
    workspaceId,
    worktreeId: worktreeQueryId,
    name: manifest.name,
    fps: manifest.fps,
    looping: true,
    frames: cardAssets.map(asset => ({ assetId: asset.id })),
    motionPlan,
  });
  return { worktreeAssetIds, selectedAnimation, animations: await api.listAnimations(workspaceId, worktreeQueryId) };
}

/** Restore a missing generation card when the latest assistant turn mentions the manifest. */
export async function hydrateGenerationData(
  workspaceId: string,
  conversationId: string,
  messages: Message[],
  assets: Asset[],
  animations: Animation[],
): Promise<Message[] | undefined> {
  const assistant = latestCompletedAssistant(messages);
  const manifest = await api.getGenerationManifest(workspaceId).catch(() => null);
  const cardAssets = manifest ? assetsFromManifestPaths(assets, manifest.files) : [];
  if (!shouldHydrateGeneration(assistant, manifest, cardAssets) || !assistant || !manifest) return;
  return persistGenerationCard(
    conversationId,
    messages,
    cardAssets,
    manifest.name,
    manifest.category,
    manifest.fps,
    findAnimationWithOrderedFrames(animations, cardAssets)?.id,
  );
}
