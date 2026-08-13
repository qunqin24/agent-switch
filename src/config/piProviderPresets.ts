import type {
  OpenClawModel,
  OpenClawProviderConfig,
  ProviderCategory,
} from "@/types";
import type { PresetTheme } from "./claudeProviderPresets";

export type PiModel = OpenClawModel;
export type PiProviderSettingsConfig = OpenClawProviderConfig;

export interface PiProviderPreset {
  name: string;
  nameKey?: string;
  providerKey: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: PiProviderSettingsConfig;
  usesBuiltinCatalog?: boolean;
  apiKeyEnvVar?: string;
  category?: ProviderCategory;
  theme?: PresetTheme;
  icon?: string;
  iconColor?: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;
}

export const piApiProtocols = [
  { value: "openai-completions", label: "OpenAI Completions" },
  { value: "openai-responses", label: "OpenAI Responses" },
  { value: "anthropic-messages", label: "Anthropic Messages" },
  { value: "google-generative-ai", label: "Google Generative AI" },
] as const;

export const PI_DEFAULT_CONFIG = JSON.stringify(
  {
    baseUrl: "",
    api: "openai-completions",
    apiKey: "",
    models: [],
  },
  null,
  2,
);

interface PiBuiltinPresetOptions {
  name: string;
  providerKey: string;
  envVar: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  category?: ProviderCategory;
  icon?: string;
  iconColor?: string;
}

function builtinPreset({
  name,
  providerKey,
  envVar,
  websiteUrl,
  apiKeyUrl,
  category = "official",
  icon,
  iconColor,
}: PiBuiltinPresetOptions): PiProviderPreset {
  return {
    name,
    providerKey,
    websiteUrl,
    apiKeyUrl,
    settingsConfig: {},
    usesBuiltinCatalog: true,
    apiKeyEnvVar: envVar,
    category,
    icon,
    iconColor,
  };
}

/**
 * Pi providers with built-in model catalogs and API-key authentication.
 *
 * These presets intentionally only override `apiKey`: Pi keeps its built-in
 * endpoint, protocol, model metadata, and dynamically refreshed catalog.
 * Cloud providers that require additional account/project/region settings and
 * OAuth-only subscriptions remain configured through Pi's `/login` or shell.
 *
 * @see https://pi.dev/docs/latest/providers
 */
