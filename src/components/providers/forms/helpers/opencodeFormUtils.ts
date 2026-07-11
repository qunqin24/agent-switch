import type { OpenCodeModel, OpenCodeProviderConfig } from "@/types";
import type { PricingModelSourceOption } from "../ProviderAdvancedConfig";

// ── Default configs ──────────────────────────────────────────────────

export const CLAUDE_DEFAULT_CONFIG = JSON.stringify({ env: {} }, null, 2);
export const CLAUDE_DESKTOP_DEFAULT_CONFIG = JSON.stringify(
  {
    env: {
      ANTHROPIC_BASE_URL: "",
      ANTHROPIC_AUTH_TOKEN: "",
    },
  },
  null,
  2,
);
export const CODEX_DEFAULT_CONFIG = JSON.stringify(
  { auth: {}, config: "" },
  null,
  2,
);
export const GEMINI_DEFAULT_CONFIG = JSON.stringify(
  {
    env: {
      GOOGLE_GEMINI_BASE_URL: "",
      GEMINI_API_KEY: "",
      GEMINI_MODEL: "gemini-3.5-flash",
    },
  },
  null,
  2,
);

export const OPENCODE_DEFAULT_NPM = "@ai-sdk/openai-compatible";
export const OPENCODE_DEFAULT_CONFIG = JSON.stringify(
  {
    npm: OPENCODE_DEFAULT_NPM,
    options: {
      baseURL: "",
      apiKey: "",
      setCacheKey: true,
    },
    models: {},
  },
  null,
  2,
);
export const OPENCODE_KNOWN_OPTION_KEYS = [
  "baseURL",
  "apiKey",
  "headers",
] as const;

export const OPENCLAW_DEFAULT_CONFIG = JSON.stringify(
  {
    baseUrl: "",
    apiKey: "",
    api: "openai-completions",
    models: [],
  },
  null,
  2,
);

// ── Pure functions ───────────────────────────────────────────────────

export function isKnownOpencodeOptionKey(key: string): boolean {
  return OPENCODE_KNOWN_OPTION_KEYS.includes(
    key as (typeof OPENCODE_KNOWN_OPTION_KEYS)[number],
  );
}

export function parseOpencodeConfig(
  settingsConfig?: Record<string, unknown>,
): OpenCodeProviderConfig {
  const normalize = (
    parsed: Partial<OpenCodeProviderConfig>,
  ): OpenCodeProviderConfig => ({
    npm: parsed.npm || OPENCODE_DEFAULT_NPM,
    options:
      parsed.options && typeof parsed.options === "object"
        ? (parsed.options as OpenCodeProviderConfig["options"])
        : {},
    models:
      parsed.models && typeof parsed.models === "object"
        ? (parsed.models as Record<string, OpenCodeModel>)
        : {},
  });

  try {
    const parsed = JSON.parse(
      settingsConfig ? JSON.stringify(settingsConfig) : OPENCODE_DEFAULT_CONFIG,
    ) as Partial<OpenCodeProviderConfig>;
    return normalize(parsed);
  } catch {
    return {
      npm: OPENCODE_DEFAULT_NPM,
      options: {},
      models: {},
    };
  }
}

export function parseOpencodeConfigStrict(
  settingsConfig?: Record<string, unknown>,
): OpenCodeProviderConfig {
  const parsed = JSON.parse(
    settingsConfig ? JSON.stringify(settingsConfig) : OPENCODE_DEFAULT_CONFIG,
  ) as Partial<OpenCodeProviderConfig>;
  return {
    npm: parsed.npm || OPENCODE_DEFAULT_NPM,
    options:
      parsed.options && typeof parsed.options === "object"
        ? (parsed.options as OpenCodeProviderConfig["options"])
        : {},
    models:
      parsed.models && typeof parsed.models === "object"
        ? (parsed.models as Record<string, OpenCodeModel>)
        : {},
  };
}

export const OPENCODE_KNOWN_MODEL_KEYS = ["name", "limit", "options"] as const;

const DISPLAY_NAME_BRANDS: Record<string, string> = {
  gpt: "GPT",
  deepseek: "DeepSeek",
  glm: "GLM",
};

/**
 * Gives a newly added custom model a readable default without changing its ID.
 * Users can still replace the generated value, and existing names are never
 * overwritten by the form.
 */
