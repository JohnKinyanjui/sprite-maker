import type { Asset } from "$lib/types";

/** Normalize workspace-relative paths for cross-platform manifest matching. */
export function normalizeManifestPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/^\.\//, "");
}

export function manifestPathsMatch(left: string, right: string): boolean {
  return normalizeManifestPath(left) === normalizeManifestPath(right);
}

/** Resolve an asset by manifest path, ignoring slash style differences. */
export function findAssetByManifestPath(assets: Asset[], path: string): Asset | undefined {
  const normalized = normalizeManifestPath(path);
  return assets.find(asset => normalizeManifestPath(asset.relativePath) === normalized);
}

const ASSET_OUTPUT_PATH = /(?:^|[\s([{"'`]|])(assets[/\\][A-Za-z0-9_.\\/-]+\.(?:png|gif|webp))/gi;
const IMAGEGEN_OUTPUT_PATH = /(?:^|[\s([{"'`]|])(\.sprite-studio[/\\]imagegen-sources[/\\][A-Za-z0-9_.\\/-]+\.(?:png|gif|webp))/gi;
const MARKDOWN_ASSET_LINK = /\]\(((?:assets|\.sprite-studio)[/\\][^)\s]+\.(?:png|gif|webp))\)/gi;

/** Workspace-relative asset paths referenced in a completed provider response. */
export function extractAssetPathsFromResponse(response: string): string[] {
  const paths = new Set<string>();
  for (const match of response.matchAll(ASSET_OUTPUT_PATH)) {
    paths.add(normalizeManifestPath(match[1]));
  }
  for (const match of response.matchAll(IMAGEGEN_OUTPUT_PATH)) {
    paths.add(normalizeManifestPath(match[1]));
  }
  for (const match of response.matchAll(MARKDOWN_ASSET_LINK)) {
    paths.add(normalizeManifestPath(match[1]));
  }
  return [...paths];
}

/** True when chat completion should rescan assets cited in the provider response. */
export function shouldRecoverAssetsFromResponse(
  manifestAssetCount: number,
  response: string,
  generationFailed: boolean,
): boolean {
  return manifestAssetCount === 0
    && !generationFailed
    && extractAssetPathsFromResponse(response).length > 0;
}
