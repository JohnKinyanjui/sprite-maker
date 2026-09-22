import { describe, expect, test } from "bun:test";
import {
  extractAssetPathsFromResponse, findAssetByManifestPath, manifestPathsMatch, normalizeManifestPath,
  shouldRecoverAssetsFromResponse,
} from "../src/lib/manifest-path";
import type { Asset } from "../src/lib/types";

const asset = (relativePath: string): Asset => ({
  id: relativePath,
  name: relativePath,
  workspaceId: "ws",
  path: `/tmp/${relativePath}`,
  relativePath,
  category: "characters",
  format: "png",
  width: 64,
  height: 64,
  fileSize: 100,
  hasAlpha: true,
  createdAt: "now",
});

describe("manifest path normalization", () => {
  test("normalizes slashes and leading ./", () => {
    expect(normalizeManifestPath(".\\assets\\frame-01.png")).toBe("assets/frame-01.png");
    expect(normalizeManifestPath("./assets/frame-01.png")).toBe("assets/frame-01.png");
    expect(manifestPathsMatch("assets/a.png", ".\\assets\\a.png")).toBe(true);
  });

  test("finds assets across path separator styles", () => {
    const library = [asset("assets/characters/hero.png")];
    expect(findAssetByManifestPath(library, "assets\\characters\\hero.png")?.id).toBe("assets/characters/hero.png");
  });

  test("extracts asset output paths from provider prose and markdown links", () => {
    const response = "Saved assets/characters/astral_cartographer.png.\n\n[Preview](assets/characters/astral_cartographer.png)";
    expect(extractAssetPathsFromResponse(response)).toEqual(["assets/characters/astral_cartographer.png"]);
    expect(extractAssetPathsFromResponse("Wrote assets\\characters\\knight.png on Windows.")).toEqual(["assets/characters/knight.png"]);
    expect(extractAssetPathsFromResponse(
      "Master in .sprite-studio/imagegen-sources/stalker-idle/master.png",
    )).toEqual([".sprite-studio/imagegen-sources/stalker-idle/master.png"]);
  });

  test("requests asset recovery only when the manifest missed cited output paths", () => {
    const response = "Saved assets/characters/knight.png.";
    expect(shouldRecoverAssetsFromResponse(0, response, false)).toBe(true);
    expect(shouldRecoverAssetsFromResponse(1, response, false)).toBe(false);
    expect(shouldRecoverAssetsFromResponse(0, response, true)).toBe(false);
    expect(shouldRecoverAssetsFromResponse(0, "Done.", false)).toBe(false);
  });

  test("requests asset recovery only when the manifest missed cited output paths", () => {
    const response = "Saved assets/characters/knight.png.";
    expect(shouldRecoverAssetsFromResponse(0, response, false)).toBe(true);
    expect(shouldRecoverAssetsFromResponse(1, response, false)).toBe(false);
    expect(shouldRecoverAssetsFromResponse(0, response, true)).toBe(false);
    expect(shouldRecoverAssetsFromResponse(0, "Done.", false)).toBe(false);
  });
});
