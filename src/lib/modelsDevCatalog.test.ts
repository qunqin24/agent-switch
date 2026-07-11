import { describe, expect, it } from "vitest";
import {
  findModelsDevCatalogModel,
  findOfficialModelsDevProviderId,
  getModelsDevCapabilityDeclarations,
  getModelsDevReasoningEfforts,
} from "./modelsDevCatalog";

const catalog = {
  models: {
    "zhipuai/glm-5.2": {
      id: "zhipuai/glm-5.2",
      name: "GLM-5.2",
      reasoning: true,
      reasoning_options: [{ type: "effort", values: ["high", "max"] }],
    },
    "openai/gpt-5": {
      id: "openai/gpt-5",
      name: "GPT-5",
      reasoning: true,
    },
  },
};

describe("Models.dev catalog matching", () => {
  it("matches a custom provider model ID against a canonical Models.dev ID", () => {
    expect(findModelsDevCatalogModel(catalog, "glm-5.2")).toMatchObject({
      id: "zhipuai/glm-5.2",
      reasoning: true,
    });
  });

  it("falls back to an unambiguous display-name match", () => {
    expect(
      findModelsDevCatalogModel(catalog, "relay-glm", "GLM-5.2"),
    ).toMatchObject({ id: "zhipuai/glm-5.2" });
  });

  it("does not report a match for unrelated models", () => {
    expect(findModelsDevCatalogModel(catalog, "not-a-model")).toBeUndefined();
  });

  it("does not guess when a display name matches multiple catalog entries", () => {
    expect(
      findModelsDevCatalogModel(
        {
          models: {
            "provider-a/example": { name: "Example" },
            "provider-b/example": { name: "Example" },
          },
        },
        "custom-model",
        "Example",
      ),
    ).toBeUndefined();
  });

  it("exposes only the native reasoning effort values published by the API", () => {
    const model = findModelsDevCatalogModel(catalog, "glm-5.2");
    expect(getModelsDevReasoningEfforts(model)).toEqual(["high", "max"]);
  });

  it("matches a model from the provider-indexed api.json response", () => {
    expect(
      findModelsDevCatalogModel(
        {
          zhipuai: {
            models: {
              "glm-5.2": {
                id: "glm-5.2",
                name: "GLM-5.2",
                reasoning_options: [
                  { type: "effort", values: ["high", "max"] },
                ],
              },
            },
          },
        },
        "glm-5.2",
      ),
    ).toMatchObject({ id: "glm-5.2", name: "GLM-5.2" });
  });

  it("uses the canonical index to prefer an official provider over a reseller", () => {
    const officialProvider = findOfficialModelsDevProviderId(
      {
        "zhipuai/glm-5.2": { id: "zhipuai/glm-5.2", name: "GLM-5.2" },
      },
      "glm-5.2",
    );
    const model = findModelsDevCatalogModel(
      {
        "alibaba-cn": {
          models: {
            "glm-5.2": {
              id: "glm-5.2",
              reasoning: true,
              reasoning_options: [],
              limit: { context: 1_000_000, output: 128_000 },
            },
          },
        },
        zhipuai: {
          models: {
            "glm-5.2": {
              id: "glm-5.2",
              reasoning: true,
              reasoning_options: [{ type: "effort", values: ["high", "max"] }],
              limit: { context: 1_000_000, output: 131_072 },
            },
          },
        },
      },
      "glm-5.2",
      undefined,
      officialProvider,
    );

    expect(officialProvider).toBe("zhipuai");
    expect(getModelsDevReasoningEfforts(model)).toEqual(["high", "max"]);
    expect(model?.limit?.output).toBe(131_072);
  });

  it("does not select an arbitrary reseller when no official provider is known", () => {
    expect(
      findModelsDevCatalogModel(
        {
          "provider-a": { models: { example: { id: "example" } } },
          "provider-b": { models: { example: { id: "example" } } },
        },
        "example",
      ),
    ).toBeUndefined();
  });

  it("copies only declared model capabilities", () => {
    expect(
      getModelsDevCapabilityDeclarations({
        attachment: false,
        reasoning: true,
        tool_call: true,
        structured_output: true,
        temperature: true,
        modalities: { input: ["text"], output: ["text"] },
      }),
    ).toEqual({
      attachment: false,
      reasoning: true,
      tool_call: true,
      structured_output: true,
      temperature: true,
      modalities: { input: ["text"], output: ["text"] },
    });
  });
});
