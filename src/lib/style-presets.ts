import type { CustomArtStyle } from "$lib/library-types";

export type StylePresetId = string;
export type ConversationStyleId = string;

export type StylePreset = {
  id: StylePresetId;
  name: string;
  description: string;
  thumbnail: string;
  prompt: string;
};

export const STYLE_PRESETS: StylePreset[] = [
  {
    id: "pixel-rpg",
    name: "Pixel RPG",
    description: "Crisp clusters, warm palette, compact game scale",
    thumbnail: "/style-presets/pixel-rpg.webp",
    prompt: "crisp handcrafted pixel RPG character, compact readable clusters, warm restrained palette, clear face and silhouette",
  },
  {
    id: "graphic-adventure",
    name: "Graphic adventure",
    description: "Angular shapes, bold silhouette, painted planes",
    thumbnail: "/style-presets/graphic-adventure.webp",
    prompt: "premium graphic adventure character, bold angular silhouette, simplified painted planes, controlled asymmetry and layered costume shapes",
  },
  {
    id: "cozy-chibi",
    name: "Cozy chibi",
    description: "Rounded proportions, expressive face, clean outlines",
    thumbnail: "/style-presets/cozy-chibi.webp",
    prompt: "polished cozy chibi game character, rounded proportions, oversized expressive head, clean dark outline and simple readable shapes",
  },
  {
    id: "limited-palette",
    name: "Limited palette",
    description: "Tight color ramp, deliberate clusters, crisp dithering",
    thumbnail: "/style-presets/limited-palette.svg",
    prompt: "handcrafted limited-palette pixel art, deliberate pixel clusters, one compact color ramp, selective dithering, crisp silhouette and no soft antialiasing",
  },
  {
    id: "isometric-pixel",
    name: "Isometric pixel",
    description: "2:1 projection, readable planes, consistent top lighting",
    thumbnail: "/style-presets/isometric-pixel.svg",
    prompt: "polished 2:1 isometric pixel game art, consistent projection and top-left lighting, readable top and side planes, compact controlled palette",
  },
  {
    id: "painterly-fantasy",
    name: "Painterly fantasy",
    description: "Soft painted planes, storybook texture, rich materials",
    thumbnail: "/style-presets/painterly-fantasy.svg",
    prompt: "original painterly fantasy game art, softly textured brushwork, layered material shapes, atmospheric color harmony, readable gameplay silhouette",
  },
  {
    id: "cel-shaded",
    name: "Cel shaded",
    description: "Bold contour, flat shadow shapes, saturated accents",
    thumbnail: "/style-presets/cel-shaded.svg",
    prompt: "clean cel-shaded 2D game art, confident dark contour, flat graphic shadow shapes, saturated accent colors, highly readable silhouette",
  },
  {
    id: "one-bit",
    name: "One-bit",
    description: "Two colors, strong negative space, retro clarity",
    thumbnail: "/style-presets/one-bit.svg",
    prompt: "high-clarity one-bit pixel art using exactly two colors, bold negative space, intentional clusters, no gray pixels and no antialiasing",
  },
];

export function allStylePresets(custom: CustomArtStyle[] = []): StylePreset[] {
  return [...STYLE_PRESETS, ...custom.map(art => ({ id: art.id, name: art.name, description: art.description, thumbnail: art.thumbnail, prompt: art.prompt }))];
}

export function parseStylePreset(value: unknown, custom: CustomArtStyle[] = []): StylePresetId {
  return allStylePresets(custom).some(preset => preset.id === value) ? String(value) : "pixel-rpg";
}

export function parseConversationStyle(value: unknown, custom: CustomArtStyle[] = []): ConversationStyleId {
  return value === "inherit" ? "inherit" : allStylePresets(custom).some(preset => preset.id === value) ? String(value) : "inherit";
}

export function stylePreset(id: StylePresetId, custom: CustomArtStyle[] = []): StylePreset {
  return allStylePresets(custom).find(preset => preset.id === id) ?? STYLE_PRESETS[0];
}