export function formatOpenCodeModelDisplayName(modelId: string): string {
  return modelId
    .trim()
    .split(/[-_\s/]+/)
    .filter(Boolean)
    .map((segment) => {
      const normalized = segment.toLowerCase();
      if (DISPLAY_NAME_BRANDS[normalized]) {
        return DISPLAY_NAME_BRANDS[normalized];
      }
      if (/^v\d+(?:\.\d+)*$/i.test(segment)) {
        return `V${segment.slice(1)}`;
      }
      if (/^\d+(?:\.\d+)*$/.test(segment)) {
        return segment;
      }
      return `${normalized[0].toUpperCase()}${normalized.slice(1)}`;
    })
    .join(" ");
}

export const OPENCODE_THINKING_BUDGET_DEFAULT = 16000;
export const OPENCODE_THINKING_LEVELS = ["low", "medium", "high"] as const;

export type OpenCodeThinkingLevel = (typeof OPENCODE_THINKING_LEVELS)[number];

export type OpenCodeThinkingProtocol = "anthropic" | "openai" | "gemini";

export interface OpenCodeThinkingSettings {
  enabled: boolean;
  budgetTokens: number;
  effort: "low" | "medium" | "high" | "xhigh" | "max";
}

export function getOpenCodeThinkingLevel(
  protocol: OpenCodeThinkingProtocol,
  settings: OpenCodeThinkingSettings,
): OpenCodeThinkingLevel {
  if (protocol === "openai") {
    return settings.effort === "low"
      ? "low"
      : settings.effort === "high" || settings.effort === "xhigh"
        ? "high"
        : "medium";
  }
  return settings.budgetTokens <= 8000
    ? "low"
    : settings.budgetTokens >= 32000
      ? "high"
      : "medium";
}

export function getOpenCodeThinkingSettingsForLevel(
  protocol: OpenCodeThinkingProtocol,
  level: OpenCodeThinkingLevel,
): OpenCodeThinkingSettings {
  if (protocol === "openai") {
    return {
      enabled: true,
      budgetTokens: OPENCODE_THINKING_BUDGET_DEFAULT,
      effort: level,
    };
  }
  return {
    enabled: true,
    budgetTokens: level === "low" ? 8000 : level === "high" ? 32000 : 16000,
    effort: "medium",
  };
}

export function buildOpenCodeThinkingVariants(
  protocol: OpenCodeThinkingProtocol,
): Record<string, Record<string, unknown>> {
  return Object.fromEntries(
    OPENCODE_THINKING_LEVELS.map((level) => {
      const settings = getOpenCodeThinkingSettingsForLevel(protocol, level);
      const configured = setOpenCodeThinkingSettings(
        { name: "" },
        protocol,
        settings,
      );
      return [level, configured.options ?? {}];
    }),
  );
}

/**
 * Maps Models.dev's generic `effort` capability to the field required by the
 * selected AI SDK provider. Unlike budget presets, these are model-native
 * values and must never be invented by the UI.
 */
export function buildOpenCodeReasoningEffortVariants(
  protocol: OpenCodeThinkingProtocol,
  efforts: string[],
): Record<string, Record<string, unknown>> {
  return Object.fromEntries(
    efforts.map((effort) => [
      effort,
      protocol === "anthropic"
        ? { effort }
        : protocol === "gemini"
          ? {
              thinkingConfig: {
                includeThoughts: true,
                thinkingLevel: effort,
              },
            }
        : { reasoningEffort: effort },
    ]),
  );
}

/**
 * If the complete generic trio is present, it came from our previous
 * auto-configure implementation. Remove only that exact batch before adding
 * native variants, leaving user-created variants untouched.
 */
export function removeAutomaticOpenCodeThinkingVariants(
  variants: Record<string, unknown>,
  protocol: OpenCodeThinkingProtocol,
): Record<string, unknown> {
  const generated = buildOpenCodeThinkingVariants(protocol);
  const isCompleteGeneratedBatch = Object.entries(generated).every(
    ([level, value]) => JSON.stringify(variants[level]) === JSON.stringify(value),
  );
  if (!isCompleteGeneratedBatch) return variants;

  const next = { ...variants };
  for (const level of Object.keys(generated)) delete next[level];
  return next;
}

