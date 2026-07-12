import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  Bot,
  Check,
  ChevronsUpDown,
  EyeOff,
  FolderOpen,
  HelpCircle,
  Plus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { providersApi } from "@/lib/api";
import {
  opencodeAgentsApi,
  type OpenCodeAgentDocument,
  type OpenCodeAgentScope,
} from "@/lib/api/opencodeAgents";
import type { OmoOpenCodeModel } from "@/lib/api/providers";
import { extractErrorMessage } from "@/utils/errorUtils";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SegmentedControl } from "@/components/settings/SegmentedControl";
import {
  SettingSection,
  SettingsNote,
} from "@/components/settings/SettingSection";
import { SettingRow } from "@/components/settings/SettingRow";
import { ToggleRow } from "@/components/ui/toggle-row";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

type AgentMode = "primary" | "subagent" | "all";
type PermissionAction = "allow" | "ask" | "deny";
type PermissionSelection = PermissionAction | "inherit";

const PERMISSION_KEYS = [
  "read",
  "edit",
  "glob",
  "grep",
  "list",
  "bash",
  "task",
  "external_directory",
  "todowrite",
  "webfetch",
  "websearch",
  "lsp",
  "skill",
  "question",
  "doom_loop",
] as const;

const KNOWN_FIELDS = new Set([
  "description",
  "mode",
  "model",
  "variant",
  "temperature",
  "top_p",
  "steps",
  "hidden",
  "disable",
  "color",
  "permission",
]);

interface AgentDraft {
  id: string;
  originalId?: string;
  description: string;
  mode: AgentMode;
  model: string;
  variant: string;
  temperature: string;
  topP: string;
  steps: string;
  hidden: boolean;
  disable: boolean;
  color: string;
  permissions: Record<string, PermissionAction>;
  permissionExtras: Record<string, unknown>;
  prompt: string;
  advancedJson: string;
}

const emptyDraft = (): AgentDraft => ({
  id: "",
  description: "",
  mode: "subagent",
  model: "",
  variant: "",
  temperature: "",
  topP: "",
  steps: "",
  hidden: false,
  disable: false,
  color: "",
  permissions: {},
  permissionExtras: {},
  prompt: "",
  advancedJson: "{}",
});

const asRecord = (value: unknown): Record<string, unknown> =>
  value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};

const toDraft = (agent: OpenCodeAgentDocument): AgentDraft => {
  const frontmatter = asRecord(agent.frontmatter);
  const permission = asRecord(frontmatter.permission);
  const permissions: Record<string, PermissionAction> = {};
  const permissionExtras: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(permission)) {
    if (
      PERMISSION_KEYS.includes(key as (typeof PERMISSION_KEYS)[number]) &&
      (value === "allow" || value === "ask" || value === "deny")
    ) {
      permissions[key] = value;
    } else {
      permissionExtras[key] = value;
    }
  }
  const advanced = Object.fromEntries(
    Object.entries(frontmatter).filter(([key]) => !KNOWN_FIELDS.has(key)),
  );
  const mode = frontmatter.mode;
  return {
    id: agent.id,
    originalId: agent.id,
    description:
      typeof frontmatter.description === "string"
        ? frontmatter.description
        : "",
    mode:
      mode === "primary" || mode === "all" || mode === "subagent"
        ? mode
        : "all",
    model: typeof frontmatter.model === "string" ? frontmatter.model : "",
    variant: typeof frontmatter.variant === "string" ? frontmatter.variant : "",
    temperature:
      typeof frontmatter.temperature === "number"
        ? String(frontmatter.temperature)
        : "",
    topP:
      typeof frontmatter.top_p === "number" ? String(frontmatter.top_p) : "",
    steps:
      typeof frontmatter.steps === "number" ? String(frontmatter.steps) : "",
    hidden: frontmatter.hidden === true,
    disable: frontmatter.disable === true,
    color: typeof frontmatter.color === "string" ? frontmatter.color : "",
    permissions,
    permissionExtras,
    prompt: agent.prompt,
    advancedJson: JSON.stringify(advanced, null, 2),
  };
};

const optionalNumber = (value: string): number | undefined => {
  if (!value.trim()) return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
};

const buildDocument = (
  draft: AgentDraft,
  scope: OpenCodeAgentScope,
): OpenCodeAgentDocument => {
  const advanced = JSON.parse(draft.advancedJson || "{}") as unknown;
  if (!advanced || typeof advanced !== "object" || Array.isArray(advanced)) {
    throw new Error("Advanced fields must be a JSON object");
  }
  const frontmatter: Record<string, unknown> = {
    ...(advanced as Record<string, unknown>),
    description: draft.description.trim(),
    mode: draft.mode,
  };
  if (draft.model) frontmatter.model = draft.model;
  if (draft.variant) frontmatter.variant = draft.variant;
  const temperature = optionalNumber(draft.temperature);
  const topP = optionalNumber(draft.topP);
  const steps = optionalNumber(draft.steps);
  if (temperature !== undefined) frontmatter.temperature = temperature;
  if (topP !== undefined) frontmatter.top_p = topP;
  if (steps !== undefined) frontmatter.steps = steps;
  if (draft.hidden) frontmatter.hidden = true;
  if (draft.disable) frontmatter.disable = true;
  if (draft.color) frontmatter.color = draft.color;
  const permission = { ...draft.permissionExtras, ...draft.permissions };
  if (Object.keys(permission).length > 0) frontmatter.permission = permission;

  return {
    id: draft.id.trim(),
    scope,
    filePath: "",
    frontmatter,
    prompt: draft.prompt,
  };
};

