export type CustomSkill = {
  id: string;
  name: string;
  description: string;
  instructions: string;
  enabled: boolean;
  createdAt: string;
};

export type CustomArtStyle = {
  id: string;
  name: string;
  description: string;
  prompt: string;
  thumbnail: string;
  createdAt: string;
};

export function parseCustomSkills(value: unknown): CustomSkill[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is CustomSkill => Boolean(
    item && typeof item === "object" &&
    typeof item.id === "string" && typeof item.name === "string" &&
    typeof item.description === "string" && typeof item.instructions === "string"
  )).map(item => ({ ...item, enabled: item.enabled !== false, createdAt: item.createdAt || new Date().toISOString() }));
}

export function parseCustomArts(value: unknown): CustomArtStyle[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is CustomArtStyle => Boolean(
    item && typeof item === "object" &&
    typeof item.id === "string" && typeof item.name === "string" &&
    typeof item.description === "string" && typeof item.prompt === "string"
  )).map(item => ({ ...item, thumbnail: item.thumbnail || "", createdAt: item.createdAt || new Date().toISOString() }));
}
