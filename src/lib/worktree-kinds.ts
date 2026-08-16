import type { WorktreeKind } from "$lib/types";

export type CreatableWorktreeKind = Exclude<WorktreeKind, "general">;

export const CREATABLE_WORKTREE_KINDS: ReadonlyArray<{
  value: CreatableWorktreeKind;
  label: string;
  description: string;
}> = [
  { value: "character", label: "Character", description: "Heroes, NPCs, and character-focused animation" },
  { value: "creature", label: "Creature", description: "Animals, monsters, enemies, and bosses" },
  { value: "object", label: "Game Object", description: "Props, equipment, pickups, and interactables" },
  { value: "environment", label: "Environment", description: "Scenes, locations, structures, and world art" },
  { value: "tileset", label: "Terrain / Tileset", description: "Complete terrain atlases with compatible fills, edges, and corners" },
  { value: "animation", label: "Animation", description: "Motion-focused work using existing or new sprites" },
  { value: "vfx", label: "VFX", description: "Animated effects such as fire, magic, impacts, and weather" },
  { value: "ui", label: "UI", description: "HUD elements, menus, icons, and interface art" },
];

export function inferWorktreeKind(value: string): CreatableWorktreeKind {
  const normalized = value.toLowerCase();
  if (/\b(vfx|fx|effect|effects|magic|spell)\b/.test(normalized)) return "vfx";
  if (/\b(character|hero|player|npc|ranger|knight|warrior|mage)\b/.test(normalized)) return "character";
  if (/\b(creature|monster|enemy|animal|centipede|slime|boss)\b/.test(normalized)) return "creature";
  if (/\b(environment|world|forest|dungeon|cave|village|biome|scene)\b/.test(normalized)) return "environment";
  if (/\b(tile|tiles|tileset|terrain)\b/.test(normalized)) return "tileset";
  if (/\b(animation|animations|motion|moveset)\b/.test(normalized)) return "animation";
  if (/\b(ui|hud|interface|menu|icons?)\b/.test(normalized)) return "ui";
  return "object";
}
