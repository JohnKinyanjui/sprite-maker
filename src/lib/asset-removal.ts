import { api } from "$lib/api";
import { confirmDialog } from "$lib/confirm-dialog";
import type { Asset, AssetUsage } from "$lib/types";

function usageSummary(usage: AssetUsage): string {
  const parts: string[] = [];
  if (usage.animationNames.length) {
    parts.push(`${usage.animationNames.length} animation(s): ${usage.animationNames.join(", ")}`);
  }
  if (usage.anchorSlugs.length) {
    parts.push(`character anchor(s): ${usage.anchorSlugs.join(", ")}`);
  }
  if (usage.spriteSheetItems) {
    parts.push(`${usage.spriteSheetItems} sprite sheet cell(s)`);
  }
  return parts.join("; ");
}

/** Ask the user and delete an asset, detaching references when needed. */
export async function confirmAndDeleteAsset(asset: Asset): Promise<boolean> {
  const usage = await api.getAssetUsage(asset.id);
  const blocked = usage.animationNames.length > 0
    || usage.anchorSlugs.length > 0
    || usage.spriteSheetItems > 0;
  if (blocked) {
    const accepted = await confirmDialog(
      `${asset.name} is still referenced (${usageSummary(usage)}).\n\nForce delete will remove the file, drop its frames from animations, clear anchors, and update the generation manifest.\n\nThis cannot be undone.`,
      { title: "Force delete asset", okLabel: "Delete", cancelLabel: "Cancel" },
    );
    if (!accepted) return false;
    await api.deleteAsset(asset.id, true);
    return true;
  }
  if (!await confirmDialog(
    `Delete ${asset.name} from this project? The file will be removed from disk.`,
    { title: "Delete asset", okLabel: "Delete", cancelLabel: "Cancel" },
  )) {
    return false;
  }
  await api.deleteAsset(asset.id);
  return true;
}
