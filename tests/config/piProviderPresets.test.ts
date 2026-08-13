import { describe, expect, it } from "vitest";
import { hasProviderIconAsset } from "@/components/ProviderIcon";
import {
  PI_DEFAULT_CONFIG,
  isPiBuiltinProviderKey,
  piApiProtocols,
  piProviderPresets,
} from "@/config/piProviderPresets";

describe("Pi provider configuration", () => {
  it("exposes exactly the API protocols supported by Pi", () => {
    expect(piApiProtocols.map(({ value }) => value)).toEqual([
      "openai-completions",
      "openai-responses",
      "anthropic-messages",
      "google-generative-ai",
    ]);
  });

  it("uses Pi's native provider shape for defaults and presets", () => {
    expect(JSON.parse(PI_DEFAULT_CONFIG)).toEqual({
      baseUrl: "",
      api: "openai-completions",
      apiKey: "",
      models: [],
    });
    expect(piProviderPresets).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: "Anthropic",
          providerKey: "anthropic",
          usesBuiltinCatalog: true,
        }),
        expect.objectContaining({
          name: "Ant Ling",
          providerKey: "ant-ling",
          apiKeyEnvVar: "ANT_LING_API_KEY",
          usesBuiltinCatalog: true,
        }),
        expect.objectContaining({
          name: "OpenAI",
          providerKey: "openai",
          usesBuiltinCatalog: true,
        }),
        expect.objectContaining({
          name: "Google Gemini",
          providerKey: "google",
          usesBuiltinCatalog: true,
        }),
        expect.objectContaining({ name: "DeepSeek" }),
        expect.objectContaining({ name: "OpenRouter" }),
        expect.objectContaining({
          name: "Radius",
          providerKey: "radius",
          apiKeyEnvVar: "RADIUS_API_KEY",
          usesBuiltinCatalog: true,
        }),
        expect.objectContaining({ name: "Ollama" }),
      ]),
    );
    expect(piProviderPresets.length).toBeGreaterThanOrEqual(25);
    expect(isPiBuiltinProviderKey("anthropic")).toBe(true);
    expect(isPiBuiltinProviderKey("ollama")).toBe(false);

    expect(
      piProviderPresets
        .filter((preset) => preset.providerKey.startsWith("xiaomi"))
        .every((preset) => preset.icon === "xiaomimimo"),
    ).toBe(true);
    expect(
      piProviderPresets
        .filter((preset) => preset.providerKey.startsWith("zai"))
        .every((preset) => preset.icon === "zai"),
    ).toBe(true);
    expect(
      piProviderPresets.every(
        (preset) => preset.icon && hasProviderIconAsset(preset.icon),
      ),
    ).toBe(true);

    const anthropic = piProviderPresets.find(
      (preset) => preset.providerKey === "anthropic",
    );
    expect(anthropic?.settingsConfig).toEqual({});
    expect(anthropic?.apiKeyEnvVar).toBe("ANTHROPIC_API_KEY");
    expect(anthropic?.iconColor).toBe("#000000");

    const openai = piProviderPresets.find(
      (preset) => preset.providerKey === "openai",
    );
    expect(openai?.iconColor).toBe("#000000");
  });
});