export function removeAllAutomaticOpenCodeThinkingVariants(
  variants: Record<string, unknown>,
): Record<string, unknown> {
  return (["anthropic", "openai", "gemini"] as const).reduce(
    (current, protocol) =>
      removeAutomaticOpenCodeThinkingVariants(current, protocol),
    variants,
  );
}

/**
 * Removes variants generated from Models.dev's native effort list. The exact
 * one-property shape lets us keep variants that the user expanded manually.
 */
export function removeAutomaticOpenCodeReasoningEffortVariants(
  variants: Record<string, unknown>,
): Record<string, unknown> {
  const next = { ...variants };
  for (const [name, variant] of Object.entries(variants)) {
    if (!isRecord(variant)) continue;

    const isAnthropicEffort =
      Object.keys(variant).length === 1 && variant.effort === name;
    const isOpenAiEffort =
      Object.keys(variant).length === 1 && variant.reasoningEffort === name;
    const thinkingConfig = variant.thinkingConfig;
    const isGeminiEffort =
      Object.keys(variant).length === 1 &&
      isRecord(thinkingConfig) &&
      Object.keys(thinkingConfig).length === 2 &&
      thinkingConfig.includeThoughts === true &&
      thinkingConfig.thinkingLevel === name;

    if (isAnthropicEffort || isOpenAiEffort || isGeminiEffort) {
      delete next[name];
    }
  }
  return next;
}

export function getOpenCodeReasoningEffort(
  model: OpenCodeModel,
  protocol: OpenCodeThinkingProtocol,
): string | undefined {
  const options = model.options ?? {};
  if (protocol === "gemini") {
    const thinkingConfig = options.thinkingConfig;
    return isRecord(thinkingConfig) && typeof thinkingConfig.thinkingLevel === "string"
      ? thinkingConfig.thinkingLevel
      : undefined;
  }
  const effort =
    protocol === "anthropic" ? options.effort : options.reasoningEffort;
  return typeof effort === "string" ? effort : undefined;
}

export function setOpenCodeReasoningEffort(
  model: OpenCodeModel,
  protocol: OpenCodeThinkingProtocol,
  effort: string,
): OpenCodeModel {
  const options = { ...(model.options ?? {}) };
  delete options.thinking;
  delete options.reasoningEffort;
  delete options.thinkingConfig;
  delete options.effort;

  if (protocol === "anthropic") {
    options.effort = effort;
  } else if (protocol === "gemini") {
    options.thinkingConfig = {
      includeThoughts: true,
      thinkingLevel: effort,
    };
  } else {
    options.reasoningEffort = effort;
  }

  return { ...model, options };
}

/** Remove protocol-owned thinking fields while preserving unrelated SDK options. */
export function clearOpenCodeThinkingSettings(
  model: OpenCodeModel,
): OpenCodeModel {
  const options = { ...(model.options ?? {}) };
  delete options.thinking;
  delete options.reasoningEffort;
  delete options.thinkingConfig;
  delete options.effort;

  return {
    ...model,
    options: Object.keys(options).length > 0 ? options : undefined,
  };
}

export function prepareOpenCodeModelForProtocolChange(
  model: OpenCodeModel,
): OpenCodeModel {
  const cleanedModel = clearOpenCodeThinkingSettings(model);
  const variants = removeAutomaticOpenCodeReasoningEffortVariants(
    removeAllAutomaticOpenCodeThinkingVariants(
      model.variants &&
        typeof model.variants === "object" &&
        !Array.isArray(model.variants)
        ? (model.variants as Record<string, unknown>)
        : {},
    ),
  );

  return {
    ...cleanedModel,
    variants: Object.keys(variants).length > 0 ? variants : undefined,
  };
}

