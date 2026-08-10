import { describe, expect, test } from "bun:test";
import { classifyMarkdownLink, pathIsInsideWorkspace } from "../src/lib/markdown-links";

describe("classifyMarkdownLink", () => {
  test("routes web URLs outside the desktop webview", () => {
    expect(classifyMarkdownLink("https://example.com/release")).toEqual({ kind: "external", url: "https://example.com/release" });
    expect(classifyMarkdownLink("//example.com/release")).toEqual({ kind: "external", url: "https://example.com/release" });
  });

  test("treats relative, absolute, encoded, and Windows paths as workspace links", () => {
    expect(classifyMarkdownLink("assets/props")).toEqual({ kind: "workspace", path: "assets/props" });
    expect(classifyMarkdownLink("assets/My%20Sprite.png")).toEqual({ kind: "workspace", path: "assets/My Sprite.png" });
    expect(classifyMarkdownLink("/Users/jakes/Game Projects/sprite-maker/assets/props")).toEqual({ kind: "workspace", path: "/Users/jakes/Game Projects/sprite-maker/assets/props" });
    expect(classifyMarkdownLink("C:\\Games\\Sprites\\hero.png")).toEqual({ kind: "workspace", path: "C:\\Games\\Sprites\\hero.png" });
  });

  test("accepts file URLs but blocks executable and unknown protocols", () => {
    expect(classifyMarkdownLink("file:///Users/jakes/Game%20Projects/sprite-maker/assets/hero.png")).toEqual({ kind: "workspace", path: "/Users/jakes/Game Projects/sprite-maker/assets/hero.png" });
    expect(classifyMarkdownLink("javascript:alert(1)").kind).toBe("blocked");
    expect(classifyMarkdownLink("data:text/html,broken").kind).toBe("blocked");
    expect(classifyMarkdownLink("sprite-studio:asset/123").kind).toBe("blocked");
  });

  test("ignores empty links and page fragments", () => {
    expect(classifyMarkdownLink("")).toEqual({ kind: "ignore" });
    expect(classifyMarkdownLink("#animation")).toEqual({ kind: "ignore" });
  });
});

describe("pathIsInsideWorkspace", () => {
  test("accepts the workspace and descendants without prefix collisions", () => {
    expect(pathIsInsideWorkspace("/work/game", "/work/game", "/")).toBe(true);
    expect(pathIsInsideWorkspace("/work/game", "/work/game/assets/hero.png", "/")).toBe(true);
    expect(pathIsInsideWorkspace("/work/game", "/work/game-backup/hero.png", "/")).toBe(false);
    expect(pathIsInsideWorkspace("/work/game", "/work/other/hero.png", "/")).toBe(false);
  });

  test("compares Windows paths case-insensitively", () => {
    expect(pathIsInsideWorkspace("C:\\Games\\Sprite Maker", "c:\\games\\sprite maker\\assets\\hero.png", "\\")).toBe(true);
    expect(pathIsInsideWorkspace("C:\\Games\\Sprite Maker", "C:\\Games\\Sprite Maker Old\\hero.png", "\\")).toBe(false);
  });
});
