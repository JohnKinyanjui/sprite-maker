import { describe, expect, test } from "bun:test";
import { CREATABLE_WORKTREE_KINDS, inferWorktreeKind } from "../src/lib/worktree-kinds";

describe("worktree kind selection", () => {
  test("exposes every user-creatable backend kind", () => {
    expect(CREATABLE_WORKTREE_KINDS.map(option => option.value)).toEqual([
      "character", "creature", "object", "environment", "tileset", "animation", "vfx", "ui",
    ]);
  });

  test("suggests tileset for terrain names without hiding the explicit choice", () => {
    expect(inferWorktreeKind("Lesser Antilles Terrain")).toBe("tileset");
    expect(inferWorktreeKind("Volcanic Tiles")).toBe("tileset");
  });
});
