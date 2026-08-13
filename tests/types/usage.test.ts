import { describe, expect, it } from "vitest";
import {
  CACHE_INCLUSIVE_APP_TYPES,
  getFreshInputTokens,
  KNOWN_APP_TYPES,
} from "@/types/usage";

describe("Pi usage dashboard semantics", () => {
  it("exposes Pi as a dashboard application filter", () => {
    expect(KNOWN_APP_TYPES).toContain("pi");
  });

  it("keeps Pi input separate from cache usage", () => {
    expect(CACHE_INCLUSIVE_APP_TYPES.has("pi")).toBe(false);
    expect(
      getFreshInputTokens({
        appType: "pi",
        inputTokens: 100,
        cacheReadTokens: 40,
      }),
    ).toBe(100);
  });
});
