import { describe, expect, it } from "vitest";
import { claudeDesktopProviderPresets } from "@/config/claudeDesktopProviderPresets";
import { providerPresets } from "@/config/claudeProviderPresets";
import { codexProviderPresets } from "@/config/codexProviderPresets";
import { geminiProviderPresets } from "@/config/geminiProviderPresets";
import { hermesProviderPresets } from "@/config/hermesProviderPresets";
import { openclawProviderPresets } from "@/config/openclawProviderPresets";
import { opencodeProviderPresets } from "@/config/opencodeProviderPresets";
import { piProviderPresets } from "@/config/piProviderPresets";

const blockedRelayPresets = [
  "Shengsuanyun",
  "PatewayAI",
  "CCSub",
  "Unity2.ai",
  "CherryIN",
  "DMXAPI",
  "PackyCode",
  "APIKEY.FUN",
  "APINebula",
  "SudoCode",
  "ClaudeAPI",
  "ClaudeCN",
  "RunAPI",
  "RelaxyCode",
  "Cubence",
  "AIGoCode",
  "RightCode",
  "AICodeMirror",
  "CrazyRouter",
  "SSSAiCode",
  "Compshare",
  "Compshare Coding Plan",
  "Micu",
  "CTok.ai",
  "E-FlowCode",
  "PIPELLM",
  "OpenAI Compatible",
  "AtlasCloud",
] as const;

const presetGroups = {
  claude: providerPresets,
  claudeDesktop: claudeDesktopProviderPresets,
  codex: codexProviderPresets,
  gemini: geminiProviderPresets,
  hermes: hermesProviderPresets,
  openclaw: openclawProviderPresets,
  opencode: opencodeProviderPresets,
  pi: piProviderPresets,
};

const expectedRecognizedPresets: Record<keyof typeof presetGroups, string[]> = {
  claude: ["Claude Official", "Kimi", "MiniMax", "SiliconFlow", "OpenRouter"],
  claudeDesktop: [
    "Claude Desktop Official",
    "Kimi",
    "MiniMax",
    "SiliconFlow",
    "OpenRouter",
  ],
  codex: [
    "OpenAI Official",
    "Azure OpenAI",
    "Kimi",
    "MiniMax",
    "SiliconFlow",
    "OpenRouter",
  ],
  gemini: ["Google Official", "OpenRouter", "TheRouter", "自定义"],
  hermes: [
    "OpenRouter",
    "DeepSeek",
    "Together AI",
    "Nous Research",
    "Kimi",
    "MiniMax",
    "SiliconFlow",
  ],
  openclaw: [
    "Qwen Coder",
    "Kimi K2.7 Code",
    "MiniMax",
    "SiliconFlow",
    "OpenRouter",
    "AWS Bedrock",
  ],
  opencode: [
    "Kimi K2.7 Code",
    "MiniMax",
    "OpenRouter",
    "TheRouter",
    "AWS Bedrock",
  ],
  pi: [
    "Anthropic",
    "OpenAI",
    "Google Gemini",
    "DeepSeek",
    "Mistral AI",
    "Groq",
    "OpenRouter",
    "Kimi For Coding",
    "MiniMax",
    "Ollama",
  ],
};

describe("provider preset curation", () => {
  it.each(Object.entries(presetGroups))(
    "%s excludes unverified relay and mirror presets",
    (_groupName, presets) => {
      const names = presets.map((preset) => preset.name);

      blockedRelayPresets.forEach((name) => {
        expect(names, `${name} should not be listed`).not.toContain(name);
      });
    },
  );

  it.each(Object.entries(presetGroups))(
    "%s keeps recognized model and inference platforms",
    (groupName, presets) => {
      const names = presets.map((preset) => preset.name);
      const expected =
        expectedRecognizedPresets[groupName as keyof typeof presetGroups];

      expect(names).toEqual(expect.arrayContaining(expected));
    },
  );
});
