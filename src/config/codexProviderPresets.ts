/**
 * Codex 预设供应商配置模板
 */
import { ProviderCategory } from "../types";
import type {
  CodexApiFormat,
  CodexCatalogModel,
  CodexChatReasoning,
} from "../types";
import type { PresetTheme } from "./claudeProviderPresets";

export interface CodexProviderPreset {
  name: string;
  nameKey?: string; // i18n key for localized display name
  websiteUrl: string;
  // 第三方供应商可提供单独的获取 API Key 链接
  apiKeyUrl?: string;
  auth: Record<string, any>; // 将写入 ~/.codex/auth.json
  config: string; // 将写入 ~/.codex/config.toml（TOML 字符串）
  isOfficial?: boolean; // 标识是否为官方预设
  isPartner?: boolean; // 标识是否为商业合作伙伴
  partnerPromotionKey?: string; // 合作伙伴促销信息的 i18n key
  category?: ProviderCategory; // 新增：分类
  isCustomTemplate?: boolean; // 标识是否为自定义模板
  // 新增：请求地址候选列表（用于地址管理/测速）
  endpointCandidates?: string[];
  // 新增：视觉主题配置
  theme?: PresetTheme;
  // 图标配置
  icon?: string; // 图标名称
  iconColor?: string; // 图标颜色
  // Codex API 格式
  apiFormat?: CodexApiFormat;
  // Codex Chat 本地路由模式下的模型目录
  modelCatalog?: CodexCatalogModel[];
  // Codex Responses -> Chat Completions reasoning capability defaults
  codexChatReasoning?: CodexChatReasoning;
}

/**
 * 生成第三方供应商的 auth.json
 */
export function generateThirdPartyAuth(apiKey: string): Record<string, any> {
  return {
    OPENAI_API_KEY: apiKey || "",
  };
}

/**
 * 生成第三方供应商的 config.toml
 */
export function generateThirdPartyConfig(
  providerName: string,
  baseUrl: string,
  modelName = "gpt-5.5",
): string {
  const tomlString = (value: string) => JSON.stringify(value);

  return `model_provider = "custom"
model = ${tomlString(modelName)}
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = ${tomlString(providerName)}
base_url = ${tomlString(baseUrl)}
wire_api = "responses"`;
}

/**
 * DeepSeek exposes a Codex-compatible Responses API directly. Keep the
 * provider id and auth-mode fields aligned with DeepSeek's Codex guide; the
 * API key itself remains in Agent Switch's protected provider settings and is
 * projected to Codex command-backed auth when the provider becomes active.
 */
function generateDeepSeekCodexConfig(): string {
  return `model = "deepseek-v4-flash"
model_provider = "deepseek"
preferred_auth_method = "apikey"
forced_login_method = "api"
model_reasoning_effort = "high"
model_catalog_json = "~/.codex/models.json"

[model_providers.deepseek]
name = "deepseek"
base_url = "https://api.deepseek.com/"
wire_api = "responses"`;
}

function modelCatalog(
  models: Array<
    string | { model: string; displayName?: string; contextWindow?: number }
  >,
): CodexCatalogModel[] {
  return models.map((entry) =>
    typeof entry === "string"
      ? { model: entry }
      : {
          model: entry.model,
          displayName: entry.displayName,
          contextWindow: entry.contextWindow,
        },
  );
}

