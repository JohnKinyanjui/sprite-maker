import { describe, expect, test } from "bun:test";
import {
  animationNameFromScan, completeManifestAssets, createdAssetsFromScan, findAnimationWithOrderedFrames,
  shouldCreateAnimationFromManifest, shouldHydrateGeneration, shouldReconcileManifest, spriteGenerationCard,
} from "../src/lib/generation-reconcile";
import type { Animation, Asset, GenerationManifest, Message } from "../src/lib/types";

const asset = (id: string, name: string, relativePath = `assets/creatures/${name}.png`): Asset => ({
  id, name, workspaceId: "workspace", path: `/workspace/${relativePath}`,
  relativePath, category: "creatures", format: "png",
  width: 64, height: 64, fileSize: 100, hasAlpha: true, createdAt: "now",
});

const animation = (frames: Asset[]): Animation => ({
  id: "animation", workspaceId: "workspace", name: "walk", fps: 8, looping: true,
  frames: frames.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
});

describe("generation reconcile", () => {
  test("detects assets that were not in the previous library", () => {
    const known = new Set(["a1"]);
    const created = createdAssetsFromScan(known, [asset("a1", "old"), asset("a2", "new")]);
    expect(created.map(item => item.id)).toEqual(["a2"]);
  });

  test("refuses to reconcile a pack or a single-frame manifest", () => {
    expect(shouldReconcileManifest(null)).toBe(false);
    expect(shouldReconcileManifest({ kind: "pack", name: "set", category: "props", fps: 1, files: ["a.png", "b.png"], generatedAt: "now" })).toBe(false);
    expect(shouldReconcileManifest({ name: "walk", category: "creatures", fps: 8, files: ["a.png"], generatedAt: "now" })).toBe(false);
    expect(shouldReconcileManifest({ name: "walk", category: "creatures", fps: 8, files: ["a.png", "b.png"], generatedAt: "now" })).toBe(true);
  });

  test("matches an animation only when frames are the same assets in order", () => {
    const frames = [asset("a1", "walk_01"), asset("a2", "walk_02")];
    const match = findAnimationWithOrderedFrames([animation(frames)], frames);
    expect(match?.id).toBe("animation");
    expect(findAnimationWithOrderedFrames([animation(frames)], [...frames].reverse())).toBeUndefined();
  });

  test("hydrates a chat card only when the assistant mentioned the manifest", () => {
    const files = [asset("a1", "slug_01"), asset("a2", "slug_02")];
    const manifest: GenerationManifest = { name: "slug", category: "creatures", fps: 8, files: files.map(item => item.relativePath), generatedAt: "now" };
    const assistant: Message = { id: "m", conversationId: "c", role: "assistant", kind: "text", content: "Wrote assets/creatures/slug_01.png", status: "completed", metadata: {}, createdAt: "now" };
    expect(shouldHydrateGeneration(assistant, manifest, files)).toBe(true);
    expect(shouldHydrateGeneration({ ...assistant, metadata: { generation: spriteGenerationCard(files, "slug", "creatures", 8) } }, manifest, files)).toBe(false);
  });

  test("never hydrates an older manifest onto a turn that did not publish it", () => {
    const files = [asset("a1", "slug_01"), asset("a2", "slug_02")];
    const staleManifest: GenerationManifest = { name: "slug", category: "creatures", fps: 8, files: files.map(item => item.relativePath), generatedAt: "2026-09-20T10:00:00Z" };
    const assistant: Message = { id: "m", conversationId: "c", role: "assistant", kind: "text", content: "Reworked the slug cycle; see assets/creatures/slug_01.png", status: "completed", metadata: {}, createdAt: "2026-09-25T12:00:00Z" };
    expect(shouldHydrateGeneration(assistant, staleManifest, files)).toBe(false);
    const freshManifest = { ...staleManifest, generatedAt: "2026-09-25T12:04:00Z" };
    expect(shouldHydrateGeneration(assistant, freshManifest, files)).toBe(true);
    const unpublished = { ...assistant, metadata: { generationOutcome: { kind: "generation-outcome", requestId: "r", status: "unpublished" } } };
    expect(shouldHydrateGeneration(unpublished, freshManifest, files)).toBe(false);
    expect(shouldHydrateGeneration({ ...assistant, status: "failed" }, freshManifest, files)).toBe(false);
  });

  test("builds animation names from the scan without the i-flag suffix strip", () => {
    const ordered = [asset("a1", "Hero_01")];
    expect(animationNameFromScan(null, ordered)).toBe("Hero");
    expect(shouldCreateAnimationFromManifest({ name: "walk", category: "creatures", fps: 8, files: ["a.png", "b.png"], generatedAt: "now" }, [ordered[0], asset("a2", "Hero_02")])).toBe(true);
    expect(completeManifestAssets([asset("a1", "walk_01", "a.png")], ["a.png", "b.png"])).toBeUndefined();
    expect(completeManifestAssets([asset("a1", "walk_01", "assets/creatures/walk_01.png")], ["assets\\creatures\\walk_01.png"])?.[0]?.id).toBe("a1");
  });
});
