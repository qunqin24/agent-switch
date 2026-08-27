import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { FormLabel } from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toast } from "sonner";
import {
  ChevronDown,
  ChevronRight,
  Download,
  Link2,
  Loader2,
  Plus,
  SlidersHorizontal,
  Sparkles,
  Trash2,
} from "lucide-react";
import EndpointSpeedTest from "./EndpointSpeedTest";
import { ApiKeySection, EndpointField, ModelDropdown } from "./shared";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import { CustomUserAgentField } from "./CustomUserAgentField";
import { ProviderFormSection } from "./ProviderFormSection";
import { modelsDevCacheApi } from "@/lib/api/modelsDev";
import type { ModelsDevCatalogModel } from "@/lib/modelsDevCatalog";
import {
  backfillCodexCatalogContextWindow,
  formatModelsDevModalities,
  getCodexReasoningLevelOptions,
  getModelsDevCapabilityFlags,
  getModelsDevContextWindow,
  selectCodexRowsNeedingContextWindow,
  type ModelsDevCapabilityFlag,
} from "./helpers/codexCatalogUtils";
import { cn } from "@/lib/utils";
import type {
  CodexApiFormat,
  CodexCatalogModel,
  CodexChatReasoning,
  ProviderCategory,
} from "@/types";

interface EndpointCandidate {
  url: string;
}

interface CodexFormFieldsProps {
  providerId?: string;
  // API Key
  codexApiKey: string;
  onApiKeyChange: (key: string) => void;
  category?: ProviderCategory;
  shouldShowApiKeyLink: boolean;
  websiteUrl: string;
  isPartner?: boolean;
  partnerPromotionKey?: string;

  // Base URL
  shouldShowSpeedTest: boolean;
  codexBaseUrl: string;
  onBaseUrlChange: (url: string) => void;
  isFullUrl: boolean;
  onFullUrlChange: (value: boolean) => void;
  isEndpointModalOpen: boolean;
  onEndpointModalToggle: (open: boolean) => void;
  onCustomEndpointsChange?: (endpoints: string[]) => void;
  autoSelect: boolean;
  onAutoSelectChange: (checked: boolean) => void;

  // API Format
  // Note: wire_api is always "responses" for Codex; apiFormat controls proxy-layer conversion
  apiFormat: CodexApiFormat;
  onApiFormatChange: (format: CodexApiFormat) => void;
  codexChatReasoning?: CodexChatReasoning;
  onCodexChatReasoningChange?: (value: CodexChatReasoning) => void;

  // Model Catalog
  catalogModels?: CodexCatalogModel[];
  onCatalogModelsChange?: (models: CodexCatalogModel[]) => void;

  // Speed Test Endpoints
  speedTestEndpoints: EndpointCandidate[];

  // Local proxy User-Agent override
  customUserAgent: string;
  onCustomUserAgentChange: (value: string) => void;
}

type CodexCatalogRow = CodexCatalogModel & { rowId: string };

/** Radix Select forbids an empty value, so "inherit the template" needs a sentinel. */
const CODEX_REASONING_LEVEL_INHERIT = "__inherit__";

/** Reuses the OpenCode panel's capability labels. */
const MODELS_DEV_CAPABILITY_LABELS: Record<
  ModelsDevCapabilityFlag,
  [key: string, fallback: string]
> = {
  attachment: ["opencode.modelsDevCapabilityAttachment", "附件"],
  reasoning: ["opencode.modelsDevCapabilityReasoning", "思考"],
  tool_call: ["opencode.modelsDevCapabilityToolCall", "工具调用"],
  structured_output: [
    "opencode.modelsDevCapabilityStructuredOutput",
    "结构化输出",
  ],
  temperature: ["opencode.modelsDevCapabilityTemperature", "温度"],
};

function createCatalogRow(seed?: Partial<CodexCatalogModel>): CodexCatalogRow {
  return {
    rowId: crypto.randomUUID(),
    model: seed?.model ?? "",
    displayName: seed?.displayName ?? "",
    contextWindow: seed?.contextWindow ?? "",
    defaultReasoningLevel: seed?.defaultReasoningLevel ?? "",
  };
}

// Compares rows (with rowId) to incoming models (without) by data fields only,
// so both sync effects can use the same equality definition.
function catalogRowsMatchModels(
  rows: Array<
    Pick<
      CodexCatalogRow,
      "model" | "displayName" | "contextWindow" | "defaultReasoningLevel"
    >
  >,
  models: CodexCatalogModel[],
): boolean {
  if (rows.length !== models.length) return false;
  return rows.every((row, i) => {
    const incoming = models[i];
    return (
      row.model === (incoming.model ?? "") &&
      (row.displayName ?? "") === (incoming.displayName ?? "") &&
      String(row.contextWindow ?? "") ===
        String(incoming.contextWindow ?? "") &&
      (row.defaultReasoningLevel ?? "") ===
        (incoming.defaultReasoningLevel ?? "")
    );
  });
}

