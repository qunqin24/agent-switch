import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Edit3, ExternalLink, FileCode2, Server, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ListItemRow } from "@/components/common/ListItemRow";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { isMcpAppId } from "@/config/appConfig";
import { mcpPresets } from "@/config/mcpPresets";
import { useDeleteMcpServer, useMcpServersForApp } from "@/hooks/useMcp";
import { settingsApi } from "@/lib/api";
import type { AppId, McpAppId } from "@/lib/api/types";
import type { McpServerSpec } from "@/types";
import { extractErrorMessage } from "@/utils/errorUtils";
import McpFormModal, { type AppMcpServerEntry } from "./McpFormModal";

interface McpPanelProps {
  appId: AppId;
}

export interface McpPanelHandle {
  openAdd: () => void;
  refresh: () => void;
}

interface ConfirmState {
  id: string;
}

function describeServer(server: McpServerSpec): string {
  const type = server.type ?? (server.url ? "sse" : "stdio");
  if (type === "http" || type === "sse") {
    return server.url ? `${type.toUpperCase()} · ${server.url}` : type;
  }

  const command = [server.command, ...(server.args ?? [])]
    .filter((part): part is string => Boolean(part))
    .join(" ");
  return command ? `STDIO · ${command}` : "STDIO";
}

const McpPanel = React.forwardRef<McpPanelHandle, McpPanelProps>(
  ({ appId }, ref) => {
    const { t } = useTranslation();
    const supportedApp: McpAppId | null = isMcpAppId(appId) ? appId : null;
    const [isFormOpen, setIsFormOpen] = useState(false);
    const [editingId, setEditingId] = useState<string | null>(null);
    const [confirmState, setConfirmState] = useState<ConfirmState | null>(null);

    const {
      data: config,
      error,
      isLoading,
      isFetching,
      refetch,
    } = useMcpServersForApp(supportedApp);
    const deleteMutation = useDeleteMcpServer();

    const serverEntries = useMemo(
      () => Object.entries(config?.servers ?? {}),
      [config?.servers],
    );

    useEffect(() => {
      setIsFormOpen(false);
      setEditingId(null);
      setConfirmState(null);
    }, [appId]);

    const handleAdd = () => {
      if (supportedApp === null) return;
      setEditingId(null);
      setIsFormOpen(true);
    };

    const handleRefresh = async () => {
      if (supportedApp === null) return;
      const result = await refetch();
      if (result.error) {
        toast.error(t("mcp.appPanel.refreshFailed"), {
          description: extractErrorMessage(result.error),
        });
        return;
      }
      toast.success(
        t("mcp.appPanel.refreshSuccess", {
          count: Object.keys(result.data?.servers ?? {}).length,
          appName: t(`apps.${supportedApp}`),
        }),
        { closeButton: true },
      );
    };

    React.useImperativeHandle(
      ref,
      () => ({
        openAdd: handleAdd,
        refresh: () => void handleRefresh(),
      }),
      [supportedApp, refetch, t],
    );

    const handleDelete = async () => {
      if (supportedApp === null || confirmState === null) return;
      try {
        await deleteMutation.mutateAsync({
          app: supportedApp,
          id: confirmState.id,
        });
        setConfirmState(null);
        toast.success(t("common.success"), { closeButton: true });
      } catch (deleteError: unknown) {
        toast.error(t("mcp.error.deleteFailed"), {
          description: extractErrorMessage(deleteError),
        });
      }
    };

    if (supportedApp === null) {
      return (
        <div className="flex flex-1 items-center justify-center px-6 pb-20">
          <div className="max-w-md text-center">
            <div className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-zinc-100 text-zinc-400 dark:bg-zinc-800 dark:text-zinc-500">
              <Server size={24} />
            </div>
            <h2 className="text-base font-semibold text-foreground">
              {t("mcp.appPanel.unsupportedTitle", {
                appName: t(`apps.${appId}`),
              })}
            </h2>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              {t("mcp.appPanel.unsupportedDescription")}
            </p>
          </div>
        </div>
      );
    }

    const formatLabel = config?.storageFormat.toUpperCase() ?? "";
    const currentServer =
      editingId === null ? undefined : config?.servers[editingId];
    const initialData: AppMcpServerEntry | undefined =
      editingId !== null && currentServer
        ? { id: editingId, server: currentServer }
        : undefined;

    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-6">
        <div className="mx-auto flex w-full max-w-6xl flex-col gap-3 pb-4 pt-4">
          <div className="flex items-center justify-between gap-4">
            <div>
              <p className="text-sm font-medium text-foreground">
                {t("mcp.appPanel.serverCount", {
                  count: serverEntries.length,
                  appName: t(`apps.${supportedApp}`),
                })}
              </p>
              {config?.configPath && (
                <p
                  className="mt-1 max-w-3xl truncate font-mono text-xs text-muted-foreground"
                  title={config.configPath}
                >
                  {config.configPath}
                </p>
              )}
            </div>
            {formatLabel && (
              <span className="inline-flex items-center gap-1.5 rounded-full border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-[11px] font-semibold tracking-wide text-zinc-500 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-400">
                <FileCode2 size={12} />
                {formatLabel}
              </span>
            )}
          </div>
        </div>

        <div className="mx-auto w-full max-w-6xl flex-1 overflow-y-auto overflow-x-hidden pb-24">
          {isLoading ? (
            <div className="py-12 text-center text-muted-foreground">
              {t("mcp.loading")}
            </div>
          ) : error ? (
            <div className="rounded-xl border border-red-200 bg-red-50/70 px-4 py-3 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
              {extractErrorMessage(error)}
            </div>
          ) : serverEntries.length === 0 ? (
            <div className="flex min-h-full flex-col items-center justify-center py-12 text-center">
              <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-zinc-100 dark:bg-zinc-800">
                <Server size={24} className="text-muted-foreground/60" />
              </div>
              <h3 className="mb-2 text-lg font-medium text-foreground">
                {t("mcp.appPanel.noServers", {
                  appName: t(`apps.${supportedApp}`),
                })}
              </h3>
              <p className="text-sm text-muted-foreground">
                {t("mcp.emptyDescription")}
              </p>
            </div>
          ) : (
            <TooltipProvider delayDuration={300}>
              <div
                className={`overflow-hidden rounded-xl border border-zinc-100 bg-white shadow-[0_1px_3px_rgba(0,0,0,0.01)] divide-y divide-zinc-100 dark:border-zinc-800 dark:bg-zinc-950 dark:divide-zinc-900 ${
                  isFetching ? "opacity-70" : ""
                }`}
              >
                {serverEntries.map(([id, server], index) => (
                  <McpListItem
                    key={`${supportedApp}:${id}`}
                    id={id}
                    server={server}
                    onEdit={() => {
                      setEditingId(id);
                      setIsFormOpen(true);
                    }}
                    onDelete={() => setConfirmState({ id })}
                    isLast={index === serverEntries.length - 1}
                  />
                ))}
              </div>
            </TooltipProvider>
          )}
        </div>

        {isFormOpen && (
          <McpFormModal
            key={`${supportedApp}:${editingId ?? "new"}`}
            appId={supportedApp}
            editingId={editingId ?? undefined}
            initialData={initialData}
            existingIds={Object.keys(config?.servers ?? {})}
            defaultFormat={supportedApp === "codex" ? "toml" : "json"}
            onSave={async () => {
              setIsFormOpen(false);
              setEditingId(null);
            }}
            onClose={() => {
              setIsFormOpen(false);
              setEditingId(null);
            }}
          />
        )}

        <ConfirmDialog
          isOpen={confirmState !== null}
          title={t("mcp.unifiedPanel.deleteServer")}
          message={t("mcp.unifiedPanel.deleteConfirm", {
            id: confirmState?.id ?? "",
          })}
          onConfirm={() => void handleDelete()}
          onCancel={() => setConfirmState(null)}
        />
      </div>
    );
  },
);

