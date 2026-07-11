import { describe, expect, it } from "vitest";
import { namespaceSvgIds } from "./namespaceSvgIds";

describe("namespaceSvgIds", () => {
  it("scopes SVG definitions and their gradient references", () => {
    const result = namespaceSvgIds(
      '<svg><defs><linearGradient id="brand-gradient" /></defs><path fill="url(#brand-gradient)" /></svg>',
      "icon-1",
    );

    expect(result).toContain('id="icon-1-brand-gradient"');
    expect(result).toContain('fill="url(#icon-1-brand-gradient)"');
    expect(result).not.toContain('url(#brand-gradient)');
  });
});
