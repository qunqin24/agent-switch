import { describe, expect, it } from "vitest";
import { normalizeProviderKey } from "./providerKeyUtils";

describe("normalizeProviderKey", () => {
  it("normalizes only after a completed input value is committed", () => {
    expect(normalizeProviderKey("QunQin-01")).toBe("qunqin-01");
    expect(normalizeProviderKey("群亲-qunqin")).toBe("-qunqin");
  });
});
