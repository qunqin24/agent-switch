import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { FormLabel } from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toast } from "sonner";
import {
  Download,
  Plus,
  Trash2,
  ChevronRight,
  Loader2,
  Sparkles,
  Link2,
  SlidersHorizontal,
  Layers,
} from "lucide-react";
import { ApiKeySection, ModelDropdown } from "./shared";
import { Switch } from "@/components/ui/switch";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import { opencodeNpmPackages } from "@/config/opencodeProviderPresets";
import { cn } from "@/lib/utils";
import { modelsDevCacheApi } from "@/lib/api/modelsDev";
import {
  buildOpenCodeReasoningEffortVariants,
  clearOpenCodeThinkingSettings,
  formatOpenCodeModelDisplayName,
  getOpenCodeThinkingProtocolForNpm,
  getOpenCodeReasoningEffort,
  getModelExtraFields,
  getOpenCodeThinkingLevel,
  getOpenCodeThinkingSettings,
  getOpenCodeThinkingSettingsForLevel,
  inferOpenCodeThinkingProtocol,
  isKnownModelKey,
  prepareOpenCodeModelForProtocolChange,
  removeAllAutomaticOpenCodeThinkingVariants,
  removeAutomaticOpenCodeReasoningEffortVariants,
  removeAutomaticOpenCodeThinkingVariants,
  setOpenCodeThinkingSettings,
  setOpenCodeReasoningEffort,
  supportsAutomaticOpenCodeThinkingConfig,
  type OpenCodeThinkingProtocol,
} from "./helpers/opencodeFormUtils";
import type { ProviderCategory, OpenCodeModel } from "@/types";
import {
  getModelsDevCapabilityDeclarations,
  getModelsDevReasoningEfforts,
  type ModelsDevCatalogModel,
} from "@/lib/modelsDevCatalog";
import { ProviderFormSection } from "./ProviderFormSection";

/**
 * Model ID input with local state to prevent focus loss.
 * The key prop issue: when Model ID changes, React sees it as a new element
 * and unmounts/remounts the input, losing focus. Using local state + onBlur
 * keeps the key stable during editing.
 */
function ModelIdInput({
  modelId,
  onChange,
  placeholder,
}: {
  modelId: string;
  onChange: (newId: string) => void;
  placeholder?: string;
}) {
  const [localValue, setLocalValue] = useState(modelId);

  // Sync when external modelId changes (e.g., undo operation)
  useEffect(() => {
    setLocalValue(modelId);
  }, [modelId]);

  return (
    <Input
      value={localValue}
      onChange={(e) => setLocalValue(e.target.value)}
      onBlur={() => {
        if (localValue !== modelId && localValue.trim()) {
          onChange(localValue);
        }
      }}
      placeholder={placeholder}
      className="flex-1"
    />
  );
}

/**
 * Extra option key input with local state to prevent focus loss.
 * Same pattern as ModelIdInput - use local state during editing,
 * only commit changes on blur.
 */
function ExtraOptionKeyInput({
  optionKey,
  onChange,
  placeholder,
}: {
  optionKey: string;
  onChange: (newKey: string) => void;
  placeholder?: string;
}) {
  // For new options with placeholder keys like "option-123", show empty string
  const displayValue = optionKey.startsWith("option-") ? "" : optionKey;
  const [localValue, setLocalValue] = useState(displayValue);

  // Sync when external key changes
  useEffect(() => {
    setLocalValue(optionKey.startsWith("option-") ? "" : optionKey);
  }, [optionKey]);

  return (
    <Input
      value={localValue}
      onChange={(e) => setLocalValue(e.target.value)}
      onBlur={() => {
        const trimmed = localValue.trim();
        if (trimmed && trimmed !== optionKey) {
          onChange(trimmed);
        }
      }}
      placeholder={placeholder}
      className="flex-1"
    />
  );
}

/**
 * Model option key input with local state to prevent focus loss.
 * Reuses the same pattern as ExtraOptionKeyInput.
 */
function ModelOptionKeyInput({
  optionKey,
  onChange,
  placeholder,
}: {
  optionKey: string;
  onChange: (newKey: string) => void;
  placeholder?: string;
}) {
  const displayValue = optionKey.startsWith("option-") ? "" : optionKey;
  const [localValue, setLocalValue] = useState(displayValue);

  useEffect(() => {
    setLocalValue(optionKey.startsWith("option-") ? "" : optionKey);
  }, [optionKey]);

  return (
    <Input
      value={localValue}
      onChange={(e) => setLocalValue(e.target.value)}
      onBlur={() => {
        const trimmed = localValue.trim();
        if (trimmed && trimmed !== optionKey) {
          onChange(trimmed);
        }
        // Reset to prop value: if parent accepted the rename, useEffect
        // will update localValue when the new optionKey prop arrives;
        // if parent rejected, this restores the correct display.
        setLocalValue(optionKey.startsWith("option-") ? "" : optionKey);
      }}
      placeholder={placeholder}
      className="flex-1"
    />
  );
}

function getModelVariants(model: OpenCodeModel): Record<string, unknown> {
  return model.variants &&
    typeof model.variants === "object" &&
    !Array.isArray(model.variants)
    ? (model.variants as Record<string, unknown>)
    : {};
}

interface OpenCodeFormFieldsProps {
  // NPM Package
  npm: string;
  onNpmChange: (value: string) => void;

  // API Key
  apiKey: string;
  onApiKeyChange: (value: string) => void;
  category?: ProviderCategory;
  shouldShowApiKeyLink: boolean;
  websiteUrl: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;

  // Base URL
  baseUrl: string;
  onBaseUrlChange: (value: string) => void;

  // Models
  models: Record<string, OpenCodeModel>;
  onModelsChange: (models: Record<string, OpenCodeModel>) => void;

