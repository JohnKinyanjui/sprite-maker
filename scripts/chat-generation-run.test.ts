import { describe, expect, test } from "bun:test";
import type { ActiveChatRequest } from "../src/lib/chat-generation-finalize";
import { generationRequestFromProfile } from "../src/lib/chat-generation-finalize";
import { attachAiPolishHandoffMetadata, normalizeManifestPath, roughIndexForPolishedPath } from "../src/lib/chat-generation-run";
import { normalizeGenerationProfile } from "../src/lib/generation-profiles";
import type { Asset } from "../src/lib/types";

describe("chat generation run helpers", () => {
  test("normalizes manifest paths for comparison", () => {
    expect(normalizeManifestPath(".\\assets\\frame-01.png")).toBe("assets/frame-01.png");
    expect(normalizeManifestPath("./assets/frame-01.png")).toBe("assets/frame-01.png");
  });

  test("maps polished manifest paths back to rough frame indices", () => {
    const rough = [
      "assets/animations/walk/frame-00.png",
      "assets/animations/walk/frame-01.png",
    ];
    expect(roughIndexForPolishedPath(rough, "assets/animations/walk/frame-01.png")).toBe(1);
    expect(roughIndexForPolishedPath(rough, ".\\assets\\animations\\walk\\frame-00.png")).toBe(0);
    expect(roughIndexForPolishedPath(rough, "assets/animations/walk/frame-99.png")).toBe(-1);
  });

  test("does not match rough paths by array index alone", () => {
    const rough = ["assets/a.png", "assets/b.png"];
    expect(roughIndexForPolishedPath(rough, "assets/b.png")).toBe(1);
    expect(roughIndexForPolishedPath(rough, "assets/a.png")).toBe(0);
  });

  test("attaches ai polish handoff metadata to active chat requests", () => {
    const frameAssets = [
      { id: "a1", relativePath: "assets/frame-00.png" } as Asset,
      { id: "a2", relativePath: "assets/frame-01.png" } as Asset,
    ];
    const request: ActiveChatRequest = {
      id: "req-1",
      conversationId: "chat-1",
      workspaceId: "ws-1",
      prompt: "polish",
      command: "animate",
      generation: generationRequestFromProfile(normalizeGenerationProfile(null)),
      knownPackIds: [],
      startedAt: Date.now(),
    };
    const enriched = attachAiPolishHandoffMetadata(request, {
      masterPath: "assets/hero.png",
      frameAssets,
      roughBackups: ["archive/frame-00.png", "archive/frame-01.png"],
    });
    expect(enriched.polishMode).toBe("ai-polish");
    expect(enriched.masterPath).toBe("assets/hero.png");
    expect(enriched.roughFramePaths).toEqual(["assets/frame-00.png", "assets/frame-01.png"]);
    expect(enriched.roughFrameBackupPaths).toEqual(["archive/frame-00.png", "archive/frame-01.png"]);
  });
});
