import { describe, expect, test } from "bun:test";
import {
  appendAssistantDelta, applyAnimationPolishModeToPrompt, buildFullRedrawPrompt, buildMotionPrompt, chatActivityLines, clearConversationRunningRequest, clearRunningRequest, generationOutcome, generationViewHandoff, inferChatCommand,
  isFreshGenerationManifest, isRejectedStaticAnimation, mergeAssistantGenerationMetadata, orderedGenerationAssets,
  parallelGenerationsInWorkspace, requestAssistantMessage, spriteCardForOrderedAssets, stripFrameSuffix, unacceptedGenerationNotice,
} from "../src/lib/chat-generation-finalize";
import { normalizeGenerationProfile } from "../src/lib/generation-profiles";
import type { Asset, Message } from "../src/lib/types";

const asset = (id: string, name: string): Asset => ({
  id, name, workspaceId: "workspace", path: `/workspace/assets/${name}.png`,
  relativePath: `assets/${name}.png`, category: "creatures", format: "png",
  width: 64, height: 64, fileSize: 100, hasAlpha: true, createdAt: "now",
});

describe("chat generation finalize", () => {
  test("infers pack from slash commands and from pack-like prose", () => {
    expect(inferChatCommand("/pack forest animals")).toBe("pack");
    expect(inferChatCommand("Generate an asset pack of mushrooms")).toBe("pack");
    expect(inferChatCommand("/animate a walk cycle")).toBe("animate");
    expect(inferChatCommand("Draw one hero sprite")).toBeUndefined();
  });

  test("rejects a stale workspace manifest that predates the request", () => {
    const manifest = { name: "walk", category: "creatures", fps: 8, files: ["a.png"], generatedAt: "2026-01-01T00:00:00.000Z" };
    expect(isFreshGenerationManifest(manifest, "new", "new", Date.parse("2026-09-01T00:00:00.000Z"))).toBe(false);
    expect(isFreshGenerationManifest(manifest, "new", "old", Date.parse("2026-01-01T00:00:00.000Z"))).toBe(true);
    expect(isFreshGenerationManifest(manifest, "new", "old", Date.parse("2026-01-01T00:00:00.000Z"), "Done.")).toBe(true);
    expect(isFreshGenerationManifest(manifest, "new", "old", Date.parse("2026-01-01T00:00:00.000Z"), "unrelated output", true)).toBe(false);
    expect(isFreshGenerationManifest(manifest, "new", "old", Date.parse("2026-01-01T00:00:00.000Z"), "Saved walk to a.png", true)).toBe(true);
  });

  test("requires response attribution only when parallel generations share a workspace", () => {
    const running = {
      a: { id: "1", conversationId: "a", workspaceId: "ws", prompt: "", generation: {} as never, knownPackIds: [], startedAt: 0 },
      b: { id: "2", conversationId: "b", workspaceId: "ws", prompt: "", generation: {} as never, knownPackIds: [], startedAt: 0 },
      c: { id: "3", conversationId: "c", workspaceId: "other", prompt: "", generation: {} as never, knownPackIds: [], startedAt: 0 },
    };
    expect(parallelGenerationsInWorkspace(running, "ws")).toBe(true);
    expect(parallelGenerationsInWorkspace({ a: running.a }, "ws")).toBe(false);
  });

  test("does not accept a single static frame as a completed /animate", () => {
    expect(isRejectedStaticAnimation("animate", [asset("a1", "walk")])).toBe(true);
    expect(isRejectedStaticAnimation("sprite", [asset("a1", "hero")])).toBe(false);
    expect(unacceptedGenerationNotice(true)).toContain("at least two fresh frames");
  });

  test("hands an animation tab to the conversation that requested it", () => {
    const handoff = generationViewHandoff({
      selectedConversationId: "chat", requestConversationId: "chat", animationId: "anim",
      ordered: [asset("a1", "walk_01"), asset("a2", "walk_02")], command: "animate", rejectedStaticAnimation: false,
    });
    expect(handoff).toEqual({ kind: "animation", animationId: "anim" });
    expect(generationViewHandoff({
      selectedConversationId: "chat", requestConversationId: "chat", animationId: "anim", rigId: "rig-1",
      ordered: [asset("a1", "walk_01"), asset("a2", "walk_02")], command: "animate", rejectedStaticAnimation: false,
    })).toEqual({ kind: "animation", animationId: "anim", rigId: "rig-1" });
    expect(generationViewHandoff({
      selectedConversationId: "other", requestConversationId: "chat", animationId: "anim",
      ordered: [], command: "animate", rejectedStaticAnimation: false,
    }).kind).toBe("none");
  });

  test("keeps manifest order when the renderer accepted frames", () => {
    const related = [asset("b", "b"), asset("a", "a")];
    expect(orderedGenerationAssets(related, related).map(item => item.id)).toEqual(["b", "a"]);
    expect(orderedGenerationAssets([], related).map(item => item.relativePath)).toEqual(["assets/a.png", "assets/b.png"]);
  });

  test("injects the selected polish mode into typed /animate prompts", () => {
    expect(applyAnimationPolishModeToPrompt("/animate walk cycle", "full-redraw")).toContain("Polish mode: Full redraw");
    expect(applyAnimationPolishModeToPrompt(
      "/animate Use assets/hero.png as the exact source master. Motion: walk. Polish mode: Rig only. Keep it tight.",
      "ai-polish",
    )).toContain("Polish mode: AI polish");
    expect(applyAnimationPolishModeToPrompt(
      "/animate Use assets/hero.png as the exact source master. Motion: walk. Polish mode: Rig only. Keep it tight.",
      "ai-polish",
    )).not.toContain("Polish mode: Rig only");
  });

  test("builds a full redraw prompt from rig frame paths", () => {
    const frames = [asset("a1", "walk_01"), asset("a2", "walk_02")];
    const animation = { id: "anim", workspaceId: "workspace", name: "walk", fps: 8, looping: true, frames: frames.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now" };
    const prompt = buildFullRedrawPrompt(animation, frames, "walk cycle");
    expect(prompt).toContain("experimental full AI redraw");
    expect(prompt).toContain("assets/walk_01.png");
    expect(prompt).toContain("walk cycle");
  });

  test("builds a motion prompt from the polish mode and frame budget", () => {
    const profile = normalizeGenerationProfile({ quality: "mid", frameMode: "fixed", frames: 8, fps: 12 });
    const prompt = buildMotionPrompt({ relativePath: "assets/hero.png" }, "walk cycle", "rig", profile);
    expect(prompt).toContain("/animate Use assets/hero.png");
    expect(prompt).toContain("8 frames");
    expect(prompt).toContain("Polish mode: Rig only");
    expect(prompt).toContain("validate the loop headlessly");
    expect(prompt).not.toContain("preview at least three cycles");
  });

  describe("explicit request outcome", () => {
    const base = { requestId: "req", generationFailed: false, ordered: [] as Asset[], rejectedStaticAnimation: false };

    test("classifies what the current request actually published", () => {
      expect(generationOutcome({ ...base, ordered: [asset("a1", "run_01"), asset("a2", "run_02")] }).status).toBe("published");
      expect(generationOutcome(base).status).toBe("unpublished");
      expect(generationOutcome({ ...base, generationFailed: true, ordered: [asset("a1", "run_01")] }).status).toBe("failed");
      expect(generationOutcome({ ...base, command: "animate", rejectedStaticAnimation: true }).status).toBe("failed");
      expect(generationOutcome({ ...base, command: "rig" }).status).toBe("none");
      expect(generationOutcome({ ...base, command: "pack" }).status).toBe("unpublished");
      expect(generationOutcome({ ...base, command: "pack", generatedPack: { id: "p", name: "p", description: "", style: "", kind: "pack", files: [], createdAt: "now" } }).status).toBe("published");
    });

    test("an unpublished outcome strips any result card instead of keeping an old one", () => {
      const merged = mergeAssistantGenerationMetadata(
        { generation: { kind: "sprite-generation", name: "cinder-courier-jog-cycle", category: "characters", fps: 10, assetIds: ["old"] }, other: 1 },
        { outcome: { kind: "generation-outcome", requestId: "req", status: "unpublished" } },
      );
      expect(merged.generation).toBeUndefined();
      expect(merged.other).toBe(1);
      expect(merged.generationOutcome).toEqual({ kind: "generation-outcome", requestId: "req", status: "unpublished" });
    });

    test("attaches results to the request's own message, not the latest completed turn", () => {
      const turn = (id: string, status: Message["status"]): Message => ({
        id, conversationId: "chat", role: "assistant", kind: "text", content: "", status, metadata: {}, createdAt: "now",
      });
      const messages = [turn("older-success", "completed"), turn("current", "failed")];
      expect(requestAssistantMessage(messages, { assistantMessageId: "current" })?.id).toBe("current");
      expect(requestAssistantMessage(messages, { assistantMessageId: "missing" })).toBeUndefined();
      expect(requestAssistantMessage(messages, {})?.id).toBe("older-success");
    });
  });

  test("merges sprite and pack cards onto assistant metadata without dropping either", () => {
    const ordered = [asset("a1", "walk_01"), asset("a2", "walk_02")];
    const spriteCard = spriteCardForOrderedAssets(ordered, 8, "anim-1");
    const merged = mergeAssistantGenerationMetadata(
      { generation: { kind: "sprite-generation", name: "stale", category: "creatures", fps: 1, assetIds: ["old"] } },
      { generation: spriteCard, packGeneration: { kind: "pack-generation", packId: "forest-pack" } },
    );
    expect(merged.generation).toEqual(spriteCard);
    expect(merged.packGeneration).toEqual({ kind: "pack-generation", packId: "forest-pack" });
  });

  test("clears running requests by conversation when request ids change", () => {
    const running = {
      chat: { id: "native-rig-99", conversationId: "chat", workspaceId: "ws", prompt: "", generation: {} as never, knownPackIds: [], startedAt: 0 },
    };
    expect(clearRunningRequest(running, "chat", "provider-uuid")).toEqual(running);
    expect(clearConversationRunningRequest(running, "chat")).toEqual({});
  });

  test("appends streamed tokens onto the running assistant message", () => {
    const messages: Message[] = [
      { id: "u", conversationId: "c", role: "user", kind: "text", content: "hi", status: "completed", metadata: {}, createdAt: "now" },
      { id: "a", conversationId: "c", role: "assistant", kind: "text", content: "Hello", status: "running", metadata: {}, createdAt: "now" },
    ];
    expect(appendAssistantDelta(messages, " world")?.[1].content).toBe("Hello world");
    expect(stripFrameSuffix("hero_01")).toBe("hero");
    expect(chatActivityLines("pack", "pack", normalizeGenerationProfile(null)).length).toBe(2);
  });
});
