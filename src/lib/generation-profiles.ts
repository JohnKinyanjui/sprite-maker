import type { ChatGenerationProfile, GenerationQuality, ProviderMode, SpriteSlashCommand } from "$lib/types";

export const GENERATION_PRESETS: Record<Exclude<GenerationQuality, "custom">, Pick<ChatGenerationProfile, "width" | "height" | "frames" | "fps">> = {
  low: { width: 32, height: 32, frames: 4, fps: 6 },
  mid: { width: 64, height: 64, frames: 6, fps: 8 },
  high: { width: 128, height: 128, frames: 8, fps: 12 },
};

export const SLASH_COMMANDS: { id: SpriteSlashCommand; label: string; description: string }[] = [
  { id: "animate", label: "/animate", description: "Generate a looping animation using this chat's frame settings" },
  { id: "sprite", label: "/sprite", description: "Generate one polished static sprite" },
  { id: "character", label: "/character", description: "Route the request through the ImageGen character harness" },
  { id: "effect", label: "/effect", description: "Create an animated game effect" },
  { id: "pack", label: "/pack", description: "Generate a coordinated set of animals, objects, or other game assets" },
];

const bounded = (value: unknown, fallback: number, minimum: number, maximum: number) => {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? Math.min(maximum, Math.max(minimum, Math.round(parsed))) : fallback;
};

export function profileForQuality(quality: Exclude<GenerationQuality, "custom">, current?: ChatGenerationProfile): ChatGenerationProfile {
  return { profileVersion: 3, quality, ...GENERATION_PRESETS[quality], frameMode: current?.frameMode ?? "auto", minFrames: current?.minFrames ?? 4, maxFrames: current?.maxFrames ?? 32, allowInterpolation: current?.profileVersion && current.profileVersion >= 3 ? current.allowInterpolation : true, allowAutoAdjust: current?.allowAutoAdjust ?? true, model: current?.model ?? "", reasoningEffort: current?.reasoningEffort ?? "" };
}

export function normalizeGenerationProfile(value: unknown, modes: ProviderMode[] = []): ChatGenerationProfile {
  const source = value && typeof value === "object" ? value as Partial<ChatGenerationProfile> : {};
  const quality: GenerationQuality = ["low", "mid", "high", "custom"].includes(String(source.quality)) ? source.quality as GenerationQuality : "mid";
  const base = quality === "custom" ? GENERATION_PRESETS.mid : GENERATION_PRESETS[quality];
  const requestedMode = modes.find(mode => mode.id === source.model) ?? modes[0];
  const model = requestedMode?.id ?? String(source.model ?? "");
  const requestedEffort = String(source.reasoningEffort ?? "");
  const reasoningEffort = requestedMode
    ? requestedMode.reasoningEfforts.includes(requestedEffort) ? requestedEffort : requestedMode.defaultReasoningEffort
    : requestedEffort;
  const frameMode = source.frameMode === "fixed" ? "fixed" : "auto";
  const minFrames = bounded(source.minFrames, 4, 1, 32);
  const isLegacyAutoRange = !source.profileVersion && frameMode === "auto" && minFrames === 4 && Number(source.maxFrames) === 12;
  const maxFrames = isLegacyAutoRange ? 32 : bounded(source.maxFrames, 32, minFrames, 32);
  return {
    profileVersion: 3,
    quality,
    width: bounded(source.width, base.width, 8, 512),
    height: bounded(source.height, base.height, 8, 512),
    frames: bounded(source.frames, base.frames, 1, 32),
    fps: bounded(source.fps, base.fps, 1, 60),
    frameMode,
    minFrames,
    maxFrames,
    allowInterpolation: source.profileVersion && source.profileVersion >= 3 ? Boolean(source.allowInterpolation) : true,
    allowAutoAdjust: frameMode === "auto" && (source.allowAutoAdjust ?? true),
    model,
    reasoningEffort,
  };
}

export function slashCommand(prompt: string): SpriteSlashCommand | undefined {
  const match = prompt.trimStart().match(/^\/(animate|sprite|character|effect|pack)(?:\s|$)/i);
  return match?.[1].toLowerCase() as SpriteSlashCommand | undefined;
}
