import { describe, expect, test } from "bun:test";
import { GENERATION_PRESETS, normalizeGenerationProfile } from "../src/lib/generation-profiles";

describe("generation profile defaults", () => {
  test("uses a 128 by 128 mid-quality canvas by default", () => {
    const profile = normalizeGenerationProfile(null);
    expect(profile.quality).toBe("mid");
    expect([profile.width, profile.height]).toEqual([128, 128]);
    expect(profile.profileVersion).toBe(8);
  });

  test("upgrades saved preset profiles while preserving custom dimensions", () => {
    const legacy = normalizeGenerationProfile({ profileVersion: 7, quality: "mid", width: 64, height: 64 });
    expect([legacy.width, legacy.height]).toEqual([128, 128]);

    const custom = normalizeGenerationProfile({ profileVersion: 7, quality: "custom", width: 96, height: 80 });
    expect([custom.width, custom.height]).toEqual([96, 80]);
    expect(GENERATION_PRESETS.mid.width).toBe(128);
  });
});