export function CodexFormFields({
  providerId,
  codexApiKey,
  onApiKeyChange,
  category,
  shouldShowApiKeyLink,
  websiteUrl,
  isPartner,
  partnerPromotionKey,
  shouldShowSpeedTest,
  codexBaseUrl,
  onBaseUrlChange,
  isFullUrl,
  onFullUrlChange,
  isEndpointModalOpen,
  onEndpointModalToggle,
  onCustomEndpointsChange,
  autoSelect,
  onAutoSelectChange,
  apiFormat,
  onApiFormatChange,
  codexChatReasoning = {},
  onCodexChatReasoningChange,
  catalogModels = [],
  onCatalogModelsChange,
  speedTestEndpoints,
  customUserAgent,
  onCustomUserAgentChange,
}: CodexFormFieldsProps) {
  const { t } = useTranslation();

  const [fetchedModels, setFetchedModels] = useState<FetchedModel[]>([]);
  const [isFetchingModels, setIsFetchingModels] = useState(false);
  const [isAutoFillingContext, setIsAutoFillingContext] = useState(false);
  const needsLocalRouting = apiFormat === "openai_chat";
  const canEditCatalog = Boolean(onCatalogModelsChange);
  const canEditReasoning = Boolean(onCodexChatReasoningChange);
  const supportsThinking =
    codexChatReasoning.supportsThinking === true ||
    codexChatReasoning.supportsEffort === true;
  const supportsEffort = codexChatReasoning.supportsEffort === true;
  const isDeepSeekNative = /^https:\/\/api\.deepseek\.com(?:\/|$)/i.test(
    codexBaseUrl.trim(),
  );

  // needsLocalRouting 非默认值说明预设/用户动过路由配置，需要让模型映射保持可见
  const hasAnyAdvancedValue =
    !!customUserAgent || needsLocalRouting || catalogModels.length > 0;
  const [advancedExpanded, setAdvancedExpanded] = useState(hasAnyAdvancedValue);

  // 预设/编辑加载填充高级值后自动展开（仅从折叠→展开，不会自动折叠）
  useEffect(() => {
    if (hasAnyAdvancedValue) {
      setAdvancedExpanded(true);
    }
  }, [hasAnyAdvancedValue]);

  const [catalogRows, setCatalogRows] = useState<CodexCatalogRow[]>(() =>
    catalogModels.map((m) => createCatalogRow(m)),
  );

  // 记录上次发送给父组件的数据，避免重复触发
  const lastSentModelsRef = useRef<CodexCatalogModel[]>(catalogModels);

  // 异步回填时需要读到最新行数据（拉取模型列表后批量回填）
  const catalogRowsRef = useRef(catalogRows);
  useEffect(() => {
    catalogRowsRef.current = catalogRows;
  }, [catalogRows]);

  // 同一模型在多行/多次触发时只查一次 models.dev 缓存
  const metadataCacheRef = useRef(
    new Map<string, ModelsDevCatalogModel | null>(),
  );
  // 展开的行详情面板（对齐 OpenCode 的展开交互）
  const [expandedRows, setExpandedRows] = useState<Set<string>>(new Set());
  const [rowMetadata, setRowMetadata] = useState<
    Record<string, ModelsDevCatalogModel | null>
  >({});
  const [metadataLoadingRows, setMetadataLoadingRows] = useState<Set<string>>(
    new Set(),
  );

  /**
   * 查询 models.dev 元数据并记到该行，用于详情面板展示。
   * 查不到 / 读取失败都返回 null（静默降级）。`ignoreInMemoryMetadata` 供
   * 「自动配置」按钮使用：绕过组件内缓存重新查询，这样用户刚在设置里刷新过
   * models.dev 缓存后能立即生效。
   */
  const lookupRowMetadata = useCallback(
    async (
      rowId: string,
      modelId: string,
      displayName?: string,
      ignoreInMemoryMetadata = false,
    ): Promise<ModelsDevCatalogModel | null> => {
      const name = displayName?.trim() || undefined;
      const cacheKey = `${modelId}\u0000${name ?? ""}`;
      const cache = metadataCacheRef.current;

      if (!ignoreInMemoryMetadata && cache.has(cacheKey)) {
        const cached = cache.get(cacheKey) ?? null;
        setRowMetadata((prev) => ({ ...prev, [rowId]: cached }));
        return cached;
      }

      setMetadataLoadingRows((prev) => new Set(prev).add(rowId));
      try {
        const metadata =
          (await modelsDevCacheApi.getModelMetadata(modelId, name)) ?? null;
        cache.set(cacheKey, metadata);
        setRowMetadata((prev) => ({ ...prev, [rowId]: metadata }));
        return metadata;
      } catch (error) {
        // 元数据缓存缺失/读取失败时静默降级，不影响模型列表本身
        console.warn("[Models.dev] Codex catalog lookup failed:", error);
        setRowMetadata((prev) => ({ ...prev, [rowId]: null }));
        return null;
      } finally {
        setMetadataLoadingRows((prev) => {
          const next = new Set(prev);
          next.delete(rowId);
          return next;
        });
      }
    },
    [],
  );

  /**
   * 用 models.dev 元数据回填「上下文窗口」。
   * 仅在该列为空时写入；查不到 / 读取失败都静默跳过，保持现状。
   * 返回真正用于回填的上下文长度（未命中时为 undefined）。
   */
  const backfillContextWindow = useCallback(
    async (
      rowId: string,
      model: string,
      displayName?: string,
      ignoreInMemoryMetadata = false,
    ): Promise<number | undefined> => {
      const modelId = model.trim();
      if (!modelId) return undefined;

      const metadata = await lookupRowMetadata(
        rowId,
        modelId,
        displayName,
        ignoreInMemoryMetadata,
      );
      const contextWindow = getModelsDevContextWindow(metadata);
      if (!contextWindow) return undefined;

      setCatalogRows((current) =>
        backfillCodexCatalogContextWindow(current, {
          rowId,
          model: modelId,
          contextWindow,
        }),
      );
      return contextWindow;
    },
    [lookupRowMetadata],
  );

  /**
   * 「自动配置」按钮：对表内所有「有模型、上下文为空」的行强制重查 models.dev。
   * 与 OpenCode 表单的自动配置一致：忽略内存缓存、逐行查询、结束后 toast 反馈。
   */
  const handleAutoFillContextWindows = useCallback(async () => {
    const targets = selectCodexRowsNeedingContextWindow(catalogRowsRef.current);
    if (targets.length === 0) {
      toast.info(
        t("codexConfig.autoFillContextNoTargets", {
          defaultValue: "没有需要回填的模型：请先填写实际请求模型",
        }),
      );
      return;
    }

    setIsAutoFillingContext(true);
    try {
      const results = await Promise.all(
        targets.map((row) =>
          backfillContextWindow(row.rowId, row.model, row.displayName, true),
        ),
      );
      const filled = results.filter(Boolean).length;
      if (filled > 0) {
        toast.success(
          t("codexConfig.autoFillContextFilled", {
            count: filled,
            defaultValue: "已按 Models.dev 回填 {{count}} 个模型的上下文窗口",
          }),
        );
      } else {
        toast.info(
          t("codexConfig.autoFillContextNotFound", {
            defaultValue: "Models.dev 未提供这些模型的上下文长度，请手动填写",
          }),
        );
      }
    } finally {
      setIsAutoFillingContext(false);
    }
  }, [backfillContextWindow, t]);

  // 展开某一行时顺带读取该行的 models.dev 元数据（对齐 OpenCode 的展开行为）
  const handleToggleRowExpand = useCallback(
    (row: CodexCatalogRow) => {
      const willExpand = !expandedRows.has(row.rowId);
      setExpandedRows((prev) => {
        const next = new Set(prev);
        if (next.has(row.rowId)) next.delete(row.rowId);
        else next.add(row.rowId);
        return next;
      });
      if (willExpand && row.model.trim()) {
        void lookupRowMetadata(row.rowId, row.model.trim(), row.displayName);
      }
    },
    [expandedRows, lookupRowMetadata],
  );

  // 行内「自动配置」：强制重查该行的 models.dev 元数据并回填上下文窗口
  const handleAutoConfigureRow = useCallback(
    async (row: CodexCatalogRow) => {
      const modelId = row.model.trim();
      if (!modelId) {
        toast.info(
          t("codexConfig.autoFillContextNoTargets", {
            defaultValue: "没有需要回填的模型：请先填写实际请求模型",
          }),
        );
        return;
      }
      const contextWindow = await backfillContextWindow(
        row.rowId,
        modelId,
        row.displayName,
        true,
      );
      if (contextWindow) {
        toast.success(
          t("codexConfig.autoFillContextFilled", {
            count: 1,
            defaultValue: "已按 Models.dev 回填 {{count}} 个模型的上下文窗口",
          }),
        );
      } else {
        toast.info(
          t("codexConfig.autoFillContextNotFound", {
            defaultValue: "Models.dev 未提供这些模型的上下文长度，请手动填写",
          }),
        );
      }
    },
    [backfillContextWindow, t],
  );

  // 父 → 子：仅当 prop 数据真的变化（预设切换 / 编辑加载）时才重建 rowId；
  // 同 shape 时保留现有 rowId，避免编辑过程中焦点丢失。
  useEffect(() => {
    setCatalogRows((current) => {
      if (catalogRowsMatchModels(current, catalogModels)) return current;
      return catalogModels.map((m) => createCatalogRow(m));
    });
    // 同步更新 ref，避免父组件传入新数据时子→父 effect 误判为本地修改
    lastSentModelsRef.current = catalogModels;
  }, [catalogModels]);

  // 子 → 父：rowId 是视图层概念，不应进入持久化数据；剥离后再回传。
  // 注意：依赖数组不包含 catalogModels，避免父→子更新触发子→父回调形成循环。
  useEffect(() => {
    if (!onCatalogModelsChange) return;
    const next: CodexCatalogModel[] = catalogRows.map(
      ({ rowId: _rowId, ...rest }) => rest,
    );
    // 只有当数据真的变化时才通知父组件
    if (catalogRowsMatchModels(catalogRows, lastSentModelsRef.current)) return;
    lastSentModelsRef.current = next;
    onCatalogModelsChange(next);
  }, [catalogRows, onCatalogModelsChange]);

  const handleLocalRoutingChange = useCallback(
    (checked: boolean) => {
      onApiFormatChange(checked ? "openai_chat" : "openai_responses");
    },
    [onApiFormatChange],
  );

  const handleReasoningThinkingChange = useCallback(
    (checked: boolean) => {
      if (!onCodexChatReasoningChange) return;
      onCodexChatReasoningChange({
        ...codexChatReasoning,
        supportsThinking: checked,
        supportsEffort: checked ? codexChatReasoning.supportsEffort : false,
      });
    },
    [codexChatReasoning, onCodexChatReasoningChange],
  );

  const handleReasoningEffortChange = useCallback(
    (checked: boolean) => {
      if (!onCodexChatReasoningChange) return;
      onCodexChatReasoningChange({
        ...codexChatReasoning,
        supportsThinking: checked ? true : codexChatReasoning.supportsThinking,
        supportsEffort: checked,
        effortParam: checked
          ? (codexChatReasoning.effortParam ?? "reasoning_effort")
          : "none",
      });
    },
    [codexChatReasoning, onCodexChatReasoningChange],
  );

  const handleFetchModels = useCallback(() => {
    if (!codexBaseUrl || !codexApiKey) {
      showFetchModelsError(null, t, {
        hasApiKey: !!codexApiKey,
        hasBaseUrl: !!codexBaseUrl,
      });
      return;
    }
    setIsFetchingModels(true);
    fetchModelsForConfig(
      codexBaseUrl,
      codexApiKey,
      isFullUrl,
      undefined,
      customUserAgent,
    )
      .then((models) => {
        setFetchedModels(models);
        if (models.length === 0) {
          toast.info(t("providerForm.fetchModelsEmpty"));
        } else {
          toast.success(
            t("providerForm.fetchModelsSuccess", { count: models.length }),
          );
        }
        // 拉到模型后顺带补全仍为空的上下文窗口（失败静默）
        for (const row of selectCodexRowsNeedingContextWindow(
          catalogRowsRef.current,
        )) {
          void backfillContextWindow(row.rowId, row.model, row.displayName);
        }
      })
      .catch((err) => {
        console.warn("[ModelFetch] Failed:", err);
        showFetchModelsError(err, t);
      })
      .finally(() => setIsFetchingModels(false));
  }, [
    codexBaseUrl,
    codexApiKey,
    isFullUrl,
    customUserAgent,
    t,
    backfillContextWindow,
  ]);

  const handleAddCatalogRow = useCallback(() => {
    if (!onCatalogModelsChange) return;
    setCatalogRows((current) => [...current, createCatalogRow()]);
  }, [onCatalogModelsChange]);

  const handleUpdateCatalogRow = useCallback(
    (index: number, patch: Partial<CodexCatalogModel>) => {
      setCatalogRows((current) =>
        current.map((row, i) => (i === index ? { ...row, ...patch } : row)),
      );
    },
    [],
  );

  const handleRemoveCatalogRow = useCallback((index: number) => {
    setCatalogRows((current) => current.filter((_, i) => i !== index));
  }, []);

  const renderCatalogRowDetails = (row: CodexCatalogRow, index: number) => {
    const modelId = row.model.trim();
    const metadata = rowMetadata[row.rowId];
    const isLoadingMetadata = metadataLoadingRows.has(row.rowId);
    const capabilities = [
      ...getModelsDevCapabilityFlags(metadata).map((flag) =>
        t(MODELS_DEV_CAPABILITY_LABELS[flag][0], {
          defaultValue: MODELS_DEV_CAPABILITY_LABELS[flag][1],
        }),
      ),
      ...(formatModelsDevModalities(metadata)
        ? [formatModelsDevModalities(metadata) as string]
        : []),
    ].join(" · ");
    const levelOptions = getCodexReasoningLevelOptions(
      metadata,
      row.defaultReasoningLevel,
    );

    return (
      <div className="space-y-3 md:ml-9 md:border-l-2 md:border-muted md:pl-4">
        {/* Models.dev 元数据摘要（对齐 OpenCode 展开面板顶部的能力块） */}
        <div className="space-y-2 rounded-md border border-border/60 bg-muted/20 p-3">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0 space-y-0.5">
              <span className="text-xs font-medium text-foreground">
                {t("codexConfig.catalogRowMetadataTitle", {
                  defaultValue: "Models.dev 元数据",
                })}
              </span>
              {metadata ? (
                <>
                  <p
                    className={cn(
                      "text-xs",
                      metadata.reasoning
                        ? "text-emerald-600 dark:text-emerald-400"
                        : "text-muted-foreground",
                    )}
                  >
                    {metadata.reasoning
                      ? t("opencode.modelsDevReasoningSupported", {
                          defaultValue: "Models.dev：支持思考",
                        })
                      : t("opencode.modelsDevReasoningUnsupported", {
                          defaultValue: "Models.dev：未标记思考能力",
                        })}
                  </p>
                  {capabilities && (
                    <p className="text-xs text-muted-foreground">
                      {t("opencode.modelsDevCapabilitySummary", {
                        capabilities,
                        defaultValue: "能力：{{capabilities}}",
                      })}
                    </p>
                  )}
                </>
              ) : isLoadingMetadata ? (
                <p className="text-xs text-muted-foreground">
                  {t("opencode.modelsDevLoadingCapabilities", {
                    defaultValue: "正在读取 Models.dev 模型能力...",
                  })}
                </p>
              ) : (
                <p className="text-xs text-muted-foreground">
                  {modelId
                    ? t("opencode.modelsDevModelNotFound", {
                        model: modelId,
                        defaultValue:
                          "Models.dev 中未找到 {{model}}；请检查模型 ID 或手动配置",
                      })
                    : t("codexConfig.catalogRowNeedsModel", {
                        defaultValue: "请先填写实际请求模型",
                      })}
                </p>
              )}
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void handleAutoConfigureRow(row)}
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
        </div>

        {/* catalog 条目里用户可覆盖的字段 */}
        <div className="space-y-3 rounded-md border border-border/60 bg-muted/20 p-3">
          <span className="text-xs font-medium text-foreground">
            {t("codexConfig.catalogRowSettingsTitle", {
              defaultValue: "条目设置",
            })}
          </span>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <div className="space-y-1.5">
              <span className="text-xs text-muted-foreground">
                {t("opencode.contextLimit", { defaultValue: "上下文" })}
              </span>
              <Input
                type="number"
                min={1}
                inputMode="numeric"
                className="h-9"
                value={row.contextWindow ?? ""}
                onChange={(event) =>
                  handleUpdateCatalogRow(index, {
                    contextWindow: event.target.value.replace(/[^\d]/g, ""),
                  })
                }
                placeholder={t("codexConfig.contextWindowPlaceholder", {
                  defaultValue: "例如: 128000",
                })}
                aria-label={t("opencode.contextLimit", {
                  defaultValue: "上下文",
                })}
              />
            </div>
            <div className="space-y-1.5">
              <span className="text-xs text-muted-foreground">
                {t("opencode.thinkingLevel", { defaultValue: "默认思考等级" })}
              </span>
              <Select
                value={
                  row.defaultReasoningLevel?.trim() ||
                  CODEX_REASONING_LEVEL_INHERIT
                }
                onValueChange={(value) =>
                  handleUpdateCatalogRow(index, {
                    defaultReasoningLevel:
                      value === CODEX_REASONING_LEVEL_INHERIT ? "" : value,
                  })
                }
              >
                <SelectTrigger
                  className="h-9"
                  aria-label={t("opencode.thinkingLevel", {
                    defaultValue: "默认思考等级",
                  })}
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={CODEX_REASONING_LEVEL_INHERIT}>
                    {t("codexConfig.catalogReasoningLevelInherit", {
                      defaultValue: "跟随模型模板",
                    })}
                  </SelectItem>
                  {levelOptions.map((level) => (
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
          <p className="text-xs leading-relaxed text-muted-foreground">
            {t("codexConfig.catalogRowSettingsHint", {
              defaultValue:
                "上下文窗口写入 catalog 的 context_window / max_context_window，默认思考等级写入 default_reasoning_level（留空则沿用 Codex 模板）。其余字段由 Agent Switch 按 Codex 模板自动生成。",
            })}
          </p>
        </div>
      </div>
    );
  };

  const renderCatalogActionButtons = (onAdd: () => void, addLabel: string) => (
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
      {/* 复用 OpenCode 自动配置按钮的文案与图标 */}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => void handleAutoFillContextWindows()}
        disabled={isAutoFillingContext}
        className="h-7 gap-1"
        title={t("codexConfig.autoFillContextHint", {
          defaultValue: "按 Models.dev 元数据回填空白的上下文窗口",
        })}
      >
        {isAutoFillingContext ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <Sparkles className="h-3.5 w-3.5" />
        )}
        {t("opencode.autoConfigureThinking", {
          defaultValue: "自动配置",
        })}
      </Button>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={onAdd}
        className="h-7 gap-1"
      >
        <Plus className="h-3.5 w-3.5" />
        {addLabel}
      </Button>
    </div>
  );

  return (
    <>
      <ProviderFormSection
        sectionKey="connection"
        icon={Link2}
        title={t("providerForm.connectionSection")}
      >
        {/* Codex API Key 输入框 */}
        <ApiKeySection
          id="codexApiKey"
          label="API Key"
          value={codexApiKey}
          onChange={onApiKeyChange}
          category={category}
          shouldShowLink={shouldShowApiKeyLink}
          websiteUrl={websiteUrl}
          isPartner={isPartner}
          partnerPromotionKey={partnerPromotionKey}
          placeholder={{
            official: t("providerForm.codexOfficialNoApiKey", {
              defaultValue: "官方供应商无需 API Key",
            }),
            thirdParty: isDeepSeekNative
              ? t("providerForm.codexDeepSeekApiKeyAutoFill", {
                  defaultValue:
                    "只需要填这里，启用后会自动写入 config.toml 的 experimental_bearer_token",
                })
              : t("providerForm.codexApiKeyAutoFill", {
                  defaultValue: "输入 API Key，将自动填充到配置",
                }),
          }}
        />

        {/* Codex Base URL 输入框 */}
        {shouldShowSpeedTest && (
          <EndpointField
            id="codexBaseUrl"
            label={t("codexConfig.apiUrlLabel")}
            value={codexBaseUrl}
            onChange={onBaseUrlChange}
            placeholder={t("providerForm.codexApiEndpointPlaceholder")}
            hint={t("providerForm.codexApiHint")}
            showFullUrlToggle
            isFullUrl={isFullUrl}
            onFullUrlChange={onFullUrlChange}
            onManageClick={() => onEndpointModalToggle(true)}
          />
        )}
      </ProviderFormSection>

      {/* 高级选项 —— 本地路由映射/模型映射/思考能力/自定义 UA；预设供应商通常无需展开 */}
      {category !== "official" && (
        <Collapsible open={advancedExpanded} onOpenChange={setAdvancedExpanded}>
          <ProviderFormSection
            sectionKey="options"
            icon={SlidersHorizontal}
            title={t("providerForm.advancedOptionsToggle", {
              defaultValue: "高级选项",
            })}
            actions={
              <CollapsibleTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-muted-foreground"
                >
                  {advancedExpanded ? (
                    <ChevronDown className="h-4 w-4" />
                  ) : (
                    <ChevronRight className="h-4 w-4" />
                  )}
                  <span className="sr-only">
                    {t("providerForm.advancedOptionsToggle", {
                      defaultValue: "高级选项",
                    })}
                  </span>
                </Button>
              </CollapsibleTrigger>
            }
            contentClassName="space-y-3"
          >
            {!advancedExpanded && (
              <p className="text-xs text-muted-foreground">
                {t("codexConfig.advancedSectionHint", {
                  defaultValue:
                    "包含本地路由映射、模型目录、思考能力与自定义 User-Agent。上游原生兼容 Codex Responses API 时无需开启本地路由。",
                })}
              </p>
            )}
            <CollapsibleContent className="space-y-3">
              {/* 本地路由映射开关 —— 沿用 shouldShowSpeedTest 门控，cloud_provider 保持不可切换 */}
              {shouldShowSpeedTest && (
                <div className="flex items-center justify-between gap-4">
                  <div className="space-y-1">
                    <FormLabel>
                      {t("codexConfig.localRoutingToggle", {
                        defaultValue: "需要本地路由映射",
                      })}
                    </FormLabel>
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      {needsLocalRouting
                        ? t("codexConfig.localRoutingOnHint", {
                            defaultValue:
                              "Agent Switch 会把 Codex Responses 请求转换为 Chat Completions；使用期间需要保持本地路由运行。",
                          })
                        : t("codexConfig.localRoutingOffHint", {
                            defaultValue:
                              "上游原生兼容 Codex Responses API 时可直接连接；仅当上游只支持 Chat Completions，或其 Responses 实现与 Codex 不兼容时开启。",
                          })}
                    </p>
                  </div>
                  <Switch
                    checked={needsLocalRouting}
                    onCheckedChange={handleLocalRoutingChange}
                    aria-label={t("codexConfig.localRoutingToggle", {
                      defaultValue: "需要本地路由映射",
                    })}
                  />
                </div>
              )}

              {needsLocalRouting && canEditReasoning && (
                <div
                  className={cn(
                    "space-y-3",
                    shouldShowSpeedTest &&
                      "border-t border-border-default pt-3",
                  )}
                >
                  <div className="space-y-1">
                    <FormLabel>
                      {t("codexConfig.reasoningGroupTitle", {
                        defaultValue: "思考能力",
                      })}
                    </FormLabel>
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      {t("codexConfig.reasoningSectionHint", {
                        defaultValue:
                          "预设供应商已自动配置；自定义供应商会按名称/地址自动推断。仅当自动识别不准时才需手动覆盖。",
                      })}
                    </p>
                  </div>

                  <div className="flex items-center justify-between gap-4">
                    <div className="space-y-1">
                      <FormLabel>
                        {t("codexConfig.reasoningModeToggle", {
                          defaultValue: "支持思考模式",
                        })}
                      </FormLabel>
                      <p className="text-xs leading-relaxed text-muted-foreground">
                        {t("codexConfig.reasoningModeHint", {
                          defaultValue:
                            "上游 Chat Completions 接口支持开启或关闭 thinking 时启用。Kimi、GLM、Qwen 等通常属于这一类。",
                        })}
                      </p>
                    </div>
                    <Switch
                      checked={supportsThinking}
                      onCheckedChange={handleReasoningThinkingChange}
                      aria-label={t("codexConfig.reasoningModeToggle", {
                        defaultValue: "支持思考模式",
                      })}
                    />
                  </div>

                  <div className="flex items-center justify-between gap-4 border-t border-border-default pt-3">
                    <div className="space-y-1">
                      <FormLabel>
                        {t("codexConfig.reasoningEffortToggle", {
                          defaultValue: "支持思考等级",
                        })}
                      </FormLabel>
                      <p className="text-xs leading-relaxed text-muted-foreground">
                        {t("codexConfig.reasoningEffortHint", {
                          defaultValue:
                            "上游支持 low/high/max 等思考深度控制时启用。启用后会自动启用思考模式，并把 Codex 的 reasoning.effort 转成上游 Chat 参数。",
                        })}
                      </p>
                    </div>
                    <Switch
                      checked={supportsEffort}
                      onCheckedChange={handleReasoningEffortChange}
                      aria-label={t("codexConfig.reasoningEffortToggle", {
                        defaultValue: "支持思考等级",
                      })}
                    />
                  </div>
                </div>
              )}

              <div
                className={cn(
                  (shouldShowSpeedTest ||
                    (needsLocalRouting && canEditReasoning)) &&
                    "border-t border-border-default pt-3",
                )}
              >
                <CustomUserAgentField
                  id="codex-custom-user-agent"
                  value={customUserAgent}
                  onChange={onCustomUserAgentChange}
                />
              </div>

              {/* 原生 Responses 的第三方模型也需要目录，不能只在本地路由时显示。 */}
              {canEditCatalog && (
                <div className="space-y-4 border-t border-border-default pt-3">
                  <div className="space-y-1">
                    <div className="flex items-center justify-between gap-3">
                      <FormLabel>
                        {t("codexConfig.modelMappingTitle", {
                          defaultValue: "模型映射",
                        })}
                      </FormLabel>
                      {renderCatalogActionButtons(
                        handleAddCatalogRow,
                        t("codexConfig.addCatalogModel", {
                          defaultValue: "添加模型",
                        }),
                      )}
                    </div>
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      {t("codexConfig.modelMappingHint", {
                        defaultValue:
                          "生成 Codex model_catalog_json，让 /model 命令显示这些第三方模型名；表中条目按填写内容原样保存。修改后需要重启 Codex 才能刷新模型列表。",
                      })}
                    </p>
                  </div>

                  {catalogRows.length > 0 && (
                    <div className="space-y-2">
                      {/* 列头：md+ 显示 */}
                      <div className="hidden grid-cols-[36px_1fr_1fr_140px_36px] gap-2 px-1 text-xs font-medium text-muted-foreground md:grid">
                        <span />
                        <span>
                          {t("codexConfig.catalogColumnDisplay", {
                            defaultValue: "菜单显示名",
                          })}
                        </span>
                        <span>
                          {t("codexConfig.catalogColumnModel", {
                            defaultValue: "实际请求模型",
                          })}
                        </span>
                        <span>
                          {t("codexConfig.catalogColumnContext", {
                            defaultValue: "上下文窗口",
                          })}
                        </span>
                        <span />
                      </div>

                      {catalogRows.map((row, index) => (
                        <div key={row.rowId} className="space-y-2">
                          <div className="grid grid-cols-1 gap-2 md:grid-cols-[36px_1fr_1fr_140px_36px]">
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              className="h-9 w-9 shrink-0"
                              onClick={() => handleToggleRowExpand(row)}
                              aria-expanded={expandedRows.has(row.rowId)}
                              title={t("codexConfig.catalogRowDetails", {
                                defaultValue: "模型详情",
                              })}
                            >
                              <ChevronRight
                                className={cn(
                                  "h-4 w-4 transition-transform",
                                  expandedRows.has(row.rowId) && "rotate-90",
                                )}
                              />
                              <span className="sr-only">
                                {t("codexConfig.catalogRowDetails", {
                                  defaultValue: "模型详情",
                                })}
                              </span>
                            </Button>
                            <Input
                              value={row.displayName ?? ""}
                              onChange={(event) =>
                                handleUpdateCatalogRow(index, {
                                  displayName: event.target.value,
                                })
                              }
                              placeholder={t(
                                "codexConfig.catalogDisplayNamePlaceholder",
                                {
                                  defaultValue: "例如: DeepSeek V4 Flash",
                                },
                              )}
                              aria-label={t(
                                "codexConfig.catalogColumnDisplay",
                                {
                                  defaultValue: "菜单显示名",
                                },
                              )}
                            />
                            <div className="flex gap-1">
                              <Input
                                value={row.model}
                                onChange={(event) =>
                                  handleUpdateCatalogRow(index, {
                                    model: event.target.value,
                                  })
                                }
                                onBlur={(event) =>
                                  void backfillContextWindow(
                                    row.rowId,
                                    event.target.value,
                                    row.displayName,
                                  )
                                }
                                placeholder={t(
                                  "codexConfig.catalogModelPlaceholder",
                                  {
                                    defaultValue: "例如: deepseek-v4-flash",
                                  },
                                )}
                                aria-label={t(
                                  "codexConfig.catalogColumnModel",
                                  {
                                    defaultValue: "实际请求模型",
                                  },
                                )}
                                className="flex-1"
                              />
                              {fetchedModels.length > 0 && (
                                <ModelDropdown
                                  models={fetchedModels}
                                  onSelect={(id) => {
                                    const displayName = row.displayName?.trim()
                                      ? row.displayName
                                      : id;
                                    handleUpdateCatalogRow(index, {
                                      model: id,
                                      displayName,
                                    });
                                    void backfillContextWindow(
                                      row.rowId,
                                      id,
                                      displayName,
                                    );
                                  }}
                                />
                              )}
                            </div>
                            <Input
                              type="number"
                              min={1}
                              inputMode="numeric"
                              value={row.contextWindow ?? ""}
                              onChange={(event) =>
                                handleUpdateCatalogRow(index, {
                                  contextWindow: event.target.value.replace(
                                    /[^\d]/g,
                                    "",
                                  ),
                                })
                              }
                              placeholder={t(
                                "codexConfig.contextWindowPlaceholder",
                                {
                                  defaultValue: "例如: 128000",
                                },
                              )}
                              aria-label={t(
                                "codexConfig.catalogColumnContext",
                                {
                                  defaultValue: "上下文窗口",
                                },
                              )}
                            />
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              className="h-9 w-9 text-muted-foreground hover:text-destructive"
                              onClick={() => handleRemoveCatalogRow(index)}
                              title={t("common.delete", {
                                defaultValue: "删除",
                              })}
                            >
                              <Trash2 className="h-4 w-4" />
                            </Button>
                          </div>

                          {expandedRows.has(row.rowId) &&
                            renderCatalogRowDetails(row, index)}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </CollapsibleContent>
          </ProviderFormSection>
        </Collapsible>
      )}

      {/* 端点测速弹窗 - Codex */}
      {shouldShowSpeedTest && isEndpointModalOpen && (
        <EndpointSpeedTest
          appId="codex"
          providerId={providerId}
          value={codexBaseUrl}
          onChange={onBaseUrlChange}
          initialEndpoints={speedTestEndpoints}
          visible={isEndpointModalOpen}
          onClose={() => onEndpointModalToggle(false)}
          autoSelect={autoSelect}
          onAutoSelectChange={onAutoSelectChange}
          onCustomEndpointsChange={onCustomEndpointsChange}
        />
      )}
    </>
  );
}
