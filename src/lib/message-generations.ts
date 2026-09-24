import { spriteGenerationCard, stripFrameSuffix } from "$lib/generation-reconcile";
import { extractAssetPathsFromResponse, findAssetByManifestPath } from "$lib/manifest-path";
import type { Animation, Asset, AssetPack, GenerationOutcomeMetadata, Message, PackGenerationMetadata, SpriteGenerationMetadata } from "$lib/types";

/** Explicit request outcome stored on an assistant message by the finalizer. */
export function generationOutcomeOf(message: Message): GenerationOutcomeMetadata | undefined {
  const stored = message.metadata.generationOutcome;
  if (!stored || typeof stored !== "object" || !("kind" in stored) || stored.kind !== "generation-outcome") return;
  return stored as GenerationOutcomeMetadata;
}

/** Tolerance between an assistant turn starting and the library indexing its output. */
const REQUEST_CLOCK_SLACK_MS = 5_000;

/** Library items created before the request began cannot be that request's output. */
function createdDuringOrAfter(message: Message, item: { createdAt: string }): boolean {
  const started = Date.parse(message.createdAt);
  const created = Date.parse(item.createdAt);
  if (!Number.isFinite(started) || !Number.isFinite(created)) return true;
  return created >= started - REQUEST_CLOCK_SLACK_MS;
}

function includesToken(content: string, token: string | undefined): boolean {
  const value = token?.trim().toLowerCase();
  return Boolean(value && value.length >= 4 && content.includes(value));
}

const GENERIC_ANIMATION_WORDS = new Set([
  "animation", "animated", "sprite", "sprites", "frame", "frames", "asset", "assets",
  "cozy", "pixel", "traveling", "walking", "moving", "motion", "preview",
]);