  // Extra Options
  extraOptions: Record<string, string>;
  onExtraOptionsChange: (options: Record<string, string>) => void;
}

export function OpenCodeFormFields({
  npm,
  onNpmChange,
  apiKey,
  onApiKeyChange,
  category,
  shouldShowApiKeyLink,
  websiteUrl,
  isPartner,
  partnerPromotionKey,
  baseUrl,
  onBaseUrlChange,
  models,
  onModelsChange,
  extraOptions,
  onExtraOptionsChange,
}: OpenCodeFormFieldsProps) {
  const { t } = useTranslation();
  const modelsRef = useRef(models);
  const autoConfigurationRevisionRef = useRef(0);

  useEffect(() => {
    modelsRef.current = models;
  }, [models]);

  const [fetchedModels, setFetchedModels] = useState<FetchedModel[]>([]);
  const [isFetchingModels, setIsFetchingModels] = useState(false);

  const handleFetchModels = useCallback(() => {
    if (!baseUrl || !apiKey) {
      showFetchModelsError(null, t, {
        hasApiKey: !!apiKey,
        hasBaseUrl: !!baseUrl,
      });
      return;
    }
    setIsFetchingModels(true);
    fetchModelsForConfig(baseUrl, apiKey)
      .then((models) => {
        setFetchedModels(models);
        if (models.length === 0) {
          toast.info(t("providerForm.fetchModelsEmpty"));
        } else {
          toast.success(
            t("providerForm.fetchModelsSuccess", { count: models.length }),
          );
        }
      })
      .catch((err) => {
        console.warn("[ModelFetch] Failed:", err);
        showFetchModelsError(err, t);
      })
      .finally(() => setIsFetchingModels(false));
  }, [baseUrl, apiKey, t]);

  // Track which models have expanded options panel
  const [expandedModels, setExpandedModels] = useState<Set<string>>(new Set());
  const [modelsDevMetadata, setModelsDevMetadata] = useState<
    Record<string, ModelsDevCatalogModel | null>
  >({});
  const [modelsDevLoading, setModelsDevLoading] = useState<Set<string>>(
    new Set(),
  );
  const [thinkingProtocols, setThinkingProtocols] = useState<
    Record<string, OpenCodeThinkingProtocol>
  >({});

  const getThinkingProtocol = (key: string, model: OpenCodeModel) =>
    thinkingProtocols[key] ?? inferOpenCodeThinkingProtocol(npm, model);

  const getModelsDevCapabilitySummary = (metadata: ModelsDevCatalogModel) => {
    const capabilities: string[] = [];
    if (metadata.attachment) {
      capabilities.push(t("opencode.modelsDevCapabilityAttachment", "附件"));
    }
    if (metadata.reasoning) {
      capabilities.push(t("opencode.modelsDevCapabilityReasoning", "思考"));
    }
    if (metadata.tool_call) {
      capabilities.push(t("opencode.modelsDevCapabilityToolCall", "工具调用"));
    }
    if (metadata.structured_output) {
      capabilities.push(
        t("opencode.modelsDevCapabilityStructuredOutput", "结构化输出"),
      );
    }
    if (metadata.temperature) {
      capabilities.push(t("opencode.modelsDevCapabilityTemperature", "温度"));
    }
    if (
      metadata.modalities?.input?.length ||
      metadata.modalities?.output?.length
    ) {
      capabilities.push(
        `${metadata.modalities.input?.join("/") ?? ""} -> ${metadata.modalities.output?.join("/") ?? ""}`,
      );
    }
    return capabilities.join(" · ");
  };

  const loadModelsDevMetadata = useCallback(
    async (modelKey: string, model: OpenCodeModel, forceRefresh = false) => {
      if (
        !forceRefresh &&
        Object.prototype.hasOwnProperty.call(modelsDevMetadata, modelKey)
      ) {
        return modelsDevMetadata[modelKey];
      }

      setModelsDevLoading((prev) => new Set(prev).add(modelKey));
      try {
        const metadata = await modelsDevCacheApi.getModelMetadata(
          modelKey,
          model.name,
        );
        setModelsDevMetadata((prev) => ({
          ...prev,
          [modelKey]: metadata ?? null,
        }));
        return metadata ?? null;
      } catch (error) {
        toast.error(
          t("opencode.modelsDevThinkingLoadError", {
            defaultValue: "读取 Models.dev 失败",
          }),
        );
        console.warn("[Models.dev] Failed to load model metadata:", error);
        return null;
      } finally {
        setModelsDevLoading((prev) => {
          const next = new Set(prev);
          next.delete(modelKey);
          return next;
        });
      }
    },
    [modelsDevMetadata, t],
  );

  const updateModelThinking = (
    modelKey: string,
    protocol: OpenCodeThinkingProtocol,
    updater: (
      settings: ReturnType<typeof getOpenCodeThinkingSettings>,
    ) => ReturnType<typeof getOpenCodeThinkingSettings>,
  ) => {
    const model = models[modelKey];
    if (!model) return;
    const settings = getOpenCodeThinkingSettings(model, protocol);
    onModelsChange({
      ...models,
      [modelKey]: setOpenCodeThinkingSettings(
        model,
        protocol,
        updater(settings),
      ),
    });
  };

  const handleThinkingProtocolChange = (
    modelKey: string,
    protocol: OpenCodeThinkingProtocol,
  ) => {
    const model = models[modelKey];
    if (!model) return;
    const previousProtocol = getThinkingProtocol(modelKey, model);
    const previousSettings = getOpenCodeThinkingSettings(
      model,
      previousProtocol,
    );
    setThinkingProtocols((prev) => ({ ...prev, [modelKey]: protocol }));
    if (previousSettings.enabled) {
      onModelsChange({
        ...models,
        [modelKey]: setOpenCodeThinkingSettings(
          model,
          protocol,
          previousSettings,
        ),
      });
    }
  };

  const applyModelsDevAutoConfiguration = async (
    modelKey: string,
    sourceModel: OpenCodeModel,
    options: {
      notify: boolean;
      ignoreInMemoryMetadata?: boolean;
      npmOverride?: string;
      protocolOverride?: OpenCodeThinkingProtocol;
      requestRevision?: number;
    },
  ) => {
    const targetNpm = options.npmOverride ?? npm;
    if (!supportsAutomaticOpenCodeThinkingConfig(targetNpm)) {
      if (options.notify) {
        toast.info(
          t("opencode.modelsDevThinkingManualFormat", {
            defaultValue: "当前接口格式请在下方手动选择配置格式后启用思考",
          }),
        );
      }
      return;
    }
    const metadata = await loadModelsDevMetadata(
      modelKey,
      sourceModel,
      options.ignoreInMemoryMetadata,
    );
    if (!metadata) {
      if (options.notify) {
        toast.info(
          t("opencode.modelsDevModelNotFound", {
            model: modelKey,
            defaultValue:
              "Models.dev 中未找到 {{model}}；请检查模型 ID 或手动配置",
          }),
        );
      }
      return;
    }
    if (
      options.requestRevision !== undefined &&
      options.requestRevision !== autoConfigurationRevisionRef.current
    ) {
      return;
    }

    // Resolve against the latest form state so a delayed metadata response
    // cannot overwrite a model the user edited or removed in the meantime.
    const currentModels = modelsRef.current;
    const currentModel = currentModels[modelKey];
    if (!currentModel) return;

    const protocol =
      options.protocolOverride ?? getThinkingProtocol(modelKey, currentModel);
    const nativeEfforts = getModelsDevReasoningEfforts(metadata);
    const defaultNativeEffort = nativeEfforts[0];
    const capabilityDeclarations = getModelsDevCapabilityDeclarations(metadata);
    // Do not infer a thinking protocol or levels from a boolean capability.
    // Only the API's explicit reasoning_options may change thinking settings.
    const protocolCleanModel = options.protocolOverride
      ? clearOpenCodeThinkingSettings(currentModel)
      : currentModel;
    const configuredModel = defaultNativeEffort
      ? setOpenCodeReasoningEffort(
          protocolCleanModel,
          protocol,
          defaultNativeEffort,
        )
      : protocolCleanModel;
    const existingLimit = configuredModel.limit ?? {};
    const resolvedLimit = {
      context: existingLimit.context ?? metadata.limit?.context,
      output: existingLimit.output ?? metadata.limit?.output,
    };
    const currentVariants = getModelVariants(configuredModel);
    const existingVariants =
      defaultNativeEffort || options.protocolOverride
        ? removeAutomaticOpenCodeReasoningEffortVariants(
            options.protocolOverride
              ? removeAllAutomaticOpenCodeThinkingVariants(currentVariants)
              : removeAutomaticOpenCodeThinkingVariants(
                  currentVariants,
                  protocol,
                ),
          )
        : currentVariants;
    const resolvedVariants = {
      ...(defaultNativeEffort
        ? buildOpenCodeReasoningEffortVariants(protocol, nativeEfforts)
        : {}),
      ...existingVariants,
    };
    const nextModels = {
      ...currentModels,
      [modelKey]: {
        ...configuredModel,
        ...capabilityDeclarations,
        ...(resolvedLimit.context || resolvedLimit.output
          ? { limit: resolvedLimit }
          : {}),
        variants: resolvedVariants,
      },
    };
    modelsRef.current = nextModels;
    onModelsChange(nextModels);

    if (!options.notify) return;
    if (defaultNativeEffort) {
      toast.success(
        t("opencode.modelsDevThinkingConfigured", {
          name: metadata.name || modelKey,
          defaultValue: "已按 {{name}} 的能力自动开启思考",
        }),
      );
    } else if (metadata.reasoning) {
      toast.info(
        t("opencode.modelsDevReasoningOptionsUnavailable", {
          defaultValue:
            "Models.dev 未提供此模型的原生思考档位；仅补充已知模型限制",
        }),
      );
    } else {
      toast.warning(
        t("opencode.modelsDevThinkingUnsupported", {
          name: metadata.name || modelKey,
          defaultValue:
            "Models.dev 未标记 {{name}} 支持思考；已补充已知模型能力",
        }),
      );
    }
  };

  const handleAutoConfigureThinking = async (modelKey: string) => {
    const model = modelsRef.current[modelKey];
    if (!model) return;
    await applyModelsDevAutoConfiguration(modelKey, model, {
      notify: true,
      ignoreInMemoryMetadata: true,
      requestRevision: autoConfigurationRevisionRef.current,
    });
  };

  const handleNpmPackageChange = (nextNpm: string) => {
    if (nextNpm === npm) return;

    onNpmChange(nextNpm);
    const protocol = getOpenCodeThinkingProtocolForNpm(nextNpm);
    const currentModels = modelsRef.current;
    const requestRevision = ++autoConfigurationRevisionRef.current;
    const resetModels = Object.fromEntries(
      Object.entries(currentModels).map(([key, model]) => [
        key,
        prepareOpenCodeModelForProtocolChange(model),
      ]),
    );
    modelsRef.current = resetModels;
    onModelsChange(resetModels);
    setThinkingProtocols(
      Object.fromEntries(
        Object.keys(currentModels).map((key) => [key, protocol]),
      ),
    );

    for (const [modelKey, model] of Object.entries(resetModels)) {
      void applyModelsDevAutoConfiguration(modelKey, model, {
        notify: false,
        ignoreInMemoryMetadata: true,
        npmOverride: nextNpm,
        protocolOverride: protocol,
        requestRevision,
      });
    }
  };

  const handleGenerateThinkingVariants = async (modelKey: string) => {
    const model = models[modelKey];
    if (!model) return;
    const protocol = getThinkingProtocol(modelKey, model);
    const metadata = await loadModelsDevMetadata(modelKey, model, true);
    const nativeEfforts = getModelsDevReasoningEfforts(metadata);
    if (!nativeEfforts.length) {
      toast.info(
        t("opencode.modelsDevReasoningOptionsUnavailable", {
          defaultValue:
            "Models.dev 未提供此模型的原生思考档位；请在模型属性中手动添加",
        }),
      );
      return;
    }
    onModelsChange({
      ...models,
      [modelKey]: {
        ...model,
        variants: {
          ...buildOpenCodeReasoningEffortVariants(protocol, nativeEfforts),
          ...removeAutomaticOpenCodeReasoningEffortVariants(
            removeAutomaticOpenCodeThinkingVariants(
              getModelVariants(model),
              protocol,
            ),
          ),
        },
      },
    });
    toast.success(
      t("opencode.thinkingVariantsGenerated", {
        values: nativeEfforts.join(" / "),
        defaultValue: "已生成 {{values}} 思考预设",
      }),
    );
  };

  const handleModelLimitChange = (
    modelKey: string,
    field: "context" | "output",
    value: string,
  ) => {
    const model = models[modelKey];
    const limit = Number(value);
    if (!model || !Number.isInteger(limit) || limit <= 0) return;
    onModelsChange({
      ...models,
      [modelKey]: {
        ...model,
        limit: { ...model.limit, [field]: limit },
      },
    });
  };

  // Toggle model expand state
  const toggleModelExpand = (key: string) => {
    const willExpand = !expandedModels.has(key);
    setExpandedModels((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
    if (willExpand && models[key]) {
      void loadModelsDevMetadata(key, models[key]);
    }
  };

  // Add a new model entry
  const handleAddModel = () => {
    const newKey = `model-${Date.now()}`;
    onModelsChange({
      ...models,
      [newKey]: { name: "" },
    });
  };

  // Remove a model entry
  const handleRemoveModel = (key: string) => {
    const newModels = { ...models };
    delete newModels[key];
    modelsRef.current = newModels;
    onModelsChange(newModels);
    // Also remove from expanded set
    setExpandedModels((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  // Update model ID (key)
  const handleModelIdChange = (oldKey: string, newKey: string) => {
    const modelId = newKey.trim();
    if (oldKey === modelId || !modelId) return;
    const newModels: Record<string, OpenCodeModel> = {};
    let addedModel: OpenCodeModel | undefined;
    for (const [k, v] of Object.entries(models)) {
      if (k === oldKey) {
        addedModel = {
          ...v,
          ...(v.name.trim()
            ? {}
            : { name: formatOpenCodeModelDisplayName(modelId) }),
        };
        newModels[modelId] = addedModel;
      } else {
        newModels[k] = v;
      }
    }
    modelsRef.current = newModels;
    onModelsChange(newModels);
    // Update expanded set if this model was expanded
    if (expandedModels.has(oldKey)) {
      setExpandedModels((prev) => {
        const next = new Set(prev);
        next.delete(oldKey);
        next.add(modelId);
        return next;
      });
    }
    if (addedModel) {
      void applyModelsDevAutoConfiguration(modelId, addedModel, {
        notify: false,
        requestRevision: autoConfigurationRevisionRef.current,
      });
    }
  };

  // Update model name
  const handleModelNameChange = (key: string, name: string) => {
    const nextModels = {
      ...models,
      [key]: { ...models[key], name },
    };
    modelsRef.current = nextModels;
    onModelsChange(nextModels);
  };

  // Model options handlers
  const handleAddModelOption = (modelKey: string) => {
    const model = models[modelKey];
    const newOptionKey = `option-${Date.now()}`;
    onModelsChange({
      ...models,
      [modelKey]: {
        ...model,
        options: { ...model.options, [newOptionKey]: "" },
      },
    });
  };

  const handleRemoveModelOption = (modelKey: string, optionKey: string) => {
    const model = models[modelKey];
    const newOptions = { ...model.options };
    delete newOptions[optionKey];
    onModelsChange({
      ...models,
      [modelKey]: {
        ...model,
        options: Object.keys(newOptions).length > 0 ? newOptions : undefined,
      },
    });
  };

  const handleModelOptionKeyChange = (
    modelKey: string,
    oldKey: string,
    newKey: string,
  ) => {
    if (!newKey.trim() || oldKey === newKey) return;
    const model = models[modelKey];
    const newOptions: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(model.options || {})) {
      if (k === oldKey) newOptions[newKey] = v;
      else newOptions[k] = v;
    }
    onModelsChange({
      ...models,
      [modelKey]: { ...model, options: newOptions },
    });
  };

  const handleModelOptionValueChange = (
    modelKey: string,
    optionKey: string,
    value: string,
  ) => {
    const model = models[modelKey];
    let parsedValue: unknown;
    try {
      parsedValue = JSON.parse(value);
    } catch {
      parsedValue = value;
    }
    onModelsChange({
      ...models,
      [modelKey]: {
        ...model,
        options: { ...model.options, [optionKey]: parsedValue },
      },
    });
  };

  // Model extra field handlers (top-level properties like variants, cost)
  const handleAddModelExtraField = (modelKey: string) => {
    const model = models[modelKey];
    const newFieldKey = `option-${Date.now()}`;
    onModelsChange({
      ...models,
      [modelKey]: { ...model, [newFieldKey]: "" },
    });
  };

  const handleRemoveModelExtraField = (modelKey: string, fieldKey: string) => {
    const model = models[modelKey];
    const newModel = { ...model };
    delete newModel[fieldKey];
    onModelsChange({
      ...models,
      [modelKey]: newModel,
    });
  };

  const handleModelExtraFieldKeyChange = (
    modelKey: string,
    oldKey: string,
    newKey: string,
  ) => {
    if (!newKey.trim() || oldKey === newKey) return;
    const model = models[modelKey];
    // Reject reserved keys and duplicate extra field names
    if (isKnownModelKey(newKey) || (newKey !== oldKey && newKey in model))
      return;
    const newModel: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(model)) {
      if (k === oldKey) newModel[newKey] = v;
      else newModel[k] = v;
    }
    onModelsChange({
      ...models,
      [modelKey]: newModel as OpenCodeModel,
    });
  };

  const handleModelExtraFieldValueChange = (
    modelKey: string,
    fieldKey: string,
    value: string,
  ) => {
    const model = models[modelKey];
    let parsedValue: unknown;
    try {
      parsedValue = JSON.parse(value);
    } catch {
      parsedValue = value;
    }
    onModelsChange({
      ...models,
      [modelKey]: { ...model, [fieldKey]: parsedValue },
    });
  };

  // Extra Options handlers
  const handleAddExtraOption = () => {
    const newKey = `option-${Date.now()}`;
    onExtraOptionsChange({
      ...extraOptions,
      [newKey]: "",
    });
  };

  const handleRemoveExtraOption = (key: string) => {
    const newOptions = { ...extraOptions };
    delete newOptions[key];
    onExtraOptionsChange(newOptions);
  };

  const handleExtraOptionKeyChange = (oldKey: string, newKey: string) => {
    if (oldKey === newKey) return;
    const newOptions: Record<string, string> = {};
    for (const [k, v] of Object.entries(extraOptions)) {
      if (k === oldKey) {
        newOptions[newKey.trim() || oldKey] = v;
      } else {
        newOptions[k] = v;
      }
    }
    onExtraOptionsChange(newOptions);
  };

  const handleExtraOptionValueChange = (key: string, value: string) => {
    onExtraOptionsChange({
      ...extraOptions,
      [key]: value,
    });
  };

  return (
    <>
      {/* 连接配置：接口格式 + API Key + Base URL */}
      <ProviderFormSection
        sectionKey="connection"
        icon={Link2}
        title={t("opencode.connectionSection", {
          defaultValue: "连接配置",
        })}
      >
        {/* NPM Package Selector */}
        <div className="space-y-2">
          <FormLabel htmlFor="opencode-npm">
            {t("opencode.npmPackage", {
              defaultValue: "接口格式",
            })}
          </FormLabel>
          <Select value={npm} onValueChange={handleNpmPackageChange}>
            <SelectTrigger id="opencode-npm">
              <SelectValue
                placeholder={t("opencode.selectPackage", {
                  defaultValue: "Select a package",
                })}
              />
            </SelectTrigger>
            <SelectContent>
              {opencodeNpmPackages.map((pkg) => (
                <SelectItem key={pkg.value} value={pkg.value}>
                  {pkg.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            {t("opencode.npmPackageHint", {
              defaultValue:
                "Select the AI SDK package that matches your provider.",
            })}
          </p>
        </div>

        {/* API Key */}
        <ApiKeySection
          value={apiKey}
          onChange={onApiKeyChange}
          category={category}
          shouldShowLink={shouldShowApiKeyLink}
          websiteUrl={websiteUrl}
          isPartner={isPartner}
          partnerPromotionKey={partnerPromotionKey}
        />

        {/* Base URL */}
        <div className="space-y-2">
          <FormLabel htmlFor="opencode-baseurl">
            {t("opencode.baseUrl", { defaultValue: "Base URL" })}
          </FormLabel>
          <Input
            id="opencode-baseurl"
            value={baseUrl}
            onChange={(e) => onBaseUrlChange(e.target.value)}
            placeholder="https://api.example.com/v1"
          />
          <p className="text-xs text-muted-foreground">
            {t("opencode.baseUrlHint", {
              defaultValue:
                "The base URL for the API endpoint. Leave empty to use the default endpoint for official SDKs.",
            })}
          </p>
        </div>
      </ProviderFormSection>

      {/* Extra Options Editor */}
      <ProviderFormSection
        sectionKey="options"
        icon={SlidersHorizontal}
        title={t("opencode.extraOptions", { defaultValue: "额外选项" })}
        contentClassName="space-y-3"
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handleAddExtraOption}
            className="h-7 gap-1"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("opencode.addExtraOption", { defaultValue: "添加" })}
          </Button>
        }
      >
        {Object.keys(extraOptions).length === 0 ? (
          <p className="text-sm text-muted-foreground py-2">
            {t("opencode.noExtraOptions", {
              defaultValue: "暂无额外选项",
            })}
          </p>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-xs text-muted-foreground px-1 mb-1">
              <span className="flex-1">
                {t("opencode.extraOptionKey", { defaultValue: "键名" })}
              </span>
              <span className="flex-1">
                {t("opencode.extraOptionValue", { defaultValue: "值" })}
              </span>
              <span className="w-9" />
            </div>
            {Object.entries(extraOptions).map(([key, value]) => (
              <div key={key} className="flex items-center gap-2">
                <ExtraOptionKeyInput
                  optionKey={key}
                  onChange={(newKey) => handleExtraOptionKeyChange(key, newKey)}
                  placeholder={t("opencode.extraOptionKeyPlaceholder", {
                    defaultValue: "timeout",
                  })}
                />
                <Input
                  value={value}
                  onChange={(e) =>
                    handleExtraOptionValueChange(key, e.target.value)
                  }
                  placeholder={t("opencode.extraOptionValuePlaceholder", {
                    defaultValue: "600000",
                  })}
                  className="flex-1"
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  onClick={() => handleRemoveExtraOption(key)}
                  className="h-9 w-9 text-muted-foreground hover:text-destructive"
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        )}

        <p className="text-xs text-muted-foreground">
          {t("opencode.extraOptionsHint", {
            defaultValue:
              "配置额外的 SDK 选项，如 timeout、setCacheKey 等。值会自动解析类型（数字、布尔值等）。",
          })}
        </p>
      </ProviderFormSection>

      {/* Models Editor */}
      <ProviderFormSection
        sectionKey="models"
        icon={Layers}
        title={t("opencode.models", { defaultValue: "Models" })}
        contentClassName="space-y-3"
        actions={
          <div className="flex gap-1">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleFetchModels}
              disabled={isFetchingModels}
              className="h-7 gap-1"
            >
              {isFetchingModels ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Download className="h-3.5 w-3.5" />
              )}
              {t("providerForm.fetchModels")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleAddModel}
              className="h-7 gap-1"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("opencode.addModel", { defaultValue: "Add" })}
            </Button>
          </div>
        }
      >
        {Object.keys(models).length === 0 ? (
          <p className="text-sm text-muted-foreground py-2">
            {t("opencode.noModels", {
              defaultValue: "No models configured. Click Add to add a model.",
            })}
          </p>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2 text-xs text-muted-foreground px-1 mb-1">
              <span className="w-9" />
              <span className="flex-1">
                {t("opencode.modelId", { defaultValue: "模型 ID" })}
              </span>
              <span className="flex-1">
                {t("opencode.modelName", { defaultValue: "显示名称" })}
              </span>
              <span className="w-9" />
            </div>
            {Object.entries(models).map(([key, model]) => (
              <div key={key} className="space-y-2">
                {/* Model row */}
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => toggleModelExpand(key)}
                    className="h-9 w-9 shrink-0"
                  >
                    <ChevronRight
                      className={cn(
                        "h-4 w-4 transition-transform",
                        expandedModels.has(key) && "rotate-90",
                      )}
                    />
                  </Button>
                  <div className="flex gap-1 flex-1">
                    <ModelIdInput
                      modelId={key}
                      onChange={(newId) => handleModelIdChange(key, newId)}
                      placeholder={t("opencode.modelId", {
                        defaultValue: "Model ID",
                      })}
                    />
                    {fetchedModels.length > 0 && (
                      <ModelDropdown
                        models={fetchedModels}
                        onSelect={(id) => handleModelIdChange(key, id)}
                      />
                    )}
                  </div>
                  <Input
                    value={model.name}
                    onChange={(e) => handleModelNameChange(key, e.target.value)}
                    placeholder={t("opencode.modelName", {
                      defaultValue: "Display Name",
                    })}
                    className="flex-1"
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => handleRemoveModel(key)}
                    className="h-9 w-9 text-muted-foreground hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>

                {/* Expanded model details */}
                {expandedModels.has(key) && (
                  <div className="ml-9 pl-4 border-l-2 border-muted space-y-3">
                    {(() => {
                      const protocol = getThinkingProtocol(key, model);
                      const settings = getOpenCodeThinkingSettings(
                        model,
                        protocol,
                      );
                      const metadata = modelsDevMetadata[key];
                      const isLoadingMetadata = modelsDevLoading.has(key);
                      const nativeEfforts =
                        getModelsDevReasoningEfforts(metadata);

                      return (
                        <div className="space-y-3 rounded-md border border-border/60 bg-muted/20 p-3">
                          <div className="flex items-start justify-between gap-3">
                            <div className="min-w-0">
                              <span className="text-xs font-medium text-foreground">
                                {t("opencode.thinkingSettings", {
                                  defaultValue: "思考设置",
                                })}
                              </span>
                              {metadata && (
                                <>
                                  <p
                                    className={cn(
                                      "mt-0.5 text-xs",
                                      metadata.reasoning
                                        ? "text-emerald-600 dark:text-emerald-400"
                                        : "text-muted-foreground",
                                    )}
                                  >
                                    {metadata.reasoning
                                      ? t(
                                          "opencode.modelsDevReasoningSupported",
                                          {
                                            defaultValue:
                                              "Models.dev：支持思考",
                                          },
                                        )
                                      : t(
                                          "opencode.modelsDevReasoningUnsupported",
                                          {
                                            defaultValue:
                                              "Models.dev：未标记思考能力",
                                          },
                                        )}
                                  </p>
                                  <p className="mt-0.5 text-xs text-muted-foreground">
                                    {t("opencode.modelsDevCapabilitySummary", {
                                      capabilities:
                                        getModelsDevCapabilitySummary(metadata),
                                      defaultValue: "能力：{{capabilities}}",
                                    })}
                                  </p>
                                </>
                              )}
                            </div>
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() =>
                                void handleAutoConfigureThinking(key)
                              }
                              disabled={isLoadingMetadata}
                              className="h-7 shrink-0 gap-1"
                            >
                              {isLoadingMetadata ? (
                                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                              ) : (
                                <Sparkles className="h-3.5 w-3.5" />
                              )}
                              {t("opencode.autoConfigureThinking", {
                                defaultValue: "自动配置",
                              })}
                            </Button>
                          </div>

                          {isLoadingMetadata ? (
                            <p className="text-xs text-muted-foreground">
                              {t("opencode.modelsDevLoadingCapabilities", {
                                defaultValue: "正在读取 Models.dev 模型能力...",
                              })}
                            </p>
                          ) : nativeEfforts.length ? (
                            <p className="text-xs text-muted-foreground">
                              {t("opencode.modelsDevNativeEffortHint", {
                                values: nativeEfforts.join(" / "),
                                defaultValue:
                                  "Models.dev 原生思考档位：{{values}}",
                              })}
                            </p>
                          ) : (
                            <>
                              <div className="flex items-center justify-between gap-3">
                                <label
                                  htmlFor={`opencode-thinking-${key}`}
                                  className="text-sm font-medium"
                                >
                                  {t("opencode.thinkingEnabled", {
                                    defaultValue: "启用思考",
                                  })}
                                </label>
                                <Switch
                                  id={`opencode-thinking-${key}`}
                                  checked={settings.enabled}
                                  onCheckedChange={(enabled) =>
                                    updateModelThinking(
                                      key,
                                      protocol,
                                      (current) => ({
                                        ...current,
                                        enabled,
                                      }),
                                    )
                                  }
                                />
                              </div>

                              <div className="grid grid-cols-2 gap-2">
                                <div className="space-y-1.5">
                                  <span className="text-xs text-muted-foreground">
                                    {t("opencode.thinkingProtocol", {
                                      defaultValue: "配置格式",
                                    })}
                                  </span>
                                  <Select
                                    value={protocol}
                                    onValueChange={(value) =>
                                      handleThinkingProtocolChange(
                                        key,
                                        value as OpenCodeThinkingProtocol,
                                      )
                                    }
                                  >
                                    <SelectTrigger className="h-9">
                                      <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                      <SelectItem value="anthropic">
                                        {t(
                                          "opencode.thinkingProtocolAnthropic",
                                          {
                                            defaultValue: "Anthropic Thinking",
                                          },
                                        )}
                                      </SelectItem>
                                      <SelectItem value="openai">
                                        {t("opencode.thinkingProtocolOpenAI", {
                                          defaultValue:
                                            "OpenAI Reasoning Effort",
                                        })}
                                      </SelectItem>
                                      <SelectItem value="gemini">
                                        {t("opencode.thinkingProtocolGemini", {
                                          defaultValue:
                                            "Gemini Thinking Config",
                                        })}
                                      </SelectItem>
                                    </SelectContent>
                                  </Select>
                                </div>

                                {protocol === "openai" ? (
                                  <div className="space-y-1.5">
                                    <span className="text-xs text-muted-foreground">
                                      {t("opencode.thinkingEffort", {
                                        defaultValue: "思考强度",
                                      })}
                                    </span>
                                    <Select
                                      value={settings.effort}
                                      onValueChange={(value) =>
                                        updateModelThinking(
                                          key,
                                          protocol,
                                          (current) => ({
                                            ...current,
                                            effort:
                                              value as typeof current.effort,
                                          }),
                                        )
                                      }
                                    >
                                      <SelectTrigger className="h-9">
                                        <SelectValue />
                                      </SelectTrigger>
                                      <SelectContent>
                                        {(
                                          [
                                            "low",
                                            "medium",
                                            "high",
                                            "xhigh",
                                          ] as const
                                        ).map((effort) => (
                                          <SelectItem
                                            key={effort}
                                            value={effort}
                                          >
                                            {t(
                                              `opencode.thinkingEffort${effort[0].toUpperCase()}${effort.slice(1)}`,
                                              {
                                                defaultValue: effort,
                                              },
                                            )}
                                          </SelectItem>
                                        ))}
                                      </SelectContent>
                                    </Select>
                                  </div>
                                ) : (
                                  <div className="space-y-1.5">
                                    <span className="text-xs text-muted-foreground">
                                      {t("opencode.thinkingBudget", {
                                        defaultValue: "思考预算",
                                      })}
                                    </span>
                                    <Input
                                      type="number"
                                      min={1}
                                      step={1}
                                      value={settings.budgetTokens}
                                      onChange={(event) => {
                                        const budgetTokens = Number(
                                          event.target.value,
                                        );
                                        if (
                                          Number.isInteger(budgetTokens) &&
                                          budgetTokens > 0
                                        ) {
                                          updateModelThinking(
                                            key,
                                            protocol,
                                            (current) => ({
                                              ...current,
                                              budgetTokens,
                                            }),
                                          );
                                        }
                                      }}
                                      className="h-9"
                                    />
                                  </div>
                                )}
                              </div>
                            </>
                          )}
                        </div>
                      );
                    })()}

                    {(() => {
                      const protocol = getThinkingProtocol(key, model);
                      const settings = getOpenCodeThinkingSettings(
                        model,
                        protocol,
                      );
                      const nativeEfforts = getModelsDevReasoningEfforts(
                        modelsDevMetadata[key],
                      );
                      const thinkingLevel = nativeEfforts.length
                        ? (getOpenCodeReasoningEffort(model, protocol) ??
                          nativeEfforts[0])
                        : getOpenCodeThinkingLevel(protocol, settings);
                      const limit = model.limit ?? {};

                      return (
                        <div className="space-y-3 rounded-md border border-border/60 bg-muted/20 p-3">
                          <div className="flex items-center justify-between gap-3">
                            <span className="text-xs font-medium text-foreground">
                              {t("opencode.modelLimits", {
                                defaultValue: "模型限制与思考等级",
                              })}
                            </span>
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() =>
                                void handleGenerateThinkingVariants(key)
                              }
                              disabled={modelsDevLoading.has(key)}
                              className="h-7 gap-1"
                            >
                              <Plus className="h-3.5 w-3.5" />
                              {t("opencode.generateThinkingVariants", {
                                defaultValue: "生成等级预设",
                              })}
                            </Button>
                          </div>

                          <div className="grid grid-cols-2 gap-2">
                            <div className="space-y-1.5">
                              <span className="text-xs text-muted-foreground">
                                {t("opencode.contextLimit", {
                                  defaultValue: "上下文",
                                })}
                              </span>
                              <Input
                                type="number"
                                min={1}
                                step={1}
                                value={limit.context ?? ""}
                                onChange={(event) =>
                                  handleModelLimitChange(
                                    key,
                                    "context",
                                    event.target.value,
                                  )
                                }
                                placeholder="1000000"
                                className="h-9"
                              />
                            </div>
                            <div className="space-y-1.5">
                              <span className="text-xs text-muted-foreground">
                                {t("opencode.outputLimit", {
                                  defaultValue: "最大输出",
                                })}
                              </span>
                              <Input
                                type="number"
                                min={1}
                                step={1}
                                value={limit.output ?? ""}
                                onChange={(event) =>
                                  handleModelLimitChange(
                                    key,
                                    "output",
                                    event.target.value,
                                  )
                                }
                                placeholder="131072"
                                className="h-9"
                              />
                            </div>
                          </div>

                          <div className="space-y-1.5">
                            <span className="text-xs text-muted-foreground">
                              {t("opencode.thinkingLevel", {
                                defaultValue: "默认思考等级",
                              })}
                            </span>
                            <Select
                              value={thinkingLevel}
                              onValueChange={(value) => {
                                if (nativeEfforts.length) {
                                  onModelsChange({
                                    ...models,
                                    [key]: setOpenCodeReasoningEffort(
                                      model,
                                      protocol,
                                      value,
                                    ),
                                  });
                                  return;
                                }
                                updateModelThinking(key, protocol, () =>
                                  getOpenCodeThinkingSettingsForLevel(
                                    protocol,
                                    value as "low" | "medium" | "high",
                                  ),
                                );
                              }}
                            >
                              <SelectTrigger className="h-9">
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                {(nativeEfforts.length
                                  ? nativeEfforts
                                  : ["low", "medium", "high"]
                                ).map((level) => (
                                  <SelectItem key={level} value={level}>
                                    {t(
                                      `opencode.thinkingLevel${level[0].toUpperCase()}${level.slice(1)}`,
                                      { defaultValue: level },
                                    )}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </div>
                        </div>
                      );
                    })()}

                    {/* Model Properties (extra fields like variants, cost) */}
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-medium text-muted-foreground">
                          {t("opencode.modelExtraFields", {
                            defaultValue: "模型属性",
                          })}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => handleAddModelExtraField(key)}
                          className="h-6 px-2 gap-1"
                        >
                          <Plus className="h-3 w-3" />
                        </Button>
                      </div>
                      {Object.keys(getModelExtraFields(model)).length === 0 ? (
                        <p className="text-xs text-muted-foreground py-1">
                          {t("opencode.noModelExtraFields", {
                            defaultValue:
                              "模型属性 (variants, cost 等)，点击 + 添加",
                          })}
                        </p>
                      ) : (
                        Object.entries(getModelExtraFields(model)).map(
                          ([fKey, fValue]) => (
                            <div key={fKey} className="flex items-center gap-2">
                              <ModelOptionKeyInput
                                optionKey={fKey}
                                onChange={(newKey) =>
                                  handleModelExtraFieldKeyChange(
                                    key,
                                    fKey,
                                    newKey,
                                  )
                                }
                                placeholder={t(
                                  "opencode.modelExtraFieldKeyPlaceholder",
                                  {
                                    defaultValue: "variants",
                                  },
                                )}
                              />
                              <Input
                                value={fValue}
                                onChange={(e) =>
                                  handleModelExtraFieldValueChange(
                                    key,
                                    fKey,
                                    e.target.value,
                                  )
                                }
                                placeholder={t(
                                  "opencode.modelOptionValuePlaceholder",
                                  {
                                    defaultValue: '{"order": ["baseten"]}',
                                  },
                                )}
                                className="flex-1"
                              />
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon"
                                onClick={() =>
                                  handleRemoveModelExtraField(key, fKey)
                                }
                                className="h-9 w-9 text-muted-foreground hover:text-destructive"
                              >
                                <Trash2 className="h-4 w-4" />
                              </Button>
                            </div>
                          ),
                        )
                      )}
                    </div>

                    {/* SDK Options (model.options) */}
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-medium text-muted-foreground">
                          {t("opencode.sdkOptions", {
                            defaultValue: "SDK 选项",
                          })}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => handleAddModelOption(key)}
                          className="h-6 px-2 gap-1"
                        >
                          <Plus className="h-3 w-3" />
                        </Button>
                      </div>
                      {Object.keys(model.options || {}).length === 0 ? (
                        <p className="text-xs text-muted-foreground py-1">
                          {t("opencode.noModelOptions", {
                            defaultValue: "模型选项，点击 + 添加",
                          })}
                        </p>
                      ) : (
                        Object.entries(model.options || {}).map(
                          ([optKey, optValue]) => (
                            <div
                              key={optKey}
                              className="flex items-center gap-2"
                            >
                              <ModelOptionKeyInput
                                optionKey={optKey}
                                onChange={(newKey) =>
                                  handleModelOptionKeyChange(
                                    key,
                                    optKey,
                                    newKey,
                                  )
                                }
                                placeholder={t(
                                  "opencode.modelOptionKeyPlaceholder",
                                  {
                                    defaultValue: "provider",
                                  },
                                )}
                              />
                              <Input
                                value={
                                  typeof optValue === "string"
                                    ? optValue
                                    : JSON.stringify(optValue)
                                }
                                onChange={(e) =>
                                  handleModelOptionValueChange(
                                    key,
                                    optKey,
                                    e.target.value,
                                  )
                                }
                                placeholder={t(
                                  "opencode.modelOptionValuePlaceholder",
                                  {
                                    defaultValue: '{"order": ["baseten"]}',
                                  },
                                )}
                                className="flex-1"
                              />
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon"
                                onClick={() =>
                                  handleRemoveModelOption(key, optKey)
                                }
                                className="h-9 w-9 text-muted-foreground hover:text-destructive"
                              >
                                <Trash2 className="h-4 w-4" />
                              </Button>
                            </div>
                          ),
                        )
                      )}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}

        <p className="text-xs text-muted-foreground">
          {t("opencode.modelsHint", {
            defaultValue:
              "Configure available models. Model ID is the API identifier, Display Name is shown in the UI.",
          })}
        </p>
      </ProviderFormSection>
    </>
  );
}