export function supportsAutomaticOpenCodeThinkingConfig(npm: string): boolean {
  return (
    npm === "@ai-sdk/anthropic" ||
    npm === "@ai-sdk/openai" ||
    npm === "@ai-sdk/openai-compatible" ||
    npm === "@ai-sdk/google" ||
    npm === "@ai-sdk/amazon-bedrock"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function positiveInteger(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isInteger(value) && value > 0
    ? value
    : fallback;
}

export function inferOpenCodeThinkingProtocol(
  npm: string,
  model: OpenCodeModel,
): OpenCodeThinkingProtocol {
  const options = model.options ?? {};
  if (isRecord(options.thinkingConfig)) return "gemini";
  if (isRecord(options.thinking)) return "anthropic";
  if (typeof options.reasoningEffort === "string") return "openai";
  if (npm === "@ai-sdk/google") return "gemini";
  return getOpenCodeThinkingProtocolForNpm(npm);
}

/** Uses the selected interface format only, ignoring options from a prior format. */
export function getOpenCodeThinkingProtocolForNpm(
  npm: string,
): OpenCodeThinkingProtocol {
  if (npm === "@ai-sdk/google") return "gemini";
  if (npm === "@ai-sdk/anthropic" || npm === "@ai-sdk/amazon-bedrock") {
    return "anthropic";
  }
  return "openai";
}

export function getOpenCodeThinkingSettings(
  model: OpenCodeModel,
  protocol: OpenCodeThinkingProtocol,
): OpenCodeThinkingSettings {
  const options = model.options ?? {};
  if (protocol === "anthropic") {
    const thinking = isRecord(options.thinking) ? options.thinking : undefined;
    const effort = options.effort;
    return {
      enabled: thinking?.type === "enabled" || typeof effort === "string",
      budgetTokens: positiveInteger(
        thinking?.budgetTokens,
        OPENCODE_THINKING_BUDGET_DEFAULT,
      ),
      effort: "medium",
    };
  }

  if (protocol === "gemini") {
    const thinkingConfig = isRecord(options.thinkingConfig)
      ? options.thinkingConfig
      : undefined;
    const budget = thinkingConfig?.thinkingBudget;
    return {
      enabled:
        thinkingConfig?.includeThoughts === true ||
        (typeof budget === "number" && budget !== 0),
      budgetTokens: positiveInteger(budget, OPENCODE_THINKING_BUDGET_DEFAULT),
      effort: "medium",
    };
  }

  const effort = options.reasoningEffort;
  return {
    enabled:
      effort === "low" ||
      effort === "medium" ||
      effort === "high" ||
      effort === "xhigh",
    budgetTokens: OPENCODE_THINKING_BUDGET_DEFAULT,
    effort:
      effort === "low" || effort === "high" || effort === "xhigh"
        ? effort
        : "medium",
  };
}

export function setOpenCodeThinkingSettings(
  model: OpenCodeModel,
  protocol: OpenCodeThinkingProtocol,
  settings: OpenCodeThinkingSettings,
): OpenCodeModel {
  const options = { ...(model.options ?? {}) };
  delete options.thinking;
  delete options.reasoningEffort;
  delete options.thinkingConfig;
  delete options.effort;

  if (settings.enabled) {
    if (protocol === "anthropic") {
      options.thinking = {
        type: "enabled",
        budgetTokens: settings.budgetTokens,
      };
    } else if (protocol === "gemini") {
      options.thinkingConfig = {
        includeThoughts: true,
        thinkingBudget: settings.budgetTokens,
      };
    } else {
      options.reasoningEffort = settings.effort;
    }
  }

  return {
    ...model,
    options: Object.keys(options).length > 0 ? options : undefined,
  };
}

export function isKnownModelKey(key: string): boolean {
  return OPENCODE_KNOWN_MODEL_KEYS.includes(
    key as (typeof OPENCODE_KNOWN_MODEL_KEYS)[number],
  );
}

export function getModelExtraFields(
  model: OpenCodeModel,
): Record<string, string> {
  const extra: Record<string, string> = {};
  for (const [k, v] of Object.entries(model)) {
    if (!isKnownModelKey(k)) {
      extra[k] = typeof v === "string" ? v : JSON.stringify(v);
    }
  }
  return extra;
}

export function toOpencodeExtraOptions(
  options: OpenCodeProviderConfig["options"],
): Record<string, string> {
  const extra: Record<string, string> = {};
  for (const [k, v] of Object.entries(options || {})) {
    if (!isKnownOpencodeOptionKey(k)) {
      extra[k] = typeof v === "string" ? v : JSON.stringify(v);
    }
  }
  return extra;
}

export { buildOmoProfilePreview } from "@/types/omo";

export const normalizePricingSource = (
  value?: string,
): PricingModelSourceOption =>
  value === "request" || value === "response" ? value : "inherit";
