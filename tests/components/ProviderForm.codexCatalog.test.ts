import { describe, expect, it } from "vitest";
import {
  buildCodexProviderSettings,
  normalizeCodexCatalogModelsForSave,
} from "@/components/providers/forms/ProviderForm";

describe("ProviderForm Codex catalog helpers", () => {
  it("normalizes catalog rows and removes empty or duplicate models", () => {
    expect(
      normalizeCodexCatalogModelsForSave([
        { model: " deepseek-v4-flash ", displayName: " DeepSeek " },
        { model: "deepseek-v4-flash", displayName: "Duplicate" },
        { model: "", displayName: "Empty" },
        { model: "kimi-k2", contextWindow: "128000 tokens" },
      ]),
    ).toEqual([
      { model: "deepseek-v4-flash", displayName: "DeepSeek" },
      { model: "kimi-k2", contextWindow: 128000 },
    ]);
  });

  it("persists model catalogs for native Responses providers", () => {
    const settings = buildCodexProviderSettings({
      authText: '{"OPENAI_API_KEY":"sk-test"}',
      configText: `model = "stale-model"
model_provider = "deepseek"

[model_providers.deepseek]
base_url = "https://api.deepseek.com/"
wire_api = "responses"`,
      category: "cn_official",
      catalogModels: [
        {
          model: " deepseek-v4-flash ",
          displayName: " DeepSeek-V4-Flash ",
          contextWindow: "1048576",
        },
      ],
    });

    expect(settings.modelCatalog).toEqual({
      models: [
        {
          model: "deepseek-v4-flash",
          displayName: "DeepSeek-V4-Flash",
          contextWindow: 1048576,
        },
      ],
    });
    expect(settings.config).toContain('model = "deepseek-v4-flash"');
    expect(settings.config).toContain('wire_api = "responses"');
  });
});