/** Withheld-result phrasing without the explicit marker. Keep in sync with `reports_withheld_publication` in Rust. */
const WITHHELD_PUBLICATION = /unpublished|not (?:been )?published|(?:did not|didn't|could not|couldn't|cannot|can't) publish/;

export function reportsGenerationFailure(content: string): boolean {
  const lower = content.toLowerCase();
  return lower.includes("generation_failed:")
    || lower.includes("unable to publish the")
    || lower.includes("withdrawing the candidate")
    || (lower.includes("did not pass the final visual acceptance gate") && lower.includes("restor"))
    || WITHHELD_PUBLICATION.test(lower);
}

export function reportsGenerationWarning(content: string): boolean {
  return content.toLowerCase().includes("generation_warning:");
}

/** Assistant or user text that describes a static master / rig source, not an animation deliverable. */
export function reportsStaticSpriteOnlyIntent(content: string): boolean {
  const lower = content.toLowerCase();
  return /\b(?:single frame|one frame|solo sprite|nessuna animazione|no animation|static sprite|master only|solo master|imagegen-sources|nessun frame|motion-?ready|pronto per la riggatura)\b/.test(lower)
    || /nessun\s+frame\s*\/\s*rig/.test(lower)
    || /\b(?:nessun|no)\s+(?:frame|rig)\s+generat/.test(lower);
}

function mentionsAnimation(content: string, name: string): boolean {
  if (includesToken(content, name)) return true;
  // Agent responses usually use a readable subject name ("caterpillar")
  // rather than the exact generated identifier ("cozy_caterpillar_traveling_wave").
  // A distinctive name token is enough to bind the real preview component.
  return name
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(word => word.length >= 5 && !GENERIC_ANIMATION_WORDS.has(word))
    .some(word => includesToken(content, word));
}

function generationFromAnimation(animation: Animation, assets: Asset[]): SpriteGenerationMetadata | undefined {
  const frames = animation.frames
    .map(frame => assets.find(asset => asset.id === frame.assetId))
    .filter((asset): asset is Asset => Boolean(asset));
  if (!frames.length) return;
  return {
    kind: "sprite-generation",
    name: animation.name,
    category: frames[0].category,
    fps: animation.fps,
    assetIds: frames.map(asset => asset.id),
    animationId: animation.id,
  };
}

function generationFromResponsePaths(
  pathResolvedAssets: Asset[],
  animations: Animation[],
  staticOnly = false,
): SpriteGenerationMetadata {
  if (!staticOnly) {
    const covering = animations.find(animation =>
      animation.frames.length === pathResolvedAssets.length
      && pathResolvedAssets.every(asset => animation.frames.some(frame => frame.assetId === asset.id)),
    );
    if (covering) return generationFromAnimation(covering, pathResolvedAssets)!;
  }
  return spriteGenerationCard(
    pathResolvedAssets,
    stripFrameSuffix(pathResolvedAssets[0].name),
    pathResolvedAssets[0].category,
    pathResolvedAssets.length > 1 ? 8 : 1,
  );
}

function sanitizeGenerationMetadata(
  metadata: SpriteGenerationMetadata,
  resolved: Asset[],
  animations: Animation[],
  staticOnly: boolean,
): SpriteGenerationMetadata {
  const animation = metadata.animationId
    ? animations.find(item => item.id === metadata.animationId)
    : undefined;
  const mismatchedAnimation = Boolean(
    animation
    && (staticOnly || animation.frames.length !== resolved.length),
  );
  if (!mismatchedAnimation) return metadata;
  return spriteGenerationCard(
    resolved,
    stripFrameSuffix(resolved[0].name),
    resolved[0].category,
    staticOnly ? 1 : (resolved.length > 1 ? metadata.fps : 1),
  );
}

export function inferMessageGeneration(message: Message, assets: Asset[], animations: Animation[]): SpriteGenerationMetadata | undefined {
  if (message.role !== "assistant" || message.status !== "completed" || reportsGenerationFailure(message.content)) return;
  const outcome = generationOutcomeOf(message);
  if (outcome) {
    // The finalizer recorded exactly what this request published. Never fall
    // back to guessing from prose, which can bind an older asset or animation.
    if (outcome.status !== "published") return;
    const stored = message.metadata.generation;
    if (!stored || typeof stored !== "object" || !("kind" in stored) || stored.kind !== "sprite-generation") return;
    const metadata = stored as SpriteGenerationMetadata;
    const resolved = metadata.assetIds
      .map(id => assets.find(asset => asset.id === id))
      .filter((asset): asset is Asset => Boolean(asset));
    return resolved.length === metadata.assetIds.length && resolved.length ? metadata : undefined;
  }
  const staticOnly = reportsStaticSpriteOnlyIntent(message.content);
  const responsePaths = extractAssetPathsFromResponse(message.content);
  const pathResolvedAssets = responsePaths
    .map(path => findAssetByManifestPath(assets, path))
    .filter((asset): asset is Asset => Boolean(asset));
  const stored = message.metadata.generation;
  if (stored && typeof stored === "object" && "kind" in stored && stored.kind === "sprite-generation") {
    const metadata = stored as SpriteGenerationMetadata;
    const resolved = metadata.assetIds
      .map(id => assets.find(asset => asset.id === id))
      .filter((asset): asset is Asset => Boolean(asset));
    if (
      pathResolvedAssets.length
      && !pathResolvedAssets.every(asset => metadata.assetIds.includes(asset.id))
    ) {
      return generationFromResponsePaths(pathResolvedAssets, animations, staticOnly || pathResolvedAssets.length === 1);
    }
    if (resolved.length === metadata.assetIds.length) {
      return sanitizeGenerationMetadata(metadata, resolved, animations, staticOnly);
    }
  }
  if (pathResolvedAssets.length) {
    return generationFromResponsePaths(pathResolvedAssets, animations, staticOnly || pathResolvedAssets.length === 1);
  }
  // Legacy messages without an explicit outcome: prose matching may only bind
  // assets and animations that did not exist before this turn started.
  const content = message.content.toLowerCase();
  const freshAnimations = animations.filter(animation => createdDuringOrAfter(message, animation));
  const mentionedAssets = assets.filter(asset => createdDuringOrAfter(message, asset)).filter(asset =>
    includesToken(content, asset.relativePath)
    || includesToken(content, asset.path)
    || includesToken(content, `${asset.name}.${asset.format}`)
    || (asset.name.length >= 8 && includesToken(content, asset.name))
  );
  const mentionedIds = new Set(mentionedAssets.map(asset => asset.id));

  const frameLinked = freshAnimations
    .map(animation => ({
      animation,
      frameMatches: animation.frames.filter(frame => mentionedIds.has(frame.assetId)).length,
    }))
    .filter(candidate => candidate.frameMatches > 0)
    .sort((left, right) => right.frameMatches - left.frameMatches);
  const animationGeneration = frameLinked[0] && !staticOnly
    ? generationFromAnimation(frameLinked[0].animation, assets)
    : undefined;
  if (animationGeneration) return animationGeneration;
  if (!mentionedAssets.length && !staticOnly) {
    const nameMatched = freshAnimations.find(animation => animation.name.length >= 8 && mentionsAnimation(content, animation.name));
    return nameMatched ? generationFromAnimation(nameMatched, assets) : undefined;
  }

  const first = mentionedAssets[0];
  return spriteGenerationCard(
    mentionedAssets,
    stripFrameSuffix(first.name),
    first.category,
    1,
  );
}

export function inferMessagePack(message: Message, packs: AssetPack[]): { pack: AssetPack; metadata: PackGenerationMetadata } | undefined {
  if (message.role !== "assistant" || message.status !== "completed" || reportsGenerationFailure(message.content)) return;
  const outcome = generationOutcomeOf(message);
  if (outcome && outcome.status !== "published") return;
  const stored = message.metadata.packGeneration;
  if (stored && typeof stored === "object" && "kind" in stored && stored.kind === "pack-generation" && "packId" in stored) {
    const pack = packs.find(item => item.id === stored.packId);
    if (pack) return { pack, metadata: stored as PackGenerationMetadata };
  }
  if (outcome) return;
  const content = message.content.toLowerCase();
  const pack = packs.filter(item => createdDuringOrAfter(message, item)).find(item =>
    includesToken(content, item.id)
    || includesToken(content, item.name)
    || item.files.some(file => includesToken(content, file))
  );
  return pack ? { pack, metadata: { kind: "pack-generation", packId: pack.id } } : undefined;
}

export function contentWithoutSpriteOutputLinks(content: string): string {
  return content
    .replace(/^.*!?\[[^\]]+\]\((?!(?:https?:|mailto:))[^)]+\).*$/gim, "")
    .replace(/^\s*(?:frames?|outputs?|files?)(?:\s+are\s+in)?\s*:\s*(?:assets|\.sprite-studio)[\\/].*$/gim, "")
    .replace(/\s*[-·]?\s*\[Frame\s+\d+\]\([^)]+\.png\)/gi, "")
    .replace(/^The source \[Sprite Studio spec\].*$/gim, "")
    // The desktop chat does not execute visualization directives. Once an
    // artifact card is present, hiding this raw fallback avoids showing the
    // user a fake component declaration as plain text.
    .replace(/^.*visualize.*(?:"path"|\.html).*$/gim, "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
