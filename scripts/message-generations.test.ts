import { describe, expect, test } from "bun:test";
import { contentWithoutSpriteOutputLinks, inferMessageGeneration, inferMessagePack, reportsGenerationFailure, reportsGenerationWarning, reportsStaticSpriteOnlyIntent } from "../src/lib/message-generations";
import type { Animation, Asset, AssetPack, Message } from "../src/lib/types";

const asset = (id: string, name: string): Asset => ({
  id, name, workspaceId: "workspace", path: `/workspace/assets/creatures/${name}.png`,
  relativePath: `assets/creatures/${name}.png`, category: "creatures", format: "png",
  width: 64, height: 64, fileSize: 100, hasAlpha: true, createdAt: "now",
});
const message = (content: string): Message => ({
  id: "message", conversationId: "chat", role: "assistant", kind: "text", content,
  status: "completed", metadata: {}, createdAt: "now",
});

describe("inferMessageGeneration", () => {
  test("restores an animation card from the generated animation name", () => {
    const assets = [asset("a1", "cave_centipede_crawl_01"), asset("a2", "cave_centipede_crawl_02")];
    const animations: Animation[] = [{
      id: "animation", workspaceId: "workspace", name: "cave_centipede_crawl", fps: 8,
      looping: true, frames: assets.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
    }];
    const result = inferMessageGeneration(message("Created cave_centipede_crawl. Frames: [assets/creatures](assets/creatures)"), assets, animations);
    expect(result?.animationId).toBe("animation");
    expect(result?.assetIds).toEqual(["a1", "a2"]);
  });

  test("restores an animation card when only frame filenames are mentioned", () => {
    const assets = [asset("a1", "rabbit_hop_01"), asset("a2", "rabbit_hop_02")];
    const animations: Animation[] = [{
      id: "hop", workspaceId: "workspace", name: "rabbit_hop", fps: 10,
      looping: true, frames: assets.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
    }];
    const result = inferMessageGeneration(message("Files rabbit_hop_01.png through rabbit_hop_02.png"), assets, animations);
    expect(result?.animationId).toBe("hop");
    expect(result?.fps).toBe(10);
  });

  test("never restores stale metadata for a rejected generation", () => {
    const failed = message("Unable to publish the rabbit hop: it did not pass the final visual acceptance gate.");
    failed.metadata = { generation: { kind: "sprite-generation", name: "old-run", category: "characters", fps: 8, assetIds: ["old"] } };
    expect(reportsGenerationFailure(failed.content)).toBe(true);
    expect(inferMessageGeneration(failed, [asset("old", "old-run")], [])).toBeUndefined();
  });

  test("binds a static sprite from explicit output paths instead of an older animation", () => {
    const shieldAssets = [
      { ...asset("s1", "shield_guard_01"), relativePath: "assets/characters/shield_guard_01.png", path: "/workspace/assets/characters/shield_guard_01.png" },
      { ...asset("s2", "shield_guard_02"), relativePath: "assets/characters/shield_guard_02.png", path: "/workspace/assets/characters/shield_guard_02.png" },
    ];
    const knight = {
      ...asset("k1", "astral_cartographer"),
      relativePath: "assets/characters/astral_cartographer.png",
      path: "/workspace/assets/characters/astral_cartographer.png",
    };
    const animations: Animation[] = [{
      id: "shield", workspaceId: "workspace", name: "shield_guard", fps: 8,
      looping: true, frames: shieldAssets.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
    }];
    const completed = message("Published assets/characters/astral_cartographer.png as the new knight master after shield_guard.");
    completed.metadata = {
      generation: {
        kind: "sprite-generation",
        name: "shield_guard",
        category: "characters",
        fps: 8,
        assetIds: ["s1", "s2"],
        animationId: "shield",
      },
    };
    const result = inferMessageGeneration(completed, [...shieldAssets, knight], animations);
    expect(result?.assetIds).toEqual(["k1"]);
    expect(result?.animationId).toBeUndefined();
    expect(result?.name).toBe("astral_cartographer");
  });

  test("binds every mentioned static asset when no animation frames match", () => {
    const mushroom = asset("m1", "forest_mushroom_red");
    const stump = asset("s1", "forest_stump_mossy");
    const result = inferMessageGeneration(
      message("Saved forest_mushroom_red and forest_stump_mossy as separate props."),
      [mushroom, stump],
      [],
    );
    expect(result?.assetIds).toEqual(["m1", "s1"]);
    expect(result?.animationId).toBeUndefined();
    expect(result?.fps).toBe(1);
  });

  test("does not bind an older animation when the assistant delivered only a rig master", () => {
    const master = {
      ...asset("m1", "master"),
      relativePath: ".sprite-studio/imagegen-sources/stalker-idle/master.png",
      path: "/workspace/.sprite-studio/imagegen-sources/stalker-idle/master.png",
    };
    const walkFrames = Array.from({ length: 32 }, (_, index) => asset(`w${index + 1}`, `stalker_walk_down_${String(index + 1).padStart(2, "0")}`));
    const animations: Animation[] = [{
      id: "walk", workspaceId: "workspace", name: "stalker_walk_down", fps: 12,
      looping: true, frames: walkFrames.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
    }];
    const completed = message(
      "Ho creato il master stalker in .sprite-studio/imagegen-sources/stalker-idle/master.png (32x32, pronto per la riggatura). Nessun frame/rig generato in questo passaggio.",
    );
    expect(reportsStaticSpriteOnlyIntent(completed.content)).toBe(true);
    expect(inferMessageGeneration(completed, [master, ...walkFrames], animations)).toEqual({
      kind: "sprite-generation",
      name: "master",
      category: "creatures",
      fps: 1,
      assetIds: ["m1"],
    });
  });

  test("strips stale animation metadata when the assistant reports a static master only", () => {
    const master = asset("m1", "stalker_master");
    const walkFrames = [asset("w1", "stalker_walk_down_01"), asset("w2", "stalker_walk_down_02")];
    const animations: Animation[] = [{
      id: "walk", workspaceId: "workspace", name: "stalker_walk_down", fps: 12,
      looping: true, frames: walkFrames.map(item => ({ assetId: item.id })), createdAt: "now", updatedAt: "now",
    }];
    const completed = message("Master stalker salvato. Nessun frame generato in questo passaggio.");
    completed.metadata = {
      generation: {
        kind: "sprite-generation",
        name: "stalker_walk_down",
        category: "creatures",
        fps: 12,
        assetIds: ["w1", "w2"],
        animationId: "walk",
      },
    };
    const result = inferMessageGeneration(completed, [master, ...walkFrames], animations);
    expect(result?.animationId).toBeUndefined();
    expect(result?.assetIds).toEqual(["w1", "w2"]);
    expect(result?.fps).toBe(1);
  });

  test("keeps the best artifact when generation completes with a warning", () => {
    const warned = message("Published the best valid lion gallop. GENERATION_WARNING: minor top-down anatomy seam remains.");
    warned.metadata = { generation: { kind: "sprite-generation", name: "lion-gallop", category: "creatures", fps: 10, assetIds: ["lion"] } };
    expect(reportsGenerationWarning(warned.content)).toBe(true);
    expect(reportsGenerationFailure(warned.content)).toBe(false);
    expect(inferMessageGeneration(warned, [asset("lion", "lion-gallop")], [])?.assetIds).toEqual(["lion"]);
  });
});

describe("contentWithoutSpriteOutputLinks", () => {
  test("removes local output links while retaining useful text and web links", () => {
    const content = "Created a crawl.\n\nFrames: [assets/creatures](/private/workspace/assets/creatures)\n\n[Animated preview](/private/workspace/.sprite-studio/previews/crawl.gif)\n\n[Documentation](https://example.com/docs)";
    const cleaned = contentWithoutSpriteOutputLinks(content);
    expect(cleaned).toContain("Created a crawl.");
    expect(cleaned).toContain("[Documentation](https://example.com/docs)");
    expect(cleaned).not.toContain("assets/creatures");
    expect(cleaned).not.toContain("Animated preview");
  });
});

describe("inferMessagePack", () => {
  test("restores one grouped pack component from the pack name", () => {
    const packs: AssetPack[] = [{id:"forest-animals",name:"Forest Animals",description:"A coordinated set",style:"pixel art",kind:"animals",files:["assets/creatures/fox.png"],createdAt:"now"}];
    expect(inferMessagePack(message("Created the Forest Animals pack."),packs)?.pack.id).toBe("forest-animals");
  });
});