export const codexProviderPresets: CodexProviderPreset[] = [
  {
    name: "OpenAI Official",
    websiteUrl: "https://chatgpt.com/codex",
    isOfficial: true,
    category: "official",
    auth: {},
    config: ``,
    theme: {
      icon: "codex",
      backgroundColor: "#1F2937", // gray-800
      textColor: "#FFFFFF",
    },
    icon: "openai",
    iconColor: "#00A67E",
  },
  {
    name: "火山Agentplan",
    websiteUrl:
      "https://www.volcengine.com/activity/codingplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://www.volcengine.com/activity/codingplan?ac=MMAP8JTTCAQ2&rc=6J6FV5N2&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "ark_agentplan",
      "https://ark.cn-beijing.volces.com/api/coding/v3",
      "ark-code-latest",
    ),
    endpointCandidates: ["https://ark.cn-beijing.volces.com/api/coding/v3"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "ark-code-latest",
        displayName: "Ark Code Latest",
        contextWindow: 256000,
      },
    ]),
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "volcengine_agentplan",
    icon: "huoshan",
    iconColor: "#3370FF",
  },
  {
    name: "BytePlus",
    websiteUrl:
      "https://www.byteplus.com/en/product/modelark?utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://www.byteplus.com/en/product/modelark?utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "byteplus",
      "https://ark.ap-southeast.bytepluses.com/api/coding/v3",
      "ark-code-latest",
    ),
    endpointCandidates: [
      "https://ark.ap-southeast.bytepluses.com/api/coding/v3",
    ],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "ark-code-latest",
        displayName: "Ark Code Latest",
        contextWindow: 256000,
      },
    ]),
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "byteplus",
    icon: "byteplus",
    iconColor: "#3370FF",
  },
  {
    name: "DouBaoSeed",
    websiteUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    apiKeyUrl:
      "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey?apikey=%7B%7D&utm_campaign=hw&utm_content=ccswitch&utm_medium=devrel_tool_web&utm_source=OWO&utm_term=ccswitch",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "doubaoseed",
      "https://ark.cn-beijing.volces.com/api/v3",
      "doubao-seed-2-0-code-preview-latest",
    ),
    endpointCandidates: ["https://ark.cn-beijing.volces.com/api/v3"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "doubao-seed-2-0-code-preview-latest",
        displayName: "Doubao Seed Code Preview",
        contextWindow: 256000,
      },
    ]),
    category: "cn_official",
    isPartner: true,
    partnerPromotionKey: "doubaoseed",
    icon: "doubao",
    iconColor: "#3370FF",
  },
  {
    name: "Azure OpenAI",
    websiteUrl:
      "https://learn.microsoft.com/en-us/azure/ai-foundry/openai/how-to/codex",
    category: "third_party",
    isOfficial: true,
    auth: generateThirdPartyAuth(""),
    config: `model_provider = "custom"
model = "gpt-5.5"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "Azure OpenAI"
base_url = "https://YOUR_RESOURCE_NAME.openai.azure.com/openai"
env_key = "OPENAI_API_KEY"
query_params = { "api-version" = "2025-04-01-preview" }
wire_api = "responses"`,
    endpointCandidates: ["https://YOUR_RESOURCE_NAME.openai.azure.com/openai"],
    theme: {
      icon: "codex",
      backgroundColor: "#0078D4",
      textColor: "#FFFFFF",
    },
    icon: "azure",
    iconColor: "#0078D4",
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    auth: generateThirdPartyAuth(""),
    config: generateDeepSeekCodexConfig(),
    endpointCandidates: ["https://api.deepseek.com/"],
    apiFormat: "openai_responses",
    modelCatalog: modelCatalog([
      {
        model: "deepseek-v4-flash",
        displayName: "DeepSeek-V4-Flash",
        contextWindow: 1048576,
      },
      {
        model: "deepseek-v4-pro",
        displayName: "DeepSeek-V4-Pro",
        contextWindow: 1048576,
      },
    ]),
    category: "cn_official",
    icon: "deepseek",
    iconColor: "#1E88E5",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/claude-code?ic=RRVJPB5SII",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "zhipu_glm",
      "https://open.bigmodel.cn/api/coding/paas/v4",
      "glm-5.1",
    ),
    endpointCandidates: ["https://open.bigmodel.cn/api/coding/paas/v4"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      { model: "glm-5.1", displayName: "GLM-5.1", contextWindow: 200000 },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Zhipu GLM en",
    websiteUrl: "https://z.ai",
    apiKeyUrl: "https://z.ai/subscribe?ic=8JVLJQFSKB",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "zhipu_glm_en",
      "https://api.z.ai/api/coding/paas/v4",
      "glm-5.1",
    ),
    endpointCandidates: ["https://api.z.ai/api/coding/paas/v4"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      { model: "glm-5.1", displayName: "GLM-5.1", contextWindow: 200000 },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "zhipu",
    iconColor: "#0F62FE",
  },
  {
    name: "Baidu Qianfan Coding Plan",
    websiteUrl: "https://cloud.baidu.com/product/qianfan_modelbuilder",
    apiKeyUrl:
      "https://console.bce.baidu.com/qianfan/ais/console/applicationConsole/application",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "qianfan_coding",
      "https://qianfan.baidubce.com/v2/coding",
      "qianfan-code-latest",
    ),
    endpointCandidates: ["https://qianfan.baidubce.com/v2/coding"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "qianfan-code-latest",
        displayName: "Qianfan Code Latest",
        contextWindow: 131072,
      },
    ]),
    category: "cn_official",
    icon: "baidu",
    iconColor: "#2932E1",
  },
  {
    name: "Bailian",
    websiteUrl: "https://bailian.console.aliyun.com",
    apiKeyUrl: "https://bailian.console.aliyun.com/#/api-key",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "bailian",
      "https://dashscope.aliyuncs.com/compatible-mode/v1",
      "qwen3-coder-plus",
    ),
    endpointCandidates: ["https://dashscope.aliyuncs.com/compatible-mode/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "qwen3-coder-plus",
        displayName: "Qwen3 Coder Plus",
        contextWindow: 1000000,
      },
      { model: "qwen3-max", displayName: "Qwen3 Max", contextWindow: 262144 },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "enable_thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "bailian",
    iconColor: "#624AFF",
  },
  {
    name: "Kimi",
    websiteUrl: "https://platform.moonshot.cn/console?aff=cc-switch",
    apiKeyUrl: "https://platform.moonshot.cn/console/api-keys?aff=cc-switch",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "kimi",
      "https://api.moonshot.cn/v1",
      "kimi-k2.7-code",
    ),
    endpointCandidates: ["https://api.moonshot.cn/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "kimi-k2.7-code",
        displayName: "Kimi K2.7 Code",
        contextWindow: 262144,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "Kimi For Coding",
    websiteUrl: "https://www.kimi.com/code/docs/",
    apiKeyUrl: "https://www.kimi.com/code/",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "kimi_coding",
      "https://api.kimi.com/coding/v1",
      "kimi-for-coding",
    ),
    endpointCandidates: ["https://api.kimi.com/coding/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "kimi-for-coding",
        displayName: "Kimi For Coding",
        contextWindow: 262144,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "kimi",
    iconColor: "#6366F1",
  },
  {
    name: "StepFun",
    websiteUrl: "https://platform.stepfun.com/step-plan",
    apiKeyUrl: "https://platform.stepfun.com/interface-key",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "stepfun",
      "https://api.stepfun.com/step_plan/v1",
      "step-3.5-flash-2603",
    ),
    endpointCandidates: ["https://api.stepfun.com/step_plan/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "step-3.5-flash-2603",
        displayName: "Step 3.5 Flash 2603",
        contextWindow: 262144,
      },
      {
        model: "step-3.5-flash",
        displayName: "Step 3.5 Flash",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "StepFun en",
    websiteUrl: "https://platform.stepfun.ai/step-plan",
    apiKeyUrl: "https://platform.stepfun.ai/interface-key",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "stepfun_en",
      "https://api.stepfun.ai/step_plan/v1",
      "step-3.5-flash-2603",
    ),
    endpointCandidates: ["https://api.stepfun.ai/step_plan/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "step-3.5-flash-2603",
        displayName: "Step 3.5 Flash 2603",
        contextWindow: 262144,
      },
      {
        model: "step-3.5-flash",
        displayName: "Step 3.5 Flash",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
    icon: "stepfun",
    iconColor: "#16D6D2",
  },
  {
    name: "ModelScope",
    websiteUrl: "https://modelscope.cn",
    apiKeyUrl: "https://modelscope.cn/my/myaccesstoken",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "modelscope",
      "https://api-inference.modelscope.cn/v1",
      "ZhipuAI/GLM-5.1",
    ),
    endpointCandidates: ["https://api-inference.modelscope.cn/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "ZhipuAI/GLM-5.1",
        displayName: "ZhipuAI / GLM-5.1",
        contextWindow: 200000,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "modelscope",
    iconColor: "#624AFF",
  },
  {
    name: "Longcat",
    websiteUrl: "https://longcat.chat/platform",
    apiKeyUrl: "https://longcat.chat/platform/api_keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "longcat",
      "https://api.longcat.chat/openai/v1",
      "LongCat-Flash-Chat",
    ),
    endpointCandidates: ["https://api.longcat.chat/openai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "LongCat-Flash-Chat",
        displayName: "LongCat Flash Chat",
        contextWindow: 262144,
      },
    ]),
    category: "cn_official",
    icon: "longcat",
    iconColor: "#29E154",
  },
  {
    name: "MiniMax",
    websiteUrl: "https://platform.minimaxi.com",
    apiKeyUrl: "https://platform.minimaxi.com/subscribe/coding-plan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "minimax",
      "https://api.minimaxi.com/v1",
      "MiniMax-M2.7",
    ),
    endpointCandidates: ["https://api.minimaxi.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "MiniMax-M2.7",
        displayName: "MiniMax M2.7",
        contextWindow: 200000,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "reasoning_split",
      effortParam: "none",
      outputFormat: "reasoning_details",
    },
    category: "cn_official",
    partnerPromotionKey: "minimax_cn",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "MiniMax en",
    websiteUrl: "https://platform.minimax.io",
    apiKeyUrl: "https://platform.minimax.io/subscribe/coding-plan",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "minimax_en",
      "https://api.minimax.io/v1",
      "MiniMax-M2.7",
    ),
    endpointCandidates: ["https://api.minimax.io/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "MiniMax-M2.7",
        displayName: "MiniMax M2.7",
        contextWindow: 200000,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "reasoning_split",
      effortParam: "none",
      outputFormat: "reasoning_details",
    },
    category: "cn_official",
    partnerPromotionKey: "minimax_en",
    theme: {
      backgroundColor: "#f64551",
      textColor: "#FFFFFF",
    },
    icon: "minimax",
    iconColor: "#FF6B6B",
  },
  {
    name: "BaiLing",
    websiteUrl: "https://alipaytbox.yuque.com/sxs0ba/ling/get_started",
    apiKeyUrl: "https://ling.tbox.cn/open",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "bailing",
      "https://api.tbox.cn/api/llm/v1",
      "Ling-2.5-1T",
    ),
    endpointCandidates: ["https://api.tbox.cn/api/llm/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "Ling-2.5-1T",
        displayName: "Ling-2.5-1T",
        contextWindow: 131072,
      },
    ]),
    category: "cn_official",
  },
  {
    name: "Xiaomi MiMo",
    websiteUrl: "https://platform.xiaomimimo.com",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "xiaomi_mimo",
      "https://api.xiaomimimo.com/v1",
      "mimo-v2.5-pro",
    ),
    endpointCandidates: ["https://api.xiaomimimo.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "mimo-v2.5-pro",
        displayName: "MiMo V2.5 Pro",
        contextWindow: 1048576,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "Xiaomi MiMo Token Plan (China)",
    websiteUrl: "https://platform.xiaomimimo.com/#/token-plan",
    apiKeyUrl: "https://platform.xiaomimimo.com/#/console/plan-manage",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "xiaomi_mimo_token_plan",
      "https://token-plan-cn.xiaomimimo.com/v1",
      "mimo-v2.5-pro",
    ),
    endpointCandidates: ["https://token-plan-cn.xiaomimimo.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "mimo-v2.5-pro",
        displayName: "MiMo V2.5 Pro",
        contextWindow: 1048576,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "cn_official",
    icon: "xiaomimimo",
    iconColor: "#000000",
  },
  {
    name: "SiliconFlow",
    websiteUrl: "https://siliconflow.cn",
    apiKeyUrl: "https://cloud.siliconflow.cn/i/drGuwc9k",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "siliconflow",
      "https://api.siliconflow.cn/v1",
      "Pro/MiniMaxAI/MiniMax-M2.7",
    ),
    endpointCandidates: ["https://api.siliconflow.cn/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "Pro/MiniMaxAI/MiniMax-M2.7",
        displayName: "Pro / MiniMax M2.7",
        contextWindow: 200000,
      },
    ]),
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "siliconflow",
    icon: "siliconflow",
    iconColor: "#6E29F6",
  },
  {
    name: "SiliconFlow en",
    websiteUrl: "https://siliconflow.com",
    apiKeyUrl: "https://cloud.siliconflow.cn/i/drGuwc9k",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "siliconflow_en",
      "https://api.siliconflow.com/v1",
      "MiniMaxAI/MiniMax-M2.7",
    ),
    endpointCandidates: ["https://api.siliconflow.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "MiniMaxAI/MiniMax-M2.7",
        displayName: "MiniMax M2.7",
        contextWindow: 200000,
      },
    ]),
    category: "aggregator",
    isPartner: true,
    partnerPromotionKey: "siliconflow",
    icon: "siliconflow",
    iconColor: "#000000",
  },
  {
    name: "Novita AI",
    websiteUrl: "https://novita.ai",
    apiKeyUrl: "https://novita.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "novita",
      "https://api.novita.ai/openai/v1",
      "zai-org/glm-5.1",
    ),
    endpointCandidates: ["https://api.novita.ai/openai/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "zai-org/glm-5.1",
        displayName: "GLM-5.1",
        contextWindow: 202800,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "novita",
    iconColor: "#000000",
  },
  {
    name: "Nvidia",
    websiteUrl: "https://build.nvidia.com",
    apiKeyUrl: "https://build.nvidia.com/settings/api-keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "nvidia",
      "https://integrate.api.nvidia.com/v1",
      "moonshotai/kimi-k2.5",
    ),
    endpointCandidates: ["https://integrate.api.nvidia.com/v1"],
    apiFormat: "openai_chat",
    modelCatalog: modelCatalog([
      {
        model: "moonshotai/kimi-k2.5",
        displayName: "Kimi K2.5",
        contextWindow: 262144,
      },
    ]),
    codexChatReasoning: {
      supportsThinking: true,
      supportsEffort: false,
      thinkingParam: "thinking",
      effortParam: "none",
      outputFormat: "reasoning_content",
    },
    category: "aggregator",
    icon: "nvidia",
    iconColor: "#000000",
  },
  {
    name: "AiHubMix",
    websiteUrl: "https://aihubmix.com",
    category: "aggregator",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "aihubmix",
      "https://aihubmix.com/v1",
      "gpt-5.5",
    ),
    endpointCandidates: [
      "https://aihubmix.com/v1",
      "https://api.aihubmix.com/v1",
    ],
  },
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "openrouter",
      "https://openrouter.ai/api/v1",
      "gpt-5.5",
    ),
    category: "aggregator",
    icon: "openrouter",
    iconColor: "#6566F1",
  },
  {
    name: "TheRouter",
    websiteUrl: "https://therouter.ai",
    apiKeyUrl: "https://dashboard.therouter.ai",
    auth: generateThirdPartyAuth(""),
    config: generateThirdPartyConfig(
      "therouter",
      "https://api.therouter.ai/v1",
      "openai/gpt-5.3-codex",
    ),
    endpointCandidates: ["https://api.therouter.ai/v1"],
    category: "aggregator",
  },
];
