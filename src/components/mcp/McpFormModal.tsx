import React, { useMemo, useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Save, Plus, AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import JsonEditor from "@/components/JsonEditor";
import type { McpAppId } from "@/lib/api/types";
import type { McpServerSpec } from "@/types";
import { mcpPresets, getMcpPresetWithDescription } from "@/config/mcpPresets";
import McpWizardModal from "./McpWizardModal";
import {
  extractErrorMessage,
  translateMcpBackendError,
} from "@/utils/errorUtils";
import {
  tomlToMcpServer,
  extractIdFromToml,
  mcpServerToToml,
} from "@/utils/tomlUtils";
import { normalizeTomlText } from "@/utils/textNormalization";
import { parseSmartMcpJson } from "@/utils/formatters";
import { useMcpValidation } from "./useMcpValidation";
import { useUpsertMcpServer } from "@/hooks/useMcp";
import { FullScreenPanel } from "@/components/common/FullScreenPanel";

export interface AppMcpServerEntry {
  id: string;
  server: McpServerSpec;
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.values(value).every((entry) => typeof entry === "string")
  );
}

function isMcpServerSpec(value: unknown): value is McpServerSpec {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  if (
    "type" in value &&
    value.type !== undefined &&
    value.type !== "stdio" &&
    value.type !== "http" &&
    value.type !== "sse"
  ) {
    return false;
  }
  if (
    "command" in value &&
    value.command !== undefined &&
    typeof value.command !== "string"
  ) {
    return false;
  }
  if (
    "url" in value &&
    value.url !== undefined &&
    typeof value.url !== "string"
  ) {
    return false;
  }
  if (
    "args" in value &&
    value.args !== undefined &&
    (!Array.isArray(value.args) ||
      !value.args.every((entry) => typeof entry === "string"))
  ) {
    return false;
  }
  if ("env" in value && value.env !== undefined && !isStringRecord(value.env)) {
    return false;
  }
  if (
    "headers" in value &&
    value.headers !== undefined &&
    !isStringRecord(value.headers)
  ) {
    return false;
  }
  return true;
}

interface McpFormModalProps {
  appId: McpAppId;
  editingId?: string;
  initialData?: AppMcpServerEntry;
  onSave: () => Promise<void>;
  onClose: () => void;
  existingIds?: string[];
  defaultFormat?: "json" | "toml";
}