const draftFingerprint = (draft: AgentDraft) =>
  JSON.stringify({
    id: draft.id,
    description: draft.description,
    mode: draft.mode,
    model: draft.model,
    variant: draft.variant,
    temperature: draft.temperature,
    topP: draft.topP,
    steps: draft.steps,
    hidden: draft.hidden,
    disable: draft.disable,
    color: draft.color,
    permissions: draft.permissions,
    permissionExtras: draft.permissionExtras,
    prompt: draft.prompt,
    advancedJson: draft.advancedJson,
  });

function ModelPicker({
  value,
  models,
  onChange,
  disabled = false,
}: {
  value: string;
  models: OmoOpenCodeModel[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [listScrolled, setListScrolled] = useState(false);
  const selected = models.find((model) => model.value === value);
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setListScrolled(false);
      }}
    >
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          className="h-9 w-full justify-between font-normal"
          disabled={disabled}
        >
          <span className="truncate">
            {selected
              ? `${selected.providerId} / ${selected.name}`
              : value || t("agents.form.inheritModel")}
          </span>
          <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[var(--radix-popover-trigger-width)] p-0">
        <Command>
          <CommandInput
            placeholder={t("agents.form.searchModel")}
            wrapperClassName={cn(
              "relative z-10 border-b-0 bg-popover transition-shadow duration-200",
              listScrolled &&
                "shadow-[0_1px_0_0_rgba(0,0,0,0.04),0_4px_12px_-6px_rgba(0,0,0,0.08)] dark:shadow-[0_1px_0_0_rgba(255,255,255,0.06),0_4px_12px_-6px_rgba(0,0,0,0.45)]",
            )}
          />
          <CommandList
            className="max-h-72"
            onScroll={(event) => {
              setListScrolled(event.currentTarget.scrollTop > 0);
            }}
          >
            <CommandEmpty>{t("agents.form.noModels")}</CommandEmpty>
            <CommandItem
              value="inherit default model"
              className="rounded-none"
              onSelect={() => {
                onChange("");
                setOpen(false);
              }}
            >
              <Check
                className={cn("h-4 w-4", value ? "opacity-0" : "opacity-100")}
              />
              {t("agents.form.inheritModel")}
            </CommandItem>
            {models.map((model) => (
              <CommandItem
                key={model.value}
                value={`${model.value} ${model.name}`}
                className="rounded-none"
                onSelect={() => {
                  onChange(model.value);
                  setOpen(false);
                }}
              >
                <Check
                  className={cn(
                    "h-4 w-4",
                    value === model.value ? "opacity-100" : "opacity-0",
                  )}
                />
                <span className="truncate">
                  {model.providerId} / {model.name}
                </span>
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

const AGENT_COLOR_PALETTE = [
  "#3b82f6",
  "#06b6d4",
  "#10b981",
  "#84cc16",
  "#eab308",
  "#f97316",
  "#ef4444",
  "#ec4899",
  "#a855f7",
  "#6366f1",
  "#14b8a6",
  "#f43f5e",
] as const;

const hashAgentId = (id: string) => {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) {
    hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  }
  return hash;
};

const displayAgentId = (id: string) =>
  id ? id.charAt(0).toUpperCase() + id.slice(1) : id;

const colorForAgent = (id: string, color?: string) => {
  const trimmed = color?.trim();
  if (trimmed) return trimmed;
  return AGENT_COLOR_PALETTE[hashAgentId(id) % AGENT_COLOR_PALETTE.length];
};

function AgentColorSwatch({ id, color }: { id: string; color?: string }) {
  return (
    <span
      className="h-2.5 w-2.5 shrink-0 rounded-full ring-1 ring-black/10 dark:ring-white/15"
      style={{ backgroundColor: colorForAgent(id, color) }}
      aria-hidden
    />
  );
}

function FieldBlock({
  label,
  htmlFor,
  children,
  className,
}: {
  label: React.ReactNode;
  htmlFor?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("space-y-1.5", className)}>
      <Label htmlFor={htmlFor} className="text-[12px] text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  );
}

const applyMcpServerPermission = (
  permissionExtras: Record<string, unknown>,
  serverId: string,
  selection: PermissionSelection,
) => {
  const pattern = `${serverId}_*`;
  const serverPrefix = `${serverId}_`;
  const nextExtras: Record<string, unknown> = {};
  let inserted = false;

  for (const [key, value] of Object.entries(permissionExtras)) {
    if (key === pattern) {
      if (selection !== "inherit") nextExtras[pattern] = selection;
      inserted = true;
      continue;
    }
    if (!inserted && selection !== "inherit" && key.startsWith(serverPrefix)) {
      nextExtras[pattern] = selection;
      inserted = true;
    }
    nextExtras[key] = value;
  }
  if (!inserted && selection !== "inherit") {
    nextExtras[pattern] = selection;
  }

  return nextExtras;
};

export function AgentsPanel({}: { onOpenChange: (open: boolean) => void }) {
  const { t } = useTranslation();
  const [scope, setScope] = useState<OpenCodeAgentScope>("global");
  const [projectDir, setProjectDir] = useState("");
  const [agents, setAgents] = useState<OpenCodeAgentDocument[]>([]);
  const [models, setModels] = useState<OmoOpenCodeModel[]>([]);
  const [mcpServerIds, setMcpServerIds] = useState<string[]>([]);
  const [mcpServersLoading, setMcpServersLoading] = useState(true);
  const [draft, setDraft] = useState<AgentDraft | null>(null);
  const [baseline, setBaseline] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [editorScrolled, setEditorScrolled] = useState(false);
  const [listScrolled, setListScrolled] = useState(false);
  const [deleteTarget, setDeleteTarget] =
    useState<OpenCodeAgentDocument | null>(null);

  const location = useMemo(
    () => ({ scope, projectDir: scope === "project" ? projectDir : undefined }),
    [projectDir, scope],
  );

  const selectDraft = useCallback((next: AgentDraft | null) => {
    setDraft(next);
    setBaseline(next ? draftFingerprint(next) : null);
    setAdvancedOpen(false);
    setEditorScrolled(false);
  }, []);

  const reload = useCallback(async () => {
    if (scope === "project" && !projectDir) {
      setAgents([]);
      selectDraft(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const next = await opencodeAgentsApi.list(location);
      setAgents(next);
      setDraft((current) => {
        if (!current?.originalId) return current;
        const updated = next.find((agent) => agent.id === current.originalId);
        if (!updated) {
          queueMicrotask(() => setBaseline(null));
          return null;
        }
        const refreshed = toDraft(updated);
        queueMicrotask(() => setBaseline(draftFingerprint(refreshed)));
        return refreshed;
      });
    } catch (error) {
      toast.error(extractErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [location, projectDir, scope, selectDraft]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    void providersApi
      .listOpenCodeModelsForOmo()
      .then(setModels)
      .catch((error) => console.warn("Failed to load OpenCode models", error));
  }, []);

  const loadMcpServerIds = useCallback(async () => {
    setMcpServersLoading(true);
    try {
      setMcpServerIds(await opencodeAgentsApi.listMcpServerIds());
    } catch (error) {
      console.warn("Failed to load OpenCode MCP servers", error);
      setMcpServerIds([]);
    } finally {
      setMcpServersLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadMcpServerIds();
  }, [loadMcpServerIds]);

  const filteredAgents = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return agents;
    return agents.filter((agent) => {
      const frontmatter = asRecord(agent.frontmatter);
      return [agent.id, frontmatter.description, frontmatter.model]
        .filter((value): value is string => typeof value === "string")
        .some((value) => value.toLowerCase().includes(query));
    });
  }, [agents, search]);

  const selectedModel = models.find((model) => model.value === draft?.model);
  const variants = useMemo(() => {
    const options = [...(selectedModel?.variants ?? [])];
    if (draft?.variant && !options.includes(draft.variant))
      options.push(draft.variant);
    return options;
  }, [draft?.variant, selectedModel?.variants]);

  const isDirty = Boolean(
    draft && baseline && draftFingerprint(draft) !== baseline,
  );
  const selectedAgent = draft?.originalId
    ? agents.find((agent) => agent.id === draft.originalId)
    : undefined;
  const isOmoSlimManaged = selectedAgent?.managedBy === "omo-slim";

  const chooseProject = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setProjectDir(selected);
      selectDraft(null);
      setListScrolled(false);
    }
  };

  const save = async () => {
    if (!draft) return;
    if (!draft.id.trim() || !draft.description.trim()) {
      toast.error(t("agents.validation.required"));
      return;
    }
    setSaving(true);
    try {
      const document = buildDocument(draft, scope);
      const saved = await opencodeAgentsApi.save(
        location,
        document,
        draft.originalId,
      );
      await reload();
      selectDraft(toDraft(saved));
      toast.success(t("agents.notifications.saved"));
    } catch (error) {
      toast.error(extractErrorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    try {
      await opencodeAgentsApi.delete(location, deleteTarget.id);
      if (draft?.originalId === deleteTarget.id) selectDraft(null);
      setDeleteTarget(null);
      await reload();
      toast.success(t("agents.notifications.deleted"));
    } catch (error) {
      toast.error(extractErrorMessage(error));
    }
  };

  const updateDraft = <K extends keyof AgentDraft>(
    key: K,
    value: AgentDraft[K],
  ) =>
    setDraft((current) => (current ? { ...current, [key]: value } : current));

  const scopeOptions = [
    { value: "global" as const, label: t("agents.scope.global") },
    { value: "project" as const, label: t("agents.scope.project") },
  ];

  const modeOptions = [
    { value: "primary" as const, label: t("agents.mode.primary") },
    { value: "subagent" as const, label: t("agents.mode.subagent") },
    { value: "all" as const, label: t("agents.mode.all") },
  ];

  const permissionOptions = [
    { value: "allow" as const, label: t("agents.permissions.allow") },
    { value: "ask" as const, label: t("agents.permissions.ask") },
    { value: "deny" as const, label: t("agents.permissions.deny") },
  ];

  const bulkToolPermissionOptions = [
    { ...permissionOptions[0], ariaLabel: t("agents.permissions.bulkAllow") },
    { ...permissionOptions[1], ariaLabel: t("agents.permissions.bulkAsk") },
    { ...permissionOptions[2], ariaLabel: t("agents.permissions.bulkDeny") },
  ];

  const bulkMcpPermissionOptions = [
    {
      ...permissionOptions[0],
      ariaLabel: t("agents.mcpPermissions.bulkAllow"),
    },
    {
      ...permissionOptions[1],
      ariaLabel: t("agents.mcpPermissions.bulkAsk"),
    },
    {
      ...permissionOptions[2],
      ariaLabel: t("agents.mcpPermissions.bulkDeny"),
    },
  ];

  const mcpPermissionOptions = [
    { value: "inherit" as const, label: t("agents.permissions.inherit") },
    ...permissionOptions,
  ];

  const activeToolBulkAction = useMemo(() => {
    if (!draft) return undefined;
    if (
      PERMISSION_KEYS.some((key) =>
        Object.prototype.hasOwnProperty.call(draft.permissionExtras, key),
      )
    ) {
      return undefined;
    }
    const actions = PERMISSION_KEYS.map(
      (key) => draft.permissions[key] ?? "ask",
    );
    const first = actions[0];
    return actions.every((action) => action === first) ? first : undefined;
  }, [draft]);

  const activeMcpBulkAction = useMemo(() => {
    if (!draft || mcpServerIds.length === 0) return undefined;
    const actions = mcpServerIds.map((serverId) => {
      const value = draft.permissionExtras[`${serverId}_*`];
      return value === "allow" || value === "ask" || value === "deny"
        ? value
        : "inherit";
    });
    const first = actions[0];
    return first !== "inherit" && actions.every((action) => action === first)
      ? first
      : undefined;
  }, [draft, mcpServerIds]);

  const updateMcpServerPermission = (
    serverId: string,
    selection: PermissionSelection,
  ) => {
    setDraft((current) => {
      if (!current) return current;
      return {
        ...current,
        permissionExtras: applyMcpServerPermission(
          current.permissionExtras,
          serverId,
          selection,
        ),
      };
    });
  };

  const updateAllToolPermissions = (action: PermissionAction) => {
    setDraft((current) => {
      if (!current) return current;
      const permissions = Object.fromEntries(
        PERMISSION_KEYS.map((key) => [key, action]),
      );
      const permissionExtras = Object.fromEntries(
        Object.entries(current.permissionExtras).filter(
          ([key]) =>
            !PERMISSION_KEYS.includes(key as (typeof PERMISSION_KEYS)[number]),
        ),
      );
      return { ...current, permissions, permissionExtras };
    });
  };

  const updateAllMcpPermissions = (action: PermissionAction) => {
    setDraft((current) => {
      if (!current) return current;
      const permissionExtras = mcpServerIds.reduce(
        (extras, serverId) =>
          applyMcpServerPermission(extras, serverId, action),
        current.permissionExtras,
      );
      return { ...current, permissionExtras };
    });
  };

  const createAgent = () => {
    selectDraft(emptyDraft());
  };

  const colorValue = draft?.color?.trim();
  const colorPickerValue =
    colorValue && /^#[0-9a-fA-F]{6}$/.test(colorValue) ? colorValue : "#3b82f6";

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden px-6 pb-6">
      <div className="flex shrink-0 items-center justify-between gap-4 py-4">
        <div className="flex min-w-0 items-center gap-2">
          <SegmentedControl
            value={scope}
            options={scopeOptions}
            onChange={(value) => {
              setScope(value);
              selectDraft(null);
              setListScrolled(false);
            }}
          />
          {scope === "project" && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 max-w-72"
              onClick={chooseProject}
            >
              <FolderOpen className="mr-2 h-3.5 w-3.5 shrink-0" />
              <span className="truncate">
                {projectDir || t("agents.scope.chooseProject")}
              </span>
            </Button>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={() => {
              void reload();
              void loadMcpServerIds();
            }}
            title={t("common.refresh")}
          >
            <RefreshCw
              className={cn(
                "h-4 w-4",
                (loading || mcpServersLoading) && "animate-spin",
              )}
            />
          </Button>
          <Button
            type="button"
            size="sm"
            className="h-8"
            onClick={createAgent}
            disabled={scope === "project" && !projectDir}
          >
            <Plus className="mr-1.5 h-4 w-4" />
            {t("agents.add")}
          </Button>
        </div>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)] overflow-hidden rounded-xl border border-border-default bg-background">
        <aside className="flex min-h-0 flex-col border-r border-border-default bg-muted/20">
          <div
            className={cn(
              "relative z-10 flex h-14 shrink-0 items-center bg-muted/20 px-3 transition-shadow duration-200",
              listScrolled &&
                "shadow-[0_1px_0_0_rgba(0,0,0,0.04),0_4px_12px_-6px_rgba(0,0,0,0.08)] dark:shadow-[0_1px_0_0_rgba(255,255,255,0.06),0_4px_12px_-6px_rgba(0,0,0,0.45)]",
            )}
          >
            <div className="relative w-full">
              <Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t("agents.search")}
                className="h-8 bg-background pl-8 text-[13px]"
              />
            </div>
          </div>

          <div
            className="flex-1 overflow-y-auto overscroll-contain"
            onScroll={(event) => {
              setListScrolled(event.currentTarget.scrollTop > 0);
            }}
          >
            {loading ? (
              <div className="space-y-1 p-2">
                {Array.from({ length: 4 }).map((_, index) => (
                  <div
                    key={index}
                    className="h-14 animate-pulse rounded-md bg-muted/60"
                  />
                ))}
              </div>
            ) : scope === "project" && !projectDir ? (
              <div className="flex min-h-full flex-col items-center justify-center px-6 text-center">
                <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                  <FolderOpen className="h-4 w-4 text-muted-foreground" />
                </div>
                <p className="text-[13px] text-muted-foreground">
                  {t("agents.scope.projectHint")}
                </p>
              </div>
            ) : filteredAgents.length === 0 ? (
              <div className="flex min-h-full flex-col items-center justify-center px-6 text-center">
                <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-full bg-muted">
                  <Bot className="h-4 w-4 text-muted-foreground" />
                </div>
                <p className="text-[13px] font-medium text-foreground">
                  {t("agents.empty")}
                </p>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="mt-4 h-8"
                  onClick={createAgent}
                  disabled={scope === "project" && !projectDir}
                >
                  <Plus className="mr-1.5 h-3.5 w-3.5" />
                  {t("agents.add")}
                </Button>
              </div>
            ) : (
              <div className="py-1">
                {filteredAgents.map((agent) => {
                  const meta = asRecord(agent.frontmatter);
                  const active = draft?.originalId === agent.id;
                  const mode = String(meta.mode ?? "subagent");
                  const modelLabel =
                    typeof meta.model === "string" && meta.model
                      ? meta.model
                      : t("agents.form.inheritModel");
                  const color =
                    typeof meta.color === "string" ? meta.color : undefined;
                  const hidden = meta.hidden === true;
                  const disabled = meta.disable === true;
                  const isOmoSlimManaged = agent.managedBy === "omo-slim";

                  return (
                    <button
                      type="button"
                      key={agent.id}
                      onClick={() => selectDraft(toDraft(agent))}
                      className={cn(
                        "relative flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors",
                        active
                          ? "bg-primary/10"
                          : "hover:bg-zinc-500/[0.04] dark:hover:bg-zinc-100/[0.03]",
                      )}
                    >
                      {active && (
                        <span className="absolute bottom-2 left-0 top-2 w-[3px] rounded-full bg-primary" />
                      )}
                      <AgentColorSwatch id={agent.id} color={color} />
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5">
                          <span className="truncate text-[13px] font-medium">
                            {displayAgentId(agent.id)}
                          </span>
                          <Badge
                            variant="outline"
                            className="h-4 shrink-0 px-1 text-[10px] font-normal text-muted-foreground"
                          >
                            {t(`agents.mode.${mode}`, { defaultValue: mode })}
                          </Badge>
                          {isOmoSlimManaged && (
                            <Badge
                              variant="outline"
                              className="h-4 shrink-0 border-primary/30 bg-primary/5 px-1 text-[10px] font-normal text-primary"
                            >
                              {t("agents.source.omoSlim")}
                            </Badge>
                          )}
                          {hidden && (
                            <EyeOff
                              className="h-3 w-3 shrink-0 text-muted-foreground"
                              aria-label={t("agents.status.hidden")}
                            />
                          )}
                          {disabled && (
                            <Badge
                              variant="secondary"
                              className="h-4 shrink-0 px-1 text-[10px] font-normal"
                            >
                              {t("agents.status.disabled")}
                            </Badge>
                          )}
                        </span>
                        <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                          {isOmoSlimManaged
                            ? t("agents.source.managedByOmoSlim")
                            : modelLabel}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          <div
            className={cn(
              "relative z-10 shrink-0 bg-muted/20 px-3 py-2 text-[11px] text-muted-foreground transition-shadow duration-200",
              listScrolled &&
                "shadow-[0_-1px_0_0_rgba(0,0,0,0.04),0_-4px_12px_-6px_rgba(0,0,0,0.08)] dark:shadow-[0_-1px_0_0_rgba(255,255,255,0.06),0_-4px_12px_-6px_rgba(0,0,0,0.45)]",
            )}
          >
            {t("agents.count", { count: agents.length })}
          </div>
        </aside>

        <section className="flex min-h-0 flex-col">
          {!draft ? (
            <div className="flex h-full flex-col items-center justify-center px-8 text-center">
              <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-muted">
                <Bot className="h-5 w-5 text-muted-foreground" />
              </div>
              <h3 className="text-[15px] font-medium tracking-tight">
                {t("agents.selectTitle")}
              </h3>
              <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
                {t("agents.selectDescription")}
              </p>
              <Button
                type="button"
                size="sm"
                className="mt-5 h-8"
                onClick={createAgent}
                disabled={scope === "project" && !projectDir}
              >
                <Plus className="mr-1.5 h-4 w-4" />
                {t("agents.add")}
              </Button>
            </div>
          ) : (
            <>
              <div
                className={cn(
                  "relative z-10 flex h-14 shrink-0 items-center justify-between gap-4 bg-background px-5 transition-shadow duration-200",
                  editorScrolled &&
                    "shadow-[0_1px_0_0_rgba(0,0,0,0.04),0_4px_12px_-6px_rgba(0,0,0,0.08)] dark:shadow-[0_1px_0_0_rgba(255,255,255,0.06),0_4px_12px_-6px_rgba(0,0,0,0.45)]",
                )}
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <h2 className="truncate text-[15px] font-semibold tracking-tight">
                      {draft.originalId
                        ? displayAgentId(draft.originalId)
                        : t("agents.newAgent")}
                    </h2>
                    {isOmoSlimManaged && (
                      <Badge
                        variant="outline"
                        className="h-5 shrink-0 border-primary/30 bg-primary/5 px-1.5 text-[10px] font-normal text-primary"
                      >
                        {t("agents.source.omoSlim")}
                      </Badge>
                    )}
                    {isDirty && (
                      <Badge
                        variant="secondary"
                        className="h-5 px-1.5 text-[10px] font-medium"
                      >
                        {t("agents.status.unsaved")}
                      </Badge>
                    )}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {draft.originalId && !isOmoSlimManaged && (
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 text-muted-foreground hover:text-destructive"
                      onClick={() =>
                        setDeleteTarget(
                          agents.find((item) => item.id === draft.originalId) ??
                            null,
                        )
                      }
                      title={t("common.delete")}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  )}
                  <Button
                    type="button"
                    size="sm"
                    className="h-8 min-w-[72px]"
                    onClick={() => void save()}
                    disabled={saving || !isDirty || isOmoSlimManaged}
                  >
                    {saving ? t("common.saving") : t("common.save")}
                  </Button>
                </div>
              </div>

              <div
                className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
                onScroll={(event) => {
                  setEditorScrolled(event.currentTarget.scrollTop > 0);
                }}
              >
                <div className="mx-auto max-w-3xl space-y-5 px-5 py-5">
                  {isOmoSlimManaged ? (
                    <SettingsNote variant="info">
                      {t("agents.source.readOnlyHint")}
                    </SettingsNote>
                  ) : (
                    <SettingsNote variant="warning">
                      {t("agents.form.restartHint")}
                    </SettingsNote>
                  )}

                  <fieldset
                    disabled={isOmoSlimManaged}
                    className="space-y-5 border-0 p-0 disabled:opacity-70"
                  >
                    <SettingSection title={t("agents.sections.identity")} inset>
                      <div className="space-y-4">
                        <FieldBlock
                          label={t("agents.form.id")}
                          htmlFor="agent-id"
                        >
                          <Input
                            id="agent-id"
                            value={draft.id}
                            onChange={(event) =>
                              updateDraft("id", event.target.value)
                            }
                            placeholder="security-reviewer"
                            className="h-9 font-mono text-[13px]"
                          />
                        </FieldBlock>
                        <FieldBlock
                          label={t("agents.form.description")}
                          htmlFor="agent-description"
                        >
                          <Textarea
                            id="agent-description"
                            value={draft.description}
                            onChange={(event) =>
                              updateDraft("description", event.target.value)
                            }
                            className="min-h-[72px] resize-y text-[13px] leading-relaxed"
                          />
                        </FieldBlock>
                      </div>
                    </SettingSection>

                    <SettingSection title={t("agents.sections.behavior")}>
                      <SettingRow
                        title={t("agents.form.mode")}
                        description={t("agents.form.modeHint")}
                      >
                        <SegmentedControl
                          value={draft.mode}
                          options={modeOptions}
                          onChange={(value) => updateDraft("mode", value)}
                        />
                      </SettingRow>
                    </SettingSection>

                    <SettingSection title={t("agents.sections.model")} inset>
                      <div className="space-y-4">
                        <div className="grid grid-cols-[minmax(0,1fr)_160px] gap-3">
                          <FieldBlock label={t("agents.form.model")}>
                            <ModelPicker
                              value={draft.model}
                              models={models}
                              disabled={isOmoSlimManaged}
                              onChange={(value) => {
                                updateDraft("model", value);
                                updateDraft("variant", "");
                              }}
                            />
                          </FieldBlock>
                          <FieldBlock label={t("agents.form.variant")}>
                            <Select
                              value={draft.variant || "__none__"}
                              onValueChange={(value) =>
                                updateDraft(
                                  "variant",
                                  value === "__none__" ? "" : value,
                                )
                              }
                              disabled={!draft.model}
                            >
                              <SelectTrigger className="h-9">
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                <SelectItem value="__none__">
                                  {t("agents.form.noVariant")}
                                </SelectItem>
                                {variants.map((variant) => (
                                  <SelectItem key={variant} value={variant}>
                                    {variant}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </FieldBlock>
                        </div>

                        <div className="grid grid-cols-4 gap-3">
                          <FieldBlock label={t("agents.form.temperature")}>
                            <Input
                              type="number"
                              min="0"
                              max="2"
                              step="0.1"
                              value={draft.temperature}
                              onChange={(event) =>
                                updateDraft("temperature", event.target.value)
                              }
                              placeholder="0.1"
                              className="h-9"
                            />
                          </FieldBlock>
                          <FieldBlock label={t("agents.form.topP")}>
                            <Input
                              type="number"
                              min="0"
                              max="1"
                              step="0.1"
                              value={draft.topP}
                              onChange={(event) =>
                                updateDraft("topP", event.target.value)
                              }
                              placeholder="1"
                              className="h-9"
                            />
                          </FieldBlock>
                          <FieldBlock label={t("agents.form.steps")}>
                            <Input
                              type="number"
                              min="1"
                              step="1"
                              value={draft.steps}
                              onChange={(event) =>
                                updateDraft("steps", event.target.value)
                              }
                              className="h-9"
                            />
                          </FieldBlock>
                          <FieldBlock
                            label={
                              <span className="flex items-center gap-1.5">
                                <span>{t("agents.form.color")}</span>
                                <TooltipProvider delayDuration={250}>
                                  <Tooltip>
                                    <TooltipTrigger asChild>
                                      <span
                                        role="button"
                                        tabIndex={0}
                                        aria-label={t("agents.form.colorHelp")}
                                        className="inline-flex cursor-help text-muted-foreground/60 outline-none transition-colors hover:text-muted-foreground focus-visible:text-foreground"
                                      >
                                        <HelpCircle className="h-3.5 w-3.5" />
                                      </span>
                                    </TooltipTrigger>
                                    <TooltipContent
                                      side="right"
                                      className="max-w-[300px] border border-border bg-popover px-3 py-2 leading-relaxed text-popover-foreground shadow-md"
                                    >
                                      {t("agents.form.colorHint")}
                                    </TooltipContent>
                                  </Tooltip>
                                </TooltipProvider>
                              </span>
                            }
                          >
                            <div className="flex h-9 items-center gap-2 rounded-md border border-input bg-background px-2">
                              <input
                                type="color"
                                value={colorPickerValue}
                                onChange={(event) =>
                                  updateDraft("color", event.target.value)
                                }
                                className="h-6 w-6 cursor-pointer rounded border-0 bg-transparent p-0"
                                aria-label={t("agents.form.color")}
                              />
                              <Input
                                value={draft.color}
                                onChange={(event) =>
                                  updateDraft("color", event.target.value)
                                }
                                placeholder="#3b82f6"
                                className="h-7 border-0 px-1 font-mono text-[12px] shadow-none focus-visible:ring-0"
                              />
                            </div>
                          </FieldBlock>
                        </div>
                      </div>
                    </SettingSection>

                    <SettingSection title={t("agents.sections.visibility")}>
                      <ToggleRow
                        variant="plain"
                        title={t("agents.form.hidden")}
                        description={t("agents.form.hiddenHint")}
                        checked={draft.hidden}
                        onCheckedChange={(value) =>
                          updateDraft("hidden", value)
                        }
                      />
                      <ToggleRow
                        variant="plain"
                        title={t("agents.form.disabled")}
                        description={t("agents.form.disabledHint")}
                        checked={draft.disable}
                        onCheckedChange={(value) =>
                          updateDraft("disable", value)
                        }
                      />
                    </SettingSection>

                    <SettingSection
                      title={t("agents.permissions.title")}
                      footer={t("agents.permissions.description")}
                    >
                      <SettingRow
                        title={t("agents.permissions.bulkTitle")}
                        description={t("agents.permissions.bulkDescription")}
                      >
                        <SegmentedControl
                          value={activeToolBulkAction}
                          options={bulkToolPermissionOptions}
                          onChange={updateAllToolPermissions}
                        />
                      </SettingRow>
                      <TooltipProvider delayDuration={250}>
                        {PERMISSION_KEYS.map((key) => {
                          const label = t(`agents.permissions.tools.${key}`, {
                            defaultValue: key,
                          });
                          return (
                            <SettingRow
                              key={key}
                              title={
                                <span className="flex items-center gap-1.5">
                                  <span>{label}</span>
                                  <Tooltip>
                                    <TooltipTrigger asChild>
                                      <span
                                        role="button"
                                        tabIndex={0}
                                        aria-label={t(
                                          "agents.permissions.help",
                                          { tool: label },
                                        )}
                                        className="inline-flex cursor-help text-muted-foreground/60 outline-none transition-colors hover:text-muted-foreground focus-visible:text-foreground"
                                      >
                                        <HelpCircle className="h-3.5 w-3.5" />
                                      </span>
                                    </TooltipTrigger>
                                    <TooltipContent
                                      side="right"
                                      className="max-w-[280px] border border-border bg-popover px-3 py-2 leading-relaxed text-popover-foreground shadow-md"
                                    >
                                      {t(
                                        `agents.permissions.toolDescriptions.${key}`,
                                      )}
                                    </TooltipContent>
                                  </Tooltip>
                                </span>
                              }
                            >
                              <SegmentedControl
                                value={draft.permissions[key] ?? "ask"}
                                options={permissionOptions}
                                onChange={(action) =>
                                  updateDraft("permissions", {
                                    ...draft.permissions,
                                    [key]: action,
                                  })
                                }
                              />
                            </SettingRow>
                          );
                        })}
                      </TooltipProvider>
                    </SettingSection>

                    <SettingSection
                      title={t("agents.mcpPermissions.title")}
                      footer={t("agents.mcpPermissions.description")}
                    >
                      <SettingRow
                        title={t("agents.mcpPermissions.bulkTitle")}
                        description={t("agents.mcpPermissions.bulkDescription")}
                      >
                        <SegmentedControl
                          value={activeMcpBulkAction}
                          options={bulkMcpPermissionOptions}
                          onChange={updateAllMcpPermissions}
                          disabled={
                            mcpServersLoading || mcpServerIds.length === 0
                          }
                        />
                      </SettingRow>
                      {mcpServersLoading ? (
                        <div className="space-y-2 px-4 py-3">
                          {Array.from({ length: 2 }).map((_, index) => (
                            <div
                              key={index}
                              className="h-9 animate-pulse rounded-md bg-muted/60"
                            />
                          ))}
                        </div>
                      ) : mcpServerIds.length === 0 ? (
                        <p className="px-4 py-4 text-[12px] text-muted-foreground">
                          {t("agents.mcpPermissions.empty")}
                        </p>
                      ) : (
                        mcpServerIds.map((serverId) => {
                          const pattern = `${serverId}_*`;
                          const rawValue = draft.permissionExtras[pattern];
                          const selection: PermissionSelection =
                            rawValue === "allow" ||
                            rawValue === "ask" ||
                            rawValue === "deny"
                              ? rawValue
                              : "inherit";
                          return (
                            <SettingRow
                              key={serverId}
                              title={serverId}
                              description={t("agents.mcpPermissions.pattern", {
                                pattern,
                              })}
                            >
                              <SegmentedControl
                                value={selection}
                                options={mcpPermissionOptions}
                                onChange={(value) =>
                                  updateMcpServerPermission(serverId, value)
                                }
                              />
                            </SettingRow>
                          );
                        })
                      )}
                    </SettingSection>

                    <SettingSection title={t("agents.sections.prompt")} inset>
                      <Textarea
                        id="agent-prompt"
                        value={draft.prompt}
                        onChange={(event) =>
                          updateDraft("prompt", event.target.value)
                        }
                        className="min-h-[240px] font-mono text-[12.5px] leading-6"
                        placeholder={t("agents.form.promptPlaceholder")}
                      />
                    </SettingSection>

                    <Collapsible
                      open={advancedOpen}
                      onOpenChange={setAdvancedOpen}
                    >
                      <div className="overflow-hidden rounded-[10px] border border-border/70 bg-card shadow-sm">
                        <CollapsibleTrigger asChild>
                          <button
                            type="button"
                            className="flex w-full items-center justify-between px-4 py-3 text-left transition-colors hover:bg-muted/40"
                          >
                            <span className="text-[13px] font-medium">
                              {t("agents.form.advanced")}
                            </span>
                            <ChevronsUpDown className="h-3.5 w-3.5 text-muted-foreground" />
                          </button>
                        </CollapsibleTrigger>
                        <CollapsibleContent>
                          <div className="space-y-2 border-t border-border/60 px-4 py-3">
                            <p className="text-[11px] leading-relaxed text-muted-foreground">
                              {t("agents.form.advancedHint")}
                            </p>
                            <Textarea
                              value={draft.advancedJson}
                              onChange={(event) =>
                                updateDraft("advancedJson", event.target.value)
                              }
                              className="min-h-[140px] font-mono text-[12px]"
                            />
                          </div>
                        </CollapsibleContent>
                      </div>
                    </Collapsible>
                  </fieldset>
                </div>
              </div>
            </>
          )}
        </section>
      </div>

      <ConfirmDialog
        isOpen={Boolean(deleteTarget)}
        title={t("agents.delete.title")}
        message={t("agents.delete.message", { name: deleteTarget?.id })}
        onConfirm={() => void confirmDelete()}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
