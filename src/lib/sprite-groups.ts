import type { Animation, Asset } from "$lib/types";

export type SpriteGroup = { id: string; name: string; category: string; preview: Asset; frames: Asset[]; fps?: number };

export function buildSpriteGroups(assets: Asset[], animations: Animation[]): SpriteGroup[] {
  const byId = new Map(assets.map(asset => [asset.id, asset]));
  const grouped = new Set<string>();
  const result: SpriteGroup[] = [];

  for (const animation of animations) {
    const frames = animation.frames.map(frame => byId.get(frame.assetId)).filter((asset): asset is Asset => Boolean(asset));
    if (frames.length < 2 || frames.some(asset => grouped.has(asset.id))) continue;
    frames.forEach(asset => grouped.add(asset.id));
    result.push({ id: `animation:${animation.id}`, name: animation.name, category: frames[0].category, preview: frames[0], frames, fps: animation.fps });
  }

  const sequences = new Map<string, Asset[]>();
  for (const asset of assets) {
    if (grouped.has(asset.id)) continue;
    const base = asset.name.replace(/[_-]\d+$/, "");
    const key = `${asset.category}:${base}`;
    const sequence = sequences.get(key) ?? [];
    sequence.push(asset);
    sequences.set(key, sequence);
  }
  for (const [key, sequence] of sequences) {
    sequence.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    const name = key.slice(key.indexOf(":") + 1);
    result.push({ id: sequence.length > 1 ? `sequence:${key}` : `asset:${sequence[0].id}`, name: sequence.length > 1 ? name : sequence[0].name, category: sequence[0].category, preview: sequence[0], frames: sequence });
  }
  return result;
}
