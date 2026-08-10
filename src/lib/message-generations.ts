import type { Animation, Asset, Message, SpriteGenerationMetadata } from "$lib/types";

function includesToken(content: string, token: string | undefined): boolean {
  const value = token?.trim().toLowerCase();
  return Boolean(value && value.length >= 4 && content.includes(value));
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
  const stored = message.metadata.generation;
  if (stored && typeof stored === "object" && "kind" in stored && stored.kind === "sprite-generation") {
    return stored as SpriteGenerationMetadata;
  }
  if (message.role !== "assistant" || message.status !== "completed") return;

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
      const nameMatch = animation.name.length >= 8 && includesToken(content, animation.name);
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

export function contentWithoutSpriteOutputLinks(content: string): string {
  return content
    .replace(/^.*!?\[[^\]]+\]\((?!(?:https?:|mailto:))[^)]+\).*$/gim, "")
    .replace(/^\s*(?:frames?|outputs?|files?)(?:\s+are\s+in)?\s*:\s*(?:assets|\.sprite-studio)[\\/].*$/gim, "")
    .replace(/\s*[-·]?\s*\[Frame\s+\d+\]\([^)]+\.png\)/gi, "")
    .replace(/^The source \[Sprite Studio spec\].*$/gim, "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
