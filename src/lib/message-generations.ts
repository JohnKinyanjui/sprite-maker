import type { Animation, Asset, AssetPack, Message, PackGenerationMetadata, SpriteGenerationMetadata } from "$lib/types";

function includesToken(content: string, token: string | undefined): boolean {
  const value = token?.trim().toLowerCase();
  return Boolean(value && value.length >= 4 && content.includes(value));
}

const GENERIC_ANIMATION_WORDS = new Set([
  "animation", "animated", "sprite", "sprites", "frame", "frames", "asset", "assets",
  "cozy", "pixel", "traveling", "walking", "moving", "motion", "preview",
]);

export function reportsGenerationFailure(content: string): boolean {
  const lower = content.toLowerCase();
  return lower.includes("generation_failed:")
    || lower.includes("unable to publish the")
    || lower.includes("withdrawing the candidate")
    || (lower.includes("did not pass the final visual acceptance gate") && lower.includes("restor"));
}

export function reportsGenerationWarning(content: string): boolean {
  return content.toLowerCase().includes("generation_warning:");
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

export function inferMessageGeneration(message: Message, assets: Asset[], animations: Animation[]): SpriteGenerationMetadata | undefined {
  if (message.role !== "assistant" || message.status !== "completed" || reportsGenerationFailure(message.content)) return;
  const stored = message.metadata.generation;
  if (stored && typeof stored === "object" && "kind" in stored && stored.kind === "sprite-generation") {
    return stored as SpriteGenerationMetadata;
  }
  const content = message.content.toLowerCase();
  const mentionedAssets = assets.filter(asset =>
    includesToken(content, asset.relativePath)
    || includesToken(content, asset.path)
    || includesToken(content, `${asset.name}.${asset.format}`)
    || (asset.name.length >= 8 && includesToken(content, asset.name))
  );
  const mentionedIds = new Set(mentionedAssets.map(asset => asset.id));

  const candidates = animations
    .map(animation => {
      const nameMatch = animation.name.length >= 8 && mentionsAnimation(content, animation.name);
      const frameMatches = animation.frames.filter(frame => mentionedIds.has(frame.assetId)).length;
      return { animation, nameMatch, frameMatches };
    })
    .filter(candidate => candidate.nameMatch || candidate.frameMatches > 0)
    .sort((left, right) => Number(right.nameMatch) - Number(left.nameMatch) || right.frameMatches - left.frameMatches);
  const animationGeneration = candidates[0] ? generationFromAnimation(candidates[0].animation, assets) : undefined;
  if (animationGeneration) return animationGeneration;
  if (!mentionedAssets.length) return;

  const first = mentionedAssets[0];
  return {
    kind: "sprite-generation",
    name: first.name,
    category: first.category,
    fps: 1,
    assetIds: [first.id],
  };
}

export function inferMessagePack(message: Message, packs: AssetPack[]): { pack: AssetPack; metadata: PackGenerationMetadata } | undefined {
  if (message.role !== "assistant" || message.status !== "completed" || reportsGenerationFailure(message.content)) return;
  const stored = message.metadata.packGeneration;
  if (stored && typeof stored === "object" && "kind" in stored && stored.kind === "pack-generation" && "packId" in stored) {
    const pack = packs.find(item => item.id === stored.packId);
    if (pack) return { pack, metadata: stored as PackGenerationMetadata };
  }
  const content = message.content.toLowerCase();
  const pack = packs.find(item =>
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
