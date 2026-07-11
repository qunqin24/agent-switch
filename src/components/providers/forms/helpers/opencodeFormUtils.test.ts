import { describe, expect, it } from "vitest";
import {
  buildOpenCodeThinkingVariants,
  buildOpenCodeReasoningEffortVariants,
  clearOpenCodeThinkingSettings,
  formatOpenCodeModelDisplayName,
  getOpenCodeThinkingProtocolForNpm,
  getOpenCodeReasoningEffort,
  getOpenCodeThinkingSettings,
  getOpenCodeThinkingSettingsForLevel,
  inferOpenCodeThinkingProtocol,
  prepareOpenCodeModelForProtocolChange,
  removeAutomaticOpenCodeThinkingVariants,
  removeAutomaticOpenCodeReasoningEffortVariants,
  removeAllAutomaticOpenCodeThinkingVariants,
  setOpenCodeThinkingSettings,
  setOpenCodeReasoningEffort,
  supportsAutomaticOpenCodeThinkingConfig,
} from "./opencodeFormUtils";

describe("OpenCode thinking settings", () => {
  it("uses Anthropic thinking for Anthropic-compatible providers", () => {
    const initial = { name: "GLM 5.2" };
    const protocol = inferOpenCodeThinkingProtocol(
      "@ai-sdk/anthropic",
      initial,
    );
    const result = setOpenCodeThinkingSettings(initial, protocol, {
      enabled: true,
      budgetTokens: 16000,
      effort: "medium",
    });

    expect(protocol).toBe("anthropic");
    expect(result.options).toEqual({
      thinking: { type: "enabled", budgetTokens: 16000 },
    });
  });

  it("uses reasoning effort for OpenAI-compatible providers", () => {
    const initial = { name: "GPT 5" };
    const result = setOpenCodeThinkingSettings(initial, "openai", {
      enabled: true,
      budgetTokens: 16000,
      effort: "high",
    });

    expect(result.options).toEqual({ reasoningEffort: "high" });
    expect(getOpenCodeThinkingSettings(result, "openai")).toMatchObject({
      enabled: true,
      effort: "high",
    });
  });

  it("keeps unrelated model SDK options when disabling thinking", () => {
    const result = setOpenCodeThinkingSettings(
      {
        name: "Claude",
        options: {
          provider: { order: ["custom"] },
          thinking: { type: "enabled", budgetTokens: 16000 },
        },
      },
      "anthropic",
      { enabled: false, budgetTokens: 16000, effort: "medium" },
    );

    expect(result.options).toEqual({ provider: { order: ["custom"] } });
  });

  it("generates low, medium, and high variants for Anthropic thinking", () => {
    expect(buildOpenCodeThinkingVariants("anthropic")).toEqual({
      low: { thinking: { type: "enabled", budgetTokens: 8000 } },
      medium: { thinking: { type: "enabled", budgetTokens: 16000 } },
      high: { thinking: { type: "enabled", budgetTokens: 32000 } },
    });
  });

  it("maps OpenAI thinking levels to reasoning effort", () => {
    expect(getOpenCodeThinkingSettingsForLevel("openai", "high")).toMatchObject(
      {
        enabled: true,
        effort: "high",
      },
    );
  });

  it("generates only native effort variants for an Anthropic-compatible model", () => {
    expect(
      buildOpenCodeReasoningEffortVariants("anthropic", ["high", "max"]),
    ).toEqual({
      high: { effort: "high" },
      max: { effort: "max" },
    });
    expect(
      setOpenCodeReasoningEffort({ name: "GLM 5.2" }, "anthropic", "high"),
    ).toMatchObject({ options: { effort: "high" } });
  });

  it("uses Google's native thinking configuration for Gemini efforts", () => {
    expect(buildOpenCodeReasoningEffortVariants("gemini", ["low", "high"])).toEqual({
      low: {
        thinkingConfig: { includeThoughts: true, thinkingLevel: "low" },
      },
      high: {
        thinkingConfig: { includeThoughts: true, thinkingLevel: "high" },
      },
    });
    expect(
      setOpenCodeReasoningEffort(
        { name: "Gemini 3.5 Flash" },
        "gemini",
        "high",
      ),
    ).toMatchObject({
      options: {
        thinkingConfig: { includeThoughts: true, thinkingLevel: "high" },
      },
    });
    expect(
      getOpenCodeReasoningEffort(
        {
          name: "Gemini 3.5 Flash",
          options: {
            thinkingConfig: { includeThoughts: true, thinkingLevel: "high" },
          },
        },
        "gemini",
      ),
    ).toBe("high");
  });

  it("uses the new interface format instead of a prior model configuration", () => {
    expect(getOpenCodeThinkingProtocolForNpm("@ai-sdk/google")).toBe("gemini");
    expect(getOpenCodeThinkingProtocolForNpm("@ai-sdk/anthropic")).toBe(
      "anthropic",
    );
    expect(getOpenCodeThinkingProtocolForNpm("@ai-sdk/openai")).toBe("openai");
  });

  it("removes only the complete legacy auto-generated thinking batch", () => {
    expect(
      removeAutomaticOpenCodeThinkingVariants(
        {
          ...buildOpenCodeThinkingVariants("anthropic"),
          custom: { effort: "custom" },
        },
        "anthropic",
      ),
    ).toEqual({ custom: { effort: "custom" } });
  });

  it("removes native effort variants from a prior interface but keeps custom variants", () => {
    expect(
      removeAutomaticOpenCodeReasoningEffortVariants({
        high: { effort: "high" },
        max: { effort: "max" },
        custom: { effort: "high", label: "Keep me" },
      }),
    ).toEqual({ custom: { effort: "high", label: "Keep me" } });
  });

  it("clears prior protocol fields without removing unrelated SDK options", () => {
    expect(
      clearOpenCodeThinkingSettings({
        name: "Custom",
        options: {
          thinking: { type: "enabled", budgetTokens: 16000 },
          headers: { "x-test": "1" },
        },
      }),
    ).toEqual({
      name: "Custom",
      options: { headers: { "x-test": "1" } },
    });
  });

  it("removes legacy generated variants for every prior interface", () => {
    expect(
      removeAllAutomaticOpenCodeThinkingVariants(
        buildOpenCodeThinkingVariants("anthropic"),
      ),
    ).toEqual({});
    expect(
      removeAllAutomaticOpenCodeThinkingVariants(
        buildOpenCodeThinkingVariants("gemini"),
      ),
    ).toEqual({});
  });

  it("supports automatic reconfiguration for Amazon Bedrock", () => {
    expect(
      supportsAutomaticOpenCodeThinkingConfig("@ai-sdk/amazon-bedrock"),
    ).toBe(true);
  });

  it("prepares unknown models for a protocol change without losing custom variants", () => {
    expect(
      prepareOpenCodeModelForProtocolChange({
        name: "Unknown",
        options: {
          reasoningEffort: "high",
          headers: { "x-test": "1" },
        },
        variants: {
          high: { reasoningEffort: "high" },
          custom: { temperature: 0.2 },
        },
      }),
    ).toEqual({
      name: "Unknown",
      options: { headers: { "x-test": "1" } },
      variants: { custom: { temperature: 0.2 } },
    });
  });
});

describe("OpenCode model display names", () => {
  it("formats GPT model IDs with an uppercase brand and title-cased suffixes", () => {
    expect(formatOpenCodeModelDisplayName("gpt-5.6-mini")).toBe(
      "GPT 5.6 Mini",
    );
  });

  it("formats DeepSeek model IDs with the brand's expected casing", () => {
    expect(formatOpenCodeModelDisplayName("deepseek-v4-flash")).toBe(
      "DeepSeek V4 Flash",
    );
  });

  it("uses normal title casing for other model families", () => {
    expect(formatOpenCodeModelDisplayName("gemini-3.5-flash")).toBe(
      "Gemini 3.5 Flash",
    );
  });
});
