import { describe, expect, it } from "vitest";
import {
  backfillCodexCatalogContextWindow,
  formatModelsDevModalities,
  getCodexReasoningLevelOptions,
  getModelsDevCapabilityFlags,
  getModelsDevContextWindow,
  isCodexContextWindowEmpty,
  selectCodexRowsNeedingContextWindow,
} from "./codexCatalogUtils";

const row = (
  overrides: Partial<{
    rowId: string;
    model: string;
    displayName: string;
    contextWindow: string | number;
  }> = {},
) => ({
  rowId: "row-1",
  model: "glm-5.2",
  displayName: "GLM 5.2",
  ...overrides,
});

describe("getModelsDevContextWindow", () => {
  it("reads limit.context from Models.dev metadata", () => {
    expect(getModelsDevContextWindow({ limit: { context: 204800 } })).toBe(
      204800,
    );
  });

  it("returns undefined when metadata is missing or has no usable context", () => {
    expect(getModelsDevContextWindow(null)).toBeUndefined();
    expect(getModelsDevContextWindow(undefined)).toBeUndefined();
    expect(getModelsDevContextWindow({})).toBeUndefined();
    expect(getModelsDevContextWindow({ limit: {} })).toBeUndefined();
    expect(
      getModelsDevContextWindow({ limit: { context: 0 } }),
    ).toBeUndefined();
    expect(
      getModelsDevContextWindow({ limit: { context: -1 } }),
    ).toBeUndefined();
  });
});

describe("isCodexContextWindowEmpty", () => {
  it("treats blank values as empty and numbers as filled", () => {
    expect(isCodexContextWindowEmpty(undefined)).toBe(true);
    expect(isCodexContextWindowEmpty(null)).toBe(true);
    expect(isCodexContextWindowEmpty("")).toBe(true);
    expect(isCodexContextWindowEmpty("  ")).toBe(true);
    expect(isCodexContextWindowEmpty(0)).toBe(false);
    expect(isCodexContextWindowEmpty("128000")).toBe(false);
  });
});

describe("backfillCodexCatalogContextWindow", () => {
  it("fills an empty context window from Models.dev metadata", () => {
    const rows = [row()];
    const next = backfillCodexCatalogContextWindow(rows, {
      rowId: "row-1",
      model: "glm-5.2",
      contextWindow: getModelsDevContextWindow({ limit: { context: 204800 } }),
    });

    expect(next).not.toBe(rows);
    expect(next[0].contextWindow).toBe("204800");
    // 原数组不被修改
    expect(rows[0]).not.toHaveProperty("contextWindow");
  });

  it("never overwrites a value the user already filled in", () => {
    const rows = [row({ contextWindow: "65536" })];
    const next = backfillCodexCatalogContextWindow(rows, {
      rowId: "row-1",
      model: "glm-5.2",
      contextWindow: 204800,
    });

    expect(next).toBe(rows);
    expect(next[0].contextWindow).toBe("65536");
  });

  it("leaves the row empty when Models.dev has no context length", () => {
    const rows = [row()];
    const next = backfillCodexCatalogContextWindow(rows, {
      rowId: "row-1",
      model: "glm-5.2",
      contextWindow: getModelsDevContextWindow(null),
    });

    expect(next).toBe(rows);
    expect(next[0].contextWindow).toBeUndefined();
  });

  it("ignores a stale lookup whose row was removed or retyped", () => {
    const rows = [row({ model: "glm-5.3" })];

    expect(
      backfillCodexCatalogContextWindow(rows, {
        rowId: "row-1",
        model: "glm-5.2",
        contextWindow: 204800,
      }),
    ).toBe(rows);

    expect(
      backfillCodexCatalogContextWindow(rows, {
        rowId: "removed-row",
        model: "glm-5.3",
        contextWindow: 204800,
      }),
    ).toBe(rows);
  });

  it("only touches the targeted row", () => {
    const rows = [row(), row({ rowId: "row-2", model: "kimi-k2" })];
    const next = backfillCodexCatalogContextWindow(rows, {
      rowId: "row-2",
      model: "kimi-k2",
      contextWindow: 262144,
    });

    expect(next[0]).toBe(rows[0]);
    expect(next[1].contextWindow).toBe("262144");
  });
});

describe("selectCodexRowsNeedingContextWindow", () => {
  it("keeps only rows with a model id and an empty context window", () => {
    const rows = [
      row(),
      row({ rowId: "row-2", contextWindow: "128000" }),
      row({ rowId: "row-3", model: "  " }),
      row({ rowId: "row-4", model: "kimi-k2", contextWindow: "  " }),
    ];

    expect(
      selectCodexRowsNeedingContextWindow(rows).map((r) => r.rowId),
    ).toEqual(["row-1", "row-4"]);
  });
});

describe("Models.dev panel selectors", () => {
  it("lists only the capability flags a model declares", () => {
    expect(
      getModelsDevCapabilityFlags({
        attachment: false,
        reasoning: true,
        tool_call: true,
        structured_output: undefined,
        temperature: true,
      }),
    ).toEqual(["reasoning", "tool_call", "temperature"]);
    expect(getModelsDevCapabilityFlags(null)).toEqual([]);
  });

  it("formats modalities like the OpenCode panel", () => {
    expect(
      formatModelsDevModalities({
        modalities: { input: ["text", "image"], output: ["text"] },
      }),
    ).toBe("text/image -> text");
    expect(formatModelsDevModalities({ modalities: {} })).toBeUndefined();
    expect(formatModelsDevModalities(null)).toBeUndefined();
  });

  it("offers Models.dev efforts, falling back to low/medium/high", () => {
    expect(
      getCodexReasoningLevelOptions({
        reasoning_options: [{ type: "effort", values: ["low", "high", "max"] }],
      }),
    ).toEqual(["low", "high", "max"]);
    expect(getCodexReasoningLevelOptions(null)).toEqual([
      "low",
      "medium",
      "high",
    ]);
  });

  it("keeps a stored level that Models.dev does not advertise", () => {
    expect(
      getCodexReasoningLevelOptions(
        { reasoning_options: [{ type: "effort", values: ["low", "high"] }] },
        "xhigh",
      ),
    ).toEqual(["low", "high", "xhigh"]);
    // 已在列表中的值不会重复
    expect(getCodexReasoningLevelOptions(null, "high")).toEqual([
      "low",
      "medium",
      "high",
    ]);
  });
});
