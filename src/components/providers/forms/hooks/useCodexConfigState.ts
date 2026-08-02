import { useState, useCallback, useEffect, useRef } from "react";
import {
  extractCodexBaseUrl,
  extractCodexExperimentalBearerToken,
  setCodexBaseUrl as setCodexBaseUrlInConfig,
  updateCodexExperimentalBearerToken,
} from "@/utils/providerConfigUtils";
import { normalizeTomlText } from "@/utils/textNormalization";
import type { CodexCatalogModel } from "@/types";

interface UseCodexConfigStateProps {
  initialData?: {
    settingsConfig?: Record<string, unknown>;
  };
}

function pickCodexApiKey(
  authObj: { OPENAI_API_KEY?: unknown } | null | undefined,
): string {
  if (authObj && typeof authObj.OPENAI_API_KEY === "string") {
    const key = authObj.OPENAI_API_KEY;
    if (key) return key;
  }
  return "";
}

// Lift legacy direct bearer credentials into Agent Switch's protected provider
// settings. Live config.toml is generated with Codex command-backed auth.
function migrateLegacyCodexCredential(
  authObj: Record<string, unknown>,
  configText: string,
): { auth: Record<string, unknown>; config: string; apiKey: string } {
  const legacyToken = extractCodexExperimentalBearerToken(configText) || "";
  const currentToken = pickCodexApiKey(authObj);
  const apiKey = currentToken || legacyToken;
  return {
    auth: apiKey ? { ...authObj, OPENAI_API_KEY: apiKey } : authObj,
    config: updateCodexExperimentalBearerToken(configText, ""),
    apiKey,
  };
}

/**
 * 管理 Codex 配置状态
 * Codex 配置包含两部分：auth.json (JSON) 和 config.toml (TOML 字符串)
 */