const McpFormModal: React.FC<McpFormModalProps> = ({
  appId,
  editingId,
  initialData,
  onSave,
  onClose,
  existingIds = [],
  defaultFormat = "json",
}) => {
  const { t } = useTranslation();
  const { formatTomlError, validateTomlConfig, validateJsonConfig } =
    useMcpValidation();

  const upsertMutation = useUpsertMcpServer(appId);

  const [formId, setFormId] = useState(
    () => editingId || initialData?.id || "",
  );

  const isEditing = !!editingId;

  const useTomlFormat = defaultFormat === "toml";

  const [formConfig, setFormConfig] = useState(() => {
    const spec = initialData?.server;
    if (!spec) return "";
    if (useTomlFormat) {
      return mcpServerToToml(spec);
    }
    return JSON.stringify(spec, null, 2);
  });

  const [configError, setConfigError] = useState("");
  const [saving, setSaving] = useState(false);
  const [isWizardOpen, setIsWizardOpen] = useState(false);
  const [idError, setIdError] = useState("");
  const [isDarkMode, setIsDarkMode] = useState(false);

  useEffect(() => {
    setIsDarkMode(document.documentElement.classList.contains("dark"));

    const observer = new MutationObserver(() => {
      setIsDarkMode(document.documentElement.classList.contains("dark"));
    });

    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => observer.disconnect();
  }, []);

  const useToml = useTomlFormat;

  const wizardInitialSpec = useMemo(() => {
    const fallback = initialData?.server;
    if (!formConfig.trim()) {
      return fallback;
    }

    if (useToml) {
      try {
        return tomlToMcpServer(formConfig);
      } catch {
        return fallback;
      }
    }

    try {
      const parsed: unknown = JSON.parse(formConfig);
      if (isMcpServerSpec(parsed)) {
        return parsed;
      }
      return fallback;
    } catch {
      return fallback;
    }
  }, [formConfig, initialData, useToml]);

  const [selectedPreset, setSelectedPreset] = useState<number | null>(
    isEditing ? null : -1,
  );

  const handleIdChange = (value: string) => {
    setFormId(value);
    if (!isEditing) {
      const exists = existingIds.includes(value.trim());
      setIdError(exists ? t("mcp.error.idExists") : "");
    }
  };

  const ensureUniqueId = (base: string): string => {
    let candidate = base.trim();
    if (!candidate) candidate = "mcp-server";
    if (!existingIds.includes(candidate)) return candidate;
    let i = 1;
    while (existingIds.includes(`${candidate}-${i}`)) i++;
    return `${candidate}-${i}`;
  };

  const applyPreset = (index: number) => {
    if (index < 0 || index >= mcpPresets.length) return;
    const preset = mcpPresets[index];
    const presetWithDesc = getMcpPresetWithDescription(preset, t);

    const id = ensureUniqueId(presetWithDesc.id);
    setFormId(id);

    if (useToml) {
      const toml = mcpServerToToml(presetWithDesc.server);
      setFormConfig(toml);
      setConfigError(validateTomlConfig(toml));
    } else {
      const json = JSON.stringify(presetWithDesc.server, null, 2);
      setFormConfig(json);
      setConfigError(validateJsonConfig(json));
    }
    setSelectedPreset(index);
  };

  const applyCustom = () => {
    setSelectedPreset(-1);
    setFormId("");
    setFormConfig("");
    setConfigError("");
  };

  const handleConfigChange = (value: string) => {
    const nextValue = useToml ? normalizeTomlText(value) : value;
    setFormConfig(nextValue);

    if (useToml) {
      const err = validateTomlConfig(nextValue);
      if (err) {
        setConfigError(err);
        return;
      }

      if (nextValue.trim() && !formId.trim()) {
        const extractedId = extractIdFromToml(nextValue);
        if (extractedId) {
          setFormId(extractedId);
        }
      }
    } else {
      try {
        const result = parseSmartMcpJson(value);
        const configJson = JSON.stringify(result.config);
        const validationErr = validateJsonConfig(configJson);

        if (validationErr) {
          setConfigError(validationErr);
          return;
        }

        if (result.id && !formId.trim() && !isEditing) {
          const uniqueId = ensureUniqueId(result.id);
          setFormId(uniqueId);
        }

        setConfigError("");
      } catch (error: unknown) {
        const errorMessage = extractErrorMessage(error);
        setConfigError(t("mcp.error.jsonInvalid") + ": " + errorMessage);
      }
    }
  };

  const handleWizardApply = (title: string, json: string) => {
    setFormId(title);
    if (useToml) {
      try {
        const parsed: unknown = JSON.parse(json);
        if (!isMcpServerSpec(parsed)) {
          throw new Error(t("mcp.error.jsonInvalid"));
        }
        const server = parsed;
        const toml = mcpServerToToml(server);
        setFormConfig(toml);
        setConfigError(validateTomlConfig(toml));
      } catch {
        setConfigError(t("mcp.error.jsonInvalid"));
      }
    } else {
      setFormConfig(json);
      setConfigError(validateJsonConfig(json));
    }
  };

  const handleSubmit = async () => {
    const trimmedId = formId.trim();
    if (!trimmedId) {
      toast.error(t("mcp.error.idRequired"), { duration: 3000 });
      return;
    }

    if (!isEditing && existingIds.includes(trimmedId)) {
      setIdError(t("mcp.error.idExists"));
      return;
    }

    let serverSpec: McpServerSpec;

    if (useToml) {
      const tomlError = validateTomlConfig(formConfig);
      setConfigError(tomlError);
      if (tomlError) {
        toast.error(t("mcp.error.tomlInvalid"), { duration: 3000 });
        return;
      }

      if (!formConfig.trim()) {
        serverSpec = {
          type: "stdio",
          command: "",
          args: [],
        };
      } else {
        try {
          serverSpec = tomlToMcpServer(formConfig);
        } catch (error: unknown) {
          const msg = extractErrorMessage(error);
          setConfigError(formatTomlError(msg));
          toast.error(t("mcp.error.tomlInvalid"), { duration: 4000 });
          return;
        }
      }
    } else {
      if (!formConfig.trim()) {
        serverSpec = {
          type: "stdio",
          command: "",
          args: [],
        };
      } else {
        try {
          const result = parseSmartMcpJson(formConfig);
          if (!isMcpServerSpec(result.config)) {
            throw new Error(t("mcp.error.jsonInvalid"));
          }
          serverSpec = result.config;
        } catch (error: unknown) {
          const errorMessage = extractErrorMessage(error);
          setConfigError(t("mcp.error.jsonInvalid") + ": " + errorMessage);
          toast.error(t("mcp.error.jsonInvalid"), { duration: 4000 });
          return;
        }
      }
    }

    if (serverSpec?.type === "stdio" && !serverSpec?.command?.trim()) {
      toast.error(t("mcp.error.commandRequired"), { duration: 3000 });
      return;
    }
    if (
      (serverSpec?.type === "http" || serverSpec?.type === "sse") &&
      !serverSpec?.url?.trim()
    ) {
      toast.error(t("mcp.wizard.urlRequired"), { duration: 3000 });
      return;
    }

    setSaving(true);
    try {
      await upsertMutation.mutateAsync({
        id: trimmedId,
        serverSpec,
      });
      toast.success(t("common.success"), { closeButton: true });
      await onSave();
    } catch (error: unknown) {
      const detail = extractErrorMessage(error);
      const mapped = translateMcpBackendError(detail, t);
      const msg = mapped || detail || t("mcp.error.saveFailed");
      toast.error(msg, { duration: mapped || detail ? 6000 : 4000 });
    } finally {
      setSaving(false);
    }
  };

  const getFormTitle = () => {
    return isEditing ? t("mcp.editServer") : t("mcp.addServer");
  };

  return (
    <>
      <FullScreenPanel
        isOpen={true}
        title={getFormTitle()}
        onClose={onClose}
        footer={
          <Button
            type="button"
            onClick={handleSubmit}
            disabled={saving || (!isEditing && !!idError)}
            className="bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isEditing ? <Save size={16} /> : <Plus size={16} />}
            {saving
              ? t("common.saving")
              : isEditing
                ? t("common.save")
                : t("common.add")}
          </Button>
        }
      >
        <div className="flex flex-col h-full gap-6">
          {/* 上半部分：表单字段 */}
          <div className="glass rounded-xl p-6 border border-white/10 space-y-6 flex-shrink-0">
            {/* 预设选择（仅新增时展示） */}
            {!isEditing && (
              <div>
                <label className="block text-sm font-medium text-foreground mb-3">
                  {t("mcp.presets.title")}
                </label>
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={applyCustom}
                    className={`inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                      selectedPreset === -1
                        ? "bg-emerald-500 text-white dark:bg-emerald-600"
                        : "bg-accent text-muted-foreground hover:bg-accent/80"
                    }`}
                  >
                    {t("presetSelector.custom")}
                  </button>
                  {mcpPresets.map((preset, idx) => {
                    const descriptionKey = `mcp.presets.${preset.id}.description`;
                    return (
                      <button
                        key={preset.id}
                        type="button"
                        onClick={() => applyPreset(idx)}
                        className={`inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                          selectedPreset === idx
                            ? "bg-emerald-500 text-white dark:bg-emerald-600"
                            : "bg-accent text-muted-foreground hover:bg-accent/80"
                        }`}
                        title={t(descriptionKey)}
                      >
                        {preset.id}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            {/* ID (标题) */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="block text-sm font-medium text-foreground">
                  {t("mcp.form.title")} <span className="text-red-500">*</span>
                </label>
                {!isEditing && idError && (
                  <span className="text-xs text-red-500 dark:text-red-400">
                    {idError}
                  </span>
                )}
              </div>
              <Input
                type="text"
                placeholder={t("mcp.form.titlePlaceholder")}
                value={formId}
                onChange={(e) => handleIdChange(e.target.value)}
                disabled={isEditing}
              />
              <p className="mt-2 text-xs text-muted-foreground">
                {t("mcp.appPanel.saveScope", {
                  appName: t(`apps.${appId}`),
                })}
              </p>
            </div>
          </div>

          {/* 下半部分：JSON 配置编辑器 - 自适应剩余高度 */}
          <div className="glass rounded-xl p-6 border border-white/10 flex flex-col flex-1 min-h-0">
            <div className="flex items-center justify-between mb-4 flex-shrink-0">
              <label className="text-sm font-medium text-foreground">
                {useToml ? t("mcp.form.tomlConfig") : t("mcp.form.jsonConfig")}
              </label>
              {(isEditing || selectedPreset === -1) && (
                <button
                  type="button"
                  onClick={() => setIsWizardOpen(true)}
                  className="text-sm text-blue-500 dark:text-blue-400 hover:text-blue-600 dark:hover:text-blue-300 transition-colors"
                >
                  {t("mcp.form.useWizard")}
                </button>
              )}
            </div>
            <div className="flex-1 min-h-0 flex flex-col">
              <div className="flex-1 min-h-0">
                <JsonEditor
                  value={formConfig}
                  onChange={handleConfigChange}
                  placeholder={
                    useToml
                      ? t("mcp.form.tomlPlaceholder")
                      : t("mcp.form.jsonPlaceholder")
                  }
                  darkMode={isDarkMode}
                  rows={12}
                  showValidation={!useToml}
                  language={useToml ? "javascript" : "json"}
                  height="100%"
                />
              </div>
              {configError && (
                <div className="flex items-center gap-2 mt-2 text-red-500 dark:text-red-400 text-sm flex-shrink-0">
                  <AlertCircle size={16} />
                  <span>{configError}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </FullScreenPanel>

      {/* Wizard Modal */}
      <McpWizardModal
        isOpen={isWizardOpen}
        onClose={() => setIsWizardOpen(false)}
        onApply={handleWizardApply}
        initialTitle={formId}
        initialServer={wizardInitialSpec}
      />
    </>
  );
};

export default McpFormModal;
