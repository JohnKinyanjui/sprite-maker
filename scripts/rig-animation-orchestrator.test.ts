import { describe, expect, test } from "bun:test";
import {
  extractAnimateMotion,
  extractMasterPathFromPrompt,
  formatBlockingQualityNotice,
  isAnimateWithoutResolvableMaster,
  needsNativeRigMasterPhase,
  parsePolishModeFromPrompt,
  resolveAnimateMasterAsset,
  resolveLatestCharacterAsset,
  resolveMasterAfterProviderMasterPhase,
  resolveMasterFromManifest,
  resolvePolishMode,
  shouldOrchestrateNativeRig,
  shouldRunNativeRigFirst,
} from "../src/lib/rig-animation-orchestrator";
import type { Asset, GenerationManifest } from "../src/lib/types";

const asset = (id: string, name: string, relativePath = `assets/${name}`): Asset => ({
  id,
  name,
  workspaceId: "ws",
  path: `/tmp/${name}`,
  relativePath,
  category: "characters",
  format: "png",
  width: 64,
  height: 64,
  fileSize: 100,
  hasAlpha: true,
  createdAt: "2026-09-12T12:00:00.000Z",
});

describe("rig animation orchestrator helpers", () => {
  test("prefers the chat-selected animation mode for /animate", () => {
    const prompt = "/animate Use assets/hero.png as the exact source master. Motion: walk. Polish mode: Full redraw.";
    expect(resolvePolishMode(prompt, "animate", "rig")).toBe("rig");
    expect(resolvePolishMode(prompt, "animate", "ai-polish")).toBe("ai-polish");
    expect(resolvePolishMode("create a walk cycle", undefined, "ai-polish")).toBe("rig");
  });

  test("runs native rig first for rig, ai-polish, and full-redraw animate", () => {
    const asset = { id: "a1", relativePath: "assets/hero.png" } as Asset;
    expect(shouldRunNativeRigFirst("animate", "rig", asset)).toBe(true);
    expect(shouldRunNativeRigFirst("animate", "ai-polish", asset)).toBe(true);
    expect(shouldRunNativeRigFirst("animate", "full-redraw", asset)).toBe(true);
  });

  test("routes rig-only animate with a context asset through native orchestration", () => {
    expect(shouldOrchestrateNativeRig("animate", "rig", asset("asset-1", "hero.png"))).toBe(true);
    expect(shouldOrchestrateNativeRig("animate", "ai-polish", asset("asset-1", "hero.png"))).toBe(false);
    expect(shouldOrchestrateNativeRig("sprite", "rig", asset("asset-1", "hero.png"))).toBe(false);
  });

  test("parses polish mode from motion prompts", () => {
    expect(parsePolishModeFromPrompt("Motion: walk. Polish mode: Rig only.")).toBe("rig");
    expect(parsePolishModeFromPrompt("Motion: walk. Polish mode: AI polish.")).toBe("ai-polish");
    expect(parsePolishModeFromPrompt("Motion: walk. Polish mode: Full redraw.")).toBe("full-redraw");
  });

  test("extracts motion text from /animate prompts", () => {
    expect(extractAnimateMotion("/animate a hunter walking forward")).toBe("a hunter walking forward");
    expect(extractAnimateMotion("/animate Use hero.png as the exact source master. Motion: idle breathing loop.")).toBe("idle breathing loop");
  });

  test("resolves master asset from prompt path without sidebar selection", () => {
    const library = [asset("a1", "hero.png", "assets/characters/hero.png")];
    const prompt = "/animate Use assets/characters/hero.png as the exact source master. Motion: walk cycle.";
    expect(extractMasterPathFromPrompt(prompt)).toBe("assets/characters/hero.png");
    expect(resolveAnimateMasterAsset(prompt, library)).toEqual(library[0]);
    expect(shouldOrchestrateNativeRig("animate", "rig", resolveAnimateMasterAsset(prompt, library))).toBe(true);
    const shortPrompt = "/animate Use assets/characters/hero.png. Motion: walk.";
    expect(resolveAnimateMasterAsset(shortPrompt, library)?.id).toBe("a1");
  });

  test("detects new-character animated requests that need a master-only pass", () => {
    expect(needsNativeRigMasterPhase("create a warrior walking forward", undefined, undefined)).toBe(true);
    expect(needsNativeRigMasterPhase("create a warrior walking forward", undefined, undefined, "ai-polish")).toBe(true);
    expect(needsNativeRigMasterPhase("create a warrior walking forward", undefined, undefined, "full-redraw")).toBe(true);
    expect(needsNativeRigMasterPhase("/animate walk cycle", "animate", undefined)).toBe(false);
    expect(needsNativeRigMasterPhase("create a warrior that walks. Polish mode: AI polish.", undefined, undefined, "ai-polish")).toBe(true);
    expect(needsNativeRigMasterPhase("create a static portrait", undefined, undefined)).toBe(false);
  });

  test("does not treat the fly substring inside butterfly as motion intent", () => {
    expect(needsNativeRigMasterPhase("create a butterfly", undefined, undefined)).toBe(false);
    expect(needsNativeRigMasterPhase("create a butterfly flying around", undefined, undefined)).toBe(true);
  });

  test("blocks /animate when no master can be resolved", () => {
    expect(isAnimateWithoutResolvableMaster("animate", "rig", "/animate walk cycle", [])).toBe(true);
    expect(isAnimateWithoutResolvableMaster("animate", "ai-polish", "/animate walk cycle", [])).toBe(true);
    expect(isAnimateWithoutResolvableMaster("animate", "full-redraw", "/animate walk cycle", [])).toBe(true);
    expect(isAnimateWithoutResolvableMaster(
      "animate",
      "rig",
      "/animate Use assets/characters/hero.png as the exact source master. Motion: walk.",
      [asset("a1", "hero.png", "assets/characters/hero.png")],
    )).toBe(false);
  });

  test("recovers the master from provider response paths when manifest freshness fails", () => {
    const soldier = asset("soldier", "soldier.png", "assets/characters/soldier.png");
    const library = [asset("old", "knight.png", "assets/characters/knight.png"), soldier];
    expect(resolveMasterAfterProviderMasterPhase({
      manifest: null,
      freshManifest: false,
      manifestChanged: false,
      scanned: [],
      library,
      responsePaths: ["assets/characters/soldier.png"],
    })?.id).toBe("soldier");
  });

  test("prefers manifest source when continuing after master-only generation", () => {
    const library = [
      asset("old", "knight.png", "assets/characters/knight.png"),
      asset("new", "warrior.png", "assets/characters/warrior.png"),
    ];
    library[0].createdAt = "2026-09-10T12:00:00.000Z";
    library[1].createdAt = "2026-09-12T12:00:00.000Z";
    const manifest: GenerationManifest = {
      name: "warrior",
      category: "characters",
      fps: 8,
      files: ["assets/characters/warrior.png"],
      generatedAt: "2026-09-12T12:00:00.000Z",
      source: "assets/characters/warrior.png",
    };
    expect(resolveMasterFromManifest(manifest, library)?.id).toBe("new");
    expect(resolveLatestCharacterAsset(library)?.id).toBe("new");
  });

  test("surfaces blocking quality checks after rig-only completion", () => {
    const notice = formatBlockingQualityNotice([
      { severity: "warning", checkType: "duplicate", message: "Adjacent frames are identical", ignored: false },
    ]);
    expect(notice).toContain("Adjacent frames are identical");
  });
});