export const piProviderPresets: PiProviderPreset[] = [
  builtinPreset({
    name: "Ant Ling",
    providerKey: "ant-ling",
    envVar: "ANT_LING_API_KEY",
    websiteUrl: "https://developer.ant-ling.com",
    category: "cn_official",
    icon: "antgroup",
  }),
  builtinPreset({
    name: "Anthropic",
    providerKey: "anthropic",
    envVar: "ANTHROPIC_API_KEY",
    websiteUrl: "https://www.anthropic.com",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
    icon: "anthropic",
    iconColor: "#000000",
  }),
  builtinPreset({
    name: "OpenAI",
    providerKey: "openai",
    envVar: "OPENAI_API_KEY",
    websiteUrl: "https://openai.com",
    apiKeyUrl: "https://platform.openai.com/api-keys",
    icon: "openai",
    iconColor: "#000000",
  }),
  builtinPreset({
    name: "Google Gemini",
    providerKey: "google",
    envVar: "GEMINI_API_KEY",
    websiteUrl: "https://ai.google.dev",
    apiKeyUrl: "https://aistudio.google.com/apikey",
    icon: "gemini",
    iconColor: "#4285F4",
  }),
  builtinPreset({
    name: "DeepSeek",
    providerKey: "deepseek",
    envVar: "DEEPSEEK_API_KEY",
    websiteUrl: "https://www.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    category: "cn_official",
    icon: "deepseek",
    iconColor: "#4D6BFE",
  }),
  builtinPreset({
    name: "NVIDIA NIM",
    providerKey: "nvidia",
    envVar: "NVIDIA_API_KEY",
    websiteUrl: "https://build.nvidia.com",
    icon: "nvidia",
    iconColor: "#76B900",
  }),
  builtinPreset({
    name: "Mistral AI",
    providerKey: "mistral",
    envVar: "MISTRAL_API_KEY",
    websiteUrl: "https://mistral.ai",
    apiKeyUrl: "https://console.mistral.ai/api-keys",
    icon: "mistral",
    iconColor: "#F7D046",
  }),
  builtinPreset({
    name: "Groq",
    providerKey: "groq",
    envVar: "GROQ_API_KEY",
    websiteUrl: "https://groq.com",
    apiKeyUrl: "https://console.groq.com/keys",
    icon: "groq",
    iconColor: "#F55036",
  }),
  builtinPreset({
    name: "Cerebras",
    providerKey: "cerebras",
    envVar: "CEREBRAS_API_KEY",
    websiteUrl: "https://www.cerebras.ai",
    apiKeyUrl: "https://cloud.cerebras.ai",
    icon: "cerebras",
    iconColor: "#F15A24",
  }),
  builtinPreset({
    name: "xAI",
    providerKey: "xai",
    envVar: "XAI_API_KEY",
    websiteUrl: "https://x.ai",
    apiKeyUrl: "https://console.x.ai",
    icon: "xai",
    iconColor: "#111111",
  }),
  builtinPreset({
    name: "OpenRouter",
    providerKey: "openrouter",
    envVar: "OPENROUTER_API_KEY",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6467F2",
  }),
  builtinPreset({
    name: "Vercel AI Gateway",
    providerKey: "vercel-ai-gateway",
    envVar: "AI_GATEWAY_API_KEY",
    websiteUrl: "https://vercel.com/ai-gateway",
    category: "aggregator",
    icon: "vercel",
    iconColor: "#111111",
  }),
  builtinPreset({
    name: "Z.AI Coding Plan",
    providerKey: "zai",
    envVar: "ZAI_API_KEY",
    websiteUrl: "https://z.ai",
    category: "cn_official",
    icon: "zai",
    iconColor: "#246BFD",
  }),
  builtinPreset({
    name: "Z.AI Coding Plan (China)",
    providerKey: "zai-coding-cn",
    envVar: "ZAI_CODING_CN_API_KEY",
    websiteUrl: "https://bigmodel.cn",
    category: "cn_official",
    icon: "zai",
    iconColor: "#246BFD",
  }),
  builtinPreset({
    name: "OpenCode Zen",
    providerKey: "opencode",
    envVar: "OPENCODE_API_KEY",
    websiteUrl: "https://opencode.ai/zen",
    icon: "opencode",
  }),
  builtinPreset({
    name: "OpenCode Go",
    providerKey: "opencode-go",
    envVar: "OPENCODE_API_KEY",
    websiteUrl: "https://opencode.ai",
    icon: "opencode",
  }),
  builtinPreset({
    name: "Radius",
    providerKey: "radius",
    envVar: "RADIUS_API_KEY",
    websiteUrl: "https://pi.dev/docs/latest/providers#radius",
    category: "aggregator",
    icon: "pi",
  }),
  builtinPreset({
    name: "Hugging Face",
    providerKey: "huggingface",
    envVar: "HF_TOKEN",
    websiteUrl: "https://huggingface.co",
    apiKeyUrl: "https://huggingface.co/settings/tokens",
    icon: "huggingface",
    iconColor: "#FFD21E",
  }),
  builtinPreset({
    name: "Fireworks AI",
    providerKey: "fireworks",
    envVar: "FIREWORKS_API_KEY",
    websiteUrl: "https://fireworks.ai",
    icon: "fireworks",
    iconColor: "#6C47FF",
  }),
  builtinPreset({
    name: "Together AI",
    providerKey: "together",
    envVar: "TOGETHER_API_KEY",
    websiteUrl: "https://www.together.ai",
    apiKeyUrl: "https://api.together.ai/settings/api-keys",
    icon: "together",
    iconColor: "#0F6FFF",
  }),
  builtinPreset({
    name: "Baseten",
    providerKey: "baseten",
    envVar: "BASETEN_API_KEY",
    websiteUrl: "https://www.baseten.co",
    icon: "baseten",
    iconColor: "#111111",
  }),
  builtinPreset({
    name: "Kimi For Coding",
    providerKey: "kimi-coding",
    envVar: "KIMI_API_KEY",
    websiteUrl: "https://www.kimi.com/code",
    category: "cn_official",
    icon: "kimi",
    iconColor: "#000000",
  }),
  builtinPreset({
    name: "MiniMax",
    providerKey: "minimax",
    envVar: "MINIMAX_API_KEY",
    websiteUrl: "https://www.minimax.io",
    icon: "minimax",
    iconColor: "#FF5A36",
  }),
  builtinPreset({
    name: "MiniMax (China)",
    providerKey: "minimax-cn",
    envVar: "MINIMAX_CN_API_KEY",
    websiteUrl: "https://www.minimaxi.com",
    category: "cn_official",
    icon: "minimax",
    iconColor: "#FF5A36",
  }),
  builtinPreset({
    name: "Qwen Token Plan",
    providerKey: "qwen-token-plan",
    envVar: "QWEN_TOKEN_PLAN_API_KEY",
    websiteUrl: "https://chat.qwen.ai",
    category: "cn_official",
    icon: "qwen",
    iconColor: "#615CED",
  }),
  builtinPreset({
    name: "Qwen Token Plan (Individual)",
    providerKey: "qwen-token-plan-individual",
    envVar: "QWEN_TOKEN_PLAN_API_KEY",
    websiteUrl: "https://chat.qwen.ai",
    category: "cn_official",
    icon: "qwen",
    iconColor: "#615CED",
  }),
  builtinPreset({
    name: "Qwen Token Plan (China)",
    providerKey: "qwen-token-plan-cn",
    envVar: "QWEN_TOKEN_PLAN_CN_API_KEY",
    websiteUrl: "https://chat.qwen.ai",
    category: "cn_official",
    icon: "qwen",
    iconColor: "#615CED",
  }),
  builtinPreset({
    name: "Xiaomi MiMo",
    providerKey: "xiaomi",
    envVar: "XIAOMI_API_KEY",
    websiteUrl: "https://mimo.xiaomi.com",
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#FF6900",
  }),
  builtinPreset({
    name: "Xiaomi MiMo Token Plan (China)",
    providerKey: "xiaomi-token-plan-cn",
    envVar: "XIAOMI_TOKEN_PLAN_CN_API_KEY",
    websiteUrl: "https://mimo.xiaomi.com",
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#FF6900",
  }),
  builtinPreset({
    name: "Xiaomi MiMo Token Plan (Amsterdam)",
    providerKey: "xiaomi-token-plan-ams",
    envVar: "XIAOMI_TOKEN_PLAN_AMS_API_KEY",
    websiteUrl: "https://mimo.xiaomi.com",
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#FF6900",
  }),
  builtinPreset({
    name: "Xiaomi MiMo Token Plan (Singapore)",
    providerKey: "xiaomi-token-plan-sgp",
    envVar: "XIAOMI_TOKEN_PLAN_SGP_API_KEY",
    websiteUrl: "https://mimo.xiaomi.com",
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#FF6900",
  }),
  {
    name: "Ollama",
    providerKey: "ollama",
    websiteUrl: "https://ollama.com",
    settingsConfig: {
      baseUrl: "http://localhost:11434/v1",
      apiKey: "ollama",
      api: "openai-completions",
      models: [{ id: "llama3.1:8b", name: "Llama 3.1 8B", input: ["text"] }],
    },
    category: "official",
    icon: "ollama",
    iconColor: "#111111",
  },
];

export const PI_BUILTIN_PROVIDER_KEYS = new Set(
  piProviderPresets
    .filter((preset) => preset.usesBuiltinCatalog)
    .map((preset) => preset.providerKey),
);

export function isPiBuiltinProviderKey(providerKey: string): boolean {
  return PI_BUILTIN_PROVIDER_KEYS.has(providerKey.trim());
}
