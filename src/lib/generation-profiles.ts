import type { ChatGenerationProfile, GenerationQuality, ProviderMode, SpriteSlashCommand } from "$lib/types";

export const GENERATION_PRESETS: Record<Exclude<GenerationQuality, "custom">, Pick<ChatGenerationProfile, "width" | "height" | "frames" | "fps" | "minFrames" | "maxFrames">> = {
  low: { width: 64, height: 64, frames: 6, fps: 8, minFrames: 6, maxFrames: 8 },
  mid: { width: 128, height: 128, frames: 8, fps: 12, minFrames: 8, maxFrames: 12 },
  high: { width: 256, height: 256, frames: 12, fps: 12, minFrames: 12, maxFrames: 16 },
};

export const SLASH_COMMANDS: { id: SpriteSlashCommand; label: string; description: string }[] = [
  { id: "animate", label: "/animate", description: "Generate a looping animation using this chat's frame settings" },
  { id: "rig", label: "/rig", description: "Ask the AI to place rig points and poses on a sprite; review and render in the Rig tab" },
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
  return { profileVersion: 8, quality, ...GENERATION_PRESETS[quality], frameMode: current?.frameMode ?? "auto", allowInterpolation: current?.allowInterpolation ?? true, allowAutoAdjust: current?.allowAutoAdjust ?? true, model: current?.model ?? "", reasoningEffort: current?.reasoningEffort ?? "", imageProviderId: current?.imageProviderId ?? "imagegen" };
}

export function normalizeGenerationProfile(value: unknown, modes: ProviderMode[] = []): ChatGenerationProfile {
  const source = value && typeof value === "object" ? value as Partial<ChatGenerationProfile> : {};
  const quality: GenerationQuality = ["low", "mid", "high", "custom"].includes(String(source.quality)) ? source.quality as GenerationQuality : "mid";
  const base = quality === "custom" ? GENERATION_PRESETS.mid : GENERATION_PRESETS[quality];
  const upgradeLegacyPreset = (source.profileVersion ?? 0) < 8 && quality !== "custom";
  const requestedMode = modes.find(mode => mode.id === source.model) ?? modes[0];
  const model = requestedMode?.id ?? String(source.model ?? "");
  const requestedEffort = String(source.reasoningEffort ?? "");
  const reasoningEffort = requestedMode
    ? requestedMode.reasoningEfforts.includes(requestedEffort) ? requestedEffort : requestedMode.defaultReasoningEffort
    : requestedEffort;
  const frameMode = source.frameMode === "fixed" ? "fixed" : "auto";
  const minFrames = bounded(upgradeLegacyPreset ? base.minFrames : source.minFrames, base.minFrames, 1, 32);
  const maxFrames = bounded(upgradeLegacyPreset ? base.maxFrames : source.maxFrames, base.maxFrames, minFrames, 32);
  return {
    profileVersion: 8,
    quality,
    width: bounded(upgradeLegacyPreset ? base.width : source.width, base.width, 8, 512),
    height: bounded(upgradeLegacyPreset ? base.height : source.height, base.height, 8, 512),
    frames: bounded(upgradeLegacyPreset ? base.frames : source.frames, base.frames, 1, 32),
    fps: bounded(upgradeLegacyPreset ? base.fps : source.fps, base.fps, 1, 60),
    frameMode,
    minFrames,
    maxFrames,
    allowInterpolation: source.allowInterpolation ?? true,
    allowAutoAdjust: frameMode === "auto" && (source.allowAutoAdjust ?? true),
    model,
    reasoningEffort,
    imageProviderId: String(source.imageProviderId ?? "imagegen"),
  };
}

export function slashCommand(prompt: string): SpriteSlashCommand | undefined {
  const match = prompt.trimStart().match(/^\/(animate|sprite|character|effect|pack|rig)(?:\s|$)/i);
  return match?.[1].toLowerCase() as SpriteSlashCommand | undefined;
}
