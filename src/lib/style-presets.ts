export type StylePresetId = "pixel-rpg" | "graphic-adventure" | "cozy-chibi";
export type ConversationStyleId = StylePresetId | "inherit";

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
];

export function parseStylePreset(value: unknown): StylePresetId {
  return STYLE_PRESETS.some(preset => preset.id === value) ? value as StylePresetId : "pixel-rpg";
}

export function parseConversationStyle(value: unknown): ConversationStyleId {
  return value === "inherit" ? "inherit" : STYLE_PRESETS.some(preset => preset.id === value) ? value as StylePresetId : "inherit";
}

export function stylePreset(id: StylePresetId): StylePreset {
  return STYLE_PRESETS.find(preset => preset.id === id) ?? STYLE_PRESETS[0];
}