export function useCodexConfigState({ initialData }: UseCodexConfigStateProps) {
  const [codexAuth, setCodexAuthState] = useState("");
  const [codexConfig, setCodexConfigState] = useState("");
  const [codexApiKey, setCodexApiKey] = useState("");
  const [codexBaseUrl, setCodexBaseUrl] = useState("");
  const [codexCatalogModels, setCodexCatalogModels] = useState<
    CodexCatalogModel[]
  >([]);
  const [codexAuthError, setCodexAuthError] = useState("");

  const isUpdatingCodexBaseUrlRef = useRef(false);

  // 初始化 Codex 配置（编辑模式）
  useEffect(() => {
    if (!initialData) return;

    const config = initialData.settingsConfig;
    if (typeof config === "object" && config !== null) {
      const auth = (config as any).auth || {};
      const configStr =
        typeof (config as any).config === "string"
          ? (config as any).config
          : "";
      const migrated = migrateLegacyCodexCredential(auth, configStr);
      setCodexAuthState(JSON.stringify(migrated.auth, null, 2));
      setCodexConfigState(migrated.config);

      const modelCatalog = (config as any).modelCatalog;
      const rawCatalogModels = Array.isArray(modelCatalog?.models)
        ? modelCatalog.models
        : [];
      setCodexCatalogModels(
        rawCatalogModels
          .map((item: any) => ({
            model: typeof item?.model === "string" ? item.model : "",
            displayName:
              typeof item?.displayName === "string"
                ? item.displayName
                : typeof item?.display_name === "string"
                  ? item.display_name
                  : "",
            contextWindow:
              typeof item?.contextWindow === "string" ||
              typeof item?.contextWindow === "number"
                ? item.contextWindow
                : typeof item?.context_window === "string" ||
                    typeof item?.context_window === "number"
                  ? item.context_window
                  : "",
          }))
          .filter((item: CodexCatalogModel) => item.model.trim()),
      );

      // 提取 Base URL
      const initialBaseUrl = extractCodexBaseUrl(migrated.config);
      if (initialBaseUrl) {
        setCodexBaseUrl(initialBaseUrl);
      }

      setCodexApiKey(migrated.apiKey);
    }
  }, [initialData]);

  // 与 TOML 配置保持基础 URL 同步
  useEffect(() => {
    if (isUpdatingCodexBaseUrlRef.current) {
      return;
    }
    const extracted = extractCodexBaseUrl(codexConfig) || "";
    setCodexBaseUrl((prev) => (prev === extracted ? prev : extracted));
  }, [codexConfig]);

  // 获取 API Key（从 auth JSON）
  const getCodexAuthApiKey = useCallback((authString: string): string => {
    try {
      const auth = JSON.parse(authString || "{}");
      return typeof auth.OPENAI_API_KEY === "string" ? auth.OPENAI_API_KEY : "";
    } catch {
      return "";
    }
  }, []);

  // 从 codexAuth 中提取并同步 API Key
  useEffect(() => {
    let parsed: { OPENAI_API_KEY?: unknown } | null = null;
    try {
      parsed = JSON.parse(codexAuth || "{}");
    } catch {
      parsed = null;
    }
    const extractedKey = pickCodexApiKey(parsed);
    setCodexApiKey((prev) => (prev === extractedKey ? prev : extractedKey));
  }, [codexAuth]);

  // 验证 Codex Auth JSON
  const validateCodexAuth = useCallback((value: string): string => {
    if (!value.trim()) return "";
    try {
      const parsed = JSON.parse(value);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        return "Auth JSON must be an object";
      }
      return "";
    } catch {
      return "Invalid JSON format";
    }
  }, []);

  // 设置 auth 并验证
  const setCodexAuth = useCallback(
    (value: string) => {
      setCodexAuthState(value);
      setCodexAuthError(validateCodexAuth(value));
    },
    [validateCodexAuth],
  );

  // 设置 config (支持函数更新)
  const setCodexConfig = useCallback(
    (value: string | ((prev: string) => string)) => {
      setCodexConfigState((prev) =>
        typeof value === "function"
          ? (value as (input: string) => string)(prev)
          : value,
      );
    },
    [],
  );

  // 处理 Codex API Key 输入并写回 Agent Switch 的供应商认证数据。
  // 旧版 direct bearer 条目会被移除，Live 配置由后端生成 command auth。
  const handleCodexApiKeyChange = useCallback(
    (key: string) => {
      const trimmed = key.trim();
      setCodexApiKey(trimmed);
      try {
        const auth = JSON.parse(codexAuth || "{}");
        auth.OPENAI_API_KEY = trimmed;
        setCodexAuth(JSON.stringify(auth, null, 2));
      } catch {
        // ignore
      }
      setCodexConfig((prev) => updateCodexExperimentalBearerToken(prev, ""));
    },
    [codexAuth, setCodexAuth, setCodexConfig],
  );

  // 处理 Codex Base URL 变化
  const handleCodexBaseUrlChange = useCallback(
    (url: string) => {
      const sanitized = url.trim();
      setCodexBaseUrl(sanitized);

      isUpdatingCodexBaseUrlRef.current = true;
      setCodexConfig((prev) => setCodexBaseUrlInConfig(prev, sanitized));
      setTimeout(() => {
        isUpdatingCodexBaseUrlRef.current = false;
      }, 0);
    },
    [setCodexConfig],
  );

  // 处理 config 变化（同步 Base URL）
  const handleCodexConfigChange = useCallback(
    (value: string) => {
      // 归一化中文/全角/弯引号，避免 TOML 解析报错
      const normalized = normalizeTomlText(value);
      const legacyToken = extractCodexExperimentalBearerToken(normalized);
      const cleanConfig = updateCodexExperimentalBearerToken(normalized, "");
      setCodexConfig(cleanConfig);

      if (legacyToken) {
        setCodexApiKey(legacyToken);
        try {
          const auth = JSON.parse(codexAuth || "{}");
          auth.OPENAI_API_KEY = legacyToken;
          setCodexAuth(JSON.stringify(auth, null, 2));
        } catch {
          // Leave the existing auth editor error intact.
        }
      }

      if (!isUpdatingCodexBaseUrlRef.current) {
        const extracted = extractCodexBaseUrl(cleanConfig) || "";
        if (extracted !== codexBaseUrl) {
          setCodexBaseUrl(extracted);
        }
      }
    },
    [codexAuth, setCodexAuth, setCodexConfig, codexBaseUrl],
  );

  // 重置配置（用于预设切换）
  const resetCodexConfig = useCallback(
    (
      auth: Record<string, unknown>,
      config: string,
      modelCatalogModels: CodexCatalogModel[] = [],
    ) => {
      const migrated = migrateLegacyCodexCredential(auth, config);
      setCodexAuth(JSON.stringify(migrated.auth, null, 2));
      setCodexConfig(migrated.config);
      setCodexCatalogModels(modelCatalogModels);

      const baseUrl = extractCodexBaseUrl(migrated.config);
      setCodexBaseUrl(baseUrl || "");

      setCodexApiKey(migrated.apiKey);
    },
    [setCodexAuth, setCodexConfig, setCodexCatalogModels],
  );

  return {
    codexAuth,
    codexConfig,
    codexApiKey,
    codexBaseUrl,
    codexCatalogModels,
    codexAuthError,
    setCodexAuth,
    setCodexConfig,
    setCodexCatalogModels,
    handleCodexApiKeyChange,
    handleCodexBaseUrlChange,
    handleCodexConfigChange,
    resetCodexConfig,
    getCodexAuthApiKey,
    validateCodexAuth,
  };
}