McpPanel.displayName = "McpPanel";

interface McpListItemProps {
  id: string;
  server: McpServerSpec;
  onEdit: () => void;
  onDelete: () => void;
  isLast: boolean;
}

function McpListItem({
  id,
  server,
  onEdit,
  onDelete,
  isLast,
}: McpListItemProps) {
  const { t } = useTranslation();
  const preset = mcpPresets.find((candidate) => candidate.id === id);
  const docsUrl = preset?.docs ?? preset?.homepage;

  const openDocs = async () => {
    if (!docsUrl) return;
    try {
      await settingsApi.openExternal(docsUrl);
    } catch {
      // External-link failure should not block MCP management.
    }
  };

  return (
    <ListItemRow isLast={isLast}>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
            {id}
          </span>
          {docsUrl && (
            <button
              type="button"
              onClick={() => void openDocs()}
              className="flex-shrink-0 text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300"
              title={t("mcp.presets.docs")}
            >
              <ExternalLink size={12} />
            </button>
          )}
        </div>
        <p
          className="mt-0.5 truncate font-mono text-xs text-zinc-400 dark:text-zinc-500"
          title={describeServer(server)}
        >
          {describeServer(server)}
        </p>
      </div>

      <div className="flex flex-shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 rounded-lg text-zinc-500 hover:bg-zinc-900/5 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/5 dark:hover:text-zinc-100"
          onClick={onEdit}
          title={t("common.edit")}
        >
          <Edit3 size={14} />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 rounded-lg text-zinc-500 hover:bg-red-500/10 hover:text-red-600 dark:text-zinc-400 dark:hover:bg-red-500/15 dark:hover:text-red-400"
          onClick={onDelete}
          title={t("common.delete")}
        >
          <Trash2 size={14} />
        </Button>
      </div>
    </ListItemRow>
  );
}

export default McpPanel;
