import { useCallback, useMemo, useState } from "react";
import type { AppId } from "@/lib/api";
import { useProvidersQuery } from "@/lib/query/queries";
import {
  PI_DEFAULT_CONFIG,
  type PiModel,
  type PiProviderSettingsConfig,
} from "@/config/piProviderPresets";

interface UsePiFormStateParams {
  initialData?: { settingsConfig?: Record<string, unknown> };
  appId: AppId;
  providerId?: string;
  onSettingsConfigChange: (config: string) => void;
  getSettingsConfig: () => string;
}

function readField<T>(
  initialData: UsePiFormStateParams["initialData"],
  field: string,
  fallback: T,
): T {
  const source = initialData?.settingsConfig ?? JSON.parse(PI_DEFAULT_CONFIG);
  return (source[field] as T | undefined) ?? fallback;
}

export function usePiFormState({
  initialData,
  appId,
  providerId,
  onSettingsConfigChange,
  getSettingsConfig,
}: UsePiFormStateParams) {
  const { data } = useProvidersQuery("pi");
  const existingPiKeys = useMemo(
    () =>
      Object.keys(data?.providers ?? {}).filter((key) => key !== providerId),
    [data?.providers, providerId],
  );

  const [piProviderKey, setPiProviderKey] = useState(
    appId === "pi" ? (providerId ?? "") : "",
  );
  const [piBaseUrl, setPiBaseUrl] = useState(
    appId === "pi" ? readField(initialData, "baseUrl", "") : "",
  );
  const [piApiKey, setPiApiKey] = useState(
    appId === "pi" ? readField(initialData, "apiKey", "") : "",
  );
  const [piApi, setPiApi] = useState(
    appId === "pi"
      ? readField(initialData, "api", "openai-completions")
      : "openai-completions",
  );
  const [piModels, setPiModels] = useState<PiModel[]>(
    appId === "pi" ? readField(initialData, "models", []) : [],
  );

  const updateConfig = useCallback(
    (updater: (config: Record<string, unknown>) => void) => {
      try {
        const config = JSON.parse(getSettingsConfig() || PI_DEFAULT_CONFIG);
        updater(config);
        onSettingsConfigChange(JSON.stringify(config, null, 2));
      } catch {
        // The raw editor remains authoritative while its JSON is invalid.
      }
    },
    [getSettingsConfig, onSettingsConfigChange],
  );

  const handlePiBaseUrlChange = useCallback(
    (value: string) => {
      setPiBaseUrl(value);
      updateConfig((config) => {
        config.baseUrl = value.trim().replace(/\/+$/, "");
      });
    },
    [updateConfig],
  );
  const handlePiApiKeyChange = useCallback(
    (value: string) => {
      setPiApiKey(value);
      updateConfig((config) => {
        if (value.trim()) {
          config.apiKey = value;
        } else {
          delete config.apiKey;
        }
      });
    },
    [updateConfig],
  );
  const handlePiApiChange = useCallback(
    (value: string) => {
      setPiApi(value);
      updateConfig((config) => {
        config.api = value;
      });
    },
    [updateConfig],
  );
  const handlePiModelsChange = useCallback(
    (value: PiModel[]) => {
      setPiModels(value);
      updateConfig((config) => {
        config.models = value;
      });
    },
    [updateConfig],
  );

  const resetPiState = useCallback(
    (config?: PiProviderSettingsConfig, providerKey = "") => {
      setPiProviderKey(providerKey);
      setPiBaseUrl(config?.baseUrl ?? "");
      setPiApiKey(config?.apiKey ?? "");
      setPiApi(config?.api ?? "openai-completions");
      setPiModels(config?.models ?? []);
    },
    [],
  );

  return {
    piProviderKey,
    setPiProviderKey,
    piBaseUrl,
    piApiKey,
    piApi,
    piModels,
    existingPiKeys,
    handlePiBaseUrlChange,
    handlePiApiKeyChange,
    handlePiApiChange,
    handlePiModelsChange,
    resetPiState,
  };
}
