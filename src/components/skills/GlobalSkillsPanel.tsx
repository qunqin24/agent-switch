import React, { useMemo, useState } from "react";
import {
  ExternalLink,
  Link2,
  Loader2,
  RefreshCw,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { AppCountBar } from "@/components/common/AppCountBar";
import { AppToggleGroup } from "@/components/common/AppToggleGroup";
import { ListItemRow } from "@/components/common/ListItemRow";
import { SkillBackupDialog } from "@/components/skills/SkillBackupDialog";
import {
  SkillScopeSelect,
  type SkillManagementScope,
} from "@/components/skills/SkillScopeSelect";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SKILL_APP_IDS } from "@/config/appConfig";
import {
  type GlobalSkill,
  type SkillBackupEntry,
  type SkillUpdateInfo,
  useCheckGlobalSkillUpdates,
  useDeleteGlobalSkillBackup,
  useGlobalSkillBackups,
  useGlobalSkills,
  useInstallGlobalSkillsFromZip,
  useRestoreGlobalSkillBackup,
  useSetGlobalSkillLink,
  useUninstallGlobalSkill,
  useUpdateGlobalSkill,
} from "@/hooks/useSkills";
import { settingsApi, skillsApi } from "@/lib/api";
import type { SkillAppId } from "@/lib/api/types";

export interface GlobalSkillsPanelHandle {
  refresh: () => void;
  openInstallFromZip: () => void;
  openRestoreFromBackup: () => void;
}

interface GlobalSkillsPanelProps {
  appOptions?: readonly SkillAppId[];
  onScopeChange: (scope: SkillManagementScope) => void;
}

const GlobalSkillsPanel = React.forwardRef<
  GlobalSkillsPanelHandle,
  GlobalSkillsPanelProps
>(function GlobalSkillsPanel({ appOptions, onScopeChange }, ref) {
  const { t } = useTranslation();
  const [restoreDialogOpen, setRestoreDialogOpen] = useState(false);
  const [isUpdatingAll, setIsUpdatingAll] = useState(false);
  const [confirmDialog, setConfirmDialog] = useState<{
    title: string;
    message: string;
    confirmText?: string;
    onConfirm: () => void;
  } | null>(null);

  const { data, isLoading, isFetching, refetch } = useGlobalSkills();
  const {
    data: backups = [],
    isFetching: isFetchingBackups,
    refetch: refetchBackups,
  } = useGlobalSkillBackups();
  const linkMutation = useSetGlobalSkillLink();
  const uninstallMutation = useUninstallGlobalSkill();
  const restoreMutation = useRestoreGlobalSkillBackup();
  const deleteBackupMutation = useDeleteGlobalSkillBackup();
  const installFromZipMutation = useInstallGlobalSkillsFromZip();
  const {
    data: skillUpdates = [],
    isFetching: isCheckingUpdates,
    refetch: checkUpdates,
  } = useCheckGlobalSkillUpdates();
  const updateMutation = useUpdateGlobalSkill();

  const skills = data?.skills ?? [];
  const directAppReasons = useMemo(() => {
    const reasons: Partial<Record<SkillAppId, string>> = {};
    for (const app of SKILL_APP_IDS) {
      if (data?.directApps[app]) {
        reasons[app] = t("skills.global.directApp", {
          appName: t(`apps.${app}`),
        });
      }
    }
    return reasons;
  }, [data?.directApps, t]);
  const linkedCounts = useMemo(
    () =>
      Object.fromEntries(
        SKILL_APP_IDS.map((app) => [
          app,
          skills.filter((skill) => skill.apps[app]).length,
        ]),
      ) as Record<SkillAppId, number>,
    [skills],
  );
  const updatesMap = useMemo(
    () =>
      Object.fromEntries(
        skillUpdates.map((update) => [update.id, update]),
      ) as Record<string, SkillUpdateInfo>,
    [skillUpdates],
  );

  const handleCheckUpdates = async () => {
    try {
      const result = await checkUpdates();
      const updates = result.data ?? [];
      toast[updates.length === 0 ? "success" : "info"](
        updates.length === 0
          ? t("skills.noUpdates")
          : t("skills.updatesFound", { count: updates.length }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(t("common.error"), { description: String(error) });
    }
  };

  const handleUpdate = async (skill: GlobalSkill) => {
    try {
      const updated = await updateMutation.mutateAsync(skill.id);
      toast.success(t("skills.updateSuccess", { name: updated.name }), {
        closeButton: true,
      });
    } catch (error) {
      toast.error(t("skills.updateFailed"), {
        description: String(error),
      });
    }
  };

  const handleUpdateAll = async () => {
    if (skillUpdates.length === 0) return;
    setIsUpdatingAll(true);
    let successCount = 0;
    for (const update of skillUpdates) {
      try {
        await updateMutation.mutateAsync(update.id);
        successCount += 1;
      } catch (error) {
        toast.error(t("skills.updateFailed"), {
          description: `${update.name}: ${String(error)}`,
        });
      }
    }
    setIsUpdatingAll(false);
    if (successCount > 0) {
      toast.success(t("skills.updateAllSuccess", { count: successCount }), {
        closeButton: true,
      });
    }
  };

  const handleToggle = async (
    skill: GlobalSkill,
    app: SkillAppId,
    enabled: boolean,
  ) => {
    try {
      await linkMutation.mutateAsync({
        directory: skill.directory,
        app,
        enabled,
      });
      toast.success(
        enabled
          ? t("skills.global.linkSuccess", {
              name: skill.name,
              appName: t(`apps.${app}`),
            })
          : t("skills.global.unlinkSuccess", {
              name: skill.name,
              appName: t(`apps.${app}`),
            }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(t("skills.global.linkFailed"), {
        description: String(error),
        duration: 10000,
      });
    }
  };

  const handleUninstall = (skill: GlobalSkill) => {
    setConfirmDialog({
      title: t("skills.uninstall"),
      message: t("skills.global.uninstallConfirm", { name: skill.name }),
      onConfirm: async () => {
        try {
          const result = await uninstallMutation.mutateAsync(skill.directory);
          setConfirmDialog(null);
          toast.success(t("skills.uninstallSuccess", { name: skill.name }), {
            description: result.backupPath
              ? t("skills.backup.location", { path: result.backupPath })
              : undefined,
            closeButton: true,
          });
        } catch (error) {
          toast.error(t("common.error"), { description: String(error) });
        }
      },
    });
  };

  const handleInstallFromZip = async () => {
    try {
      const filePath = await skillsApi.openZipFileDialog();
      if (!filePath) return;
      const installed = await installFromZipMutation.mutateAsync(filePath);
      if (installed.length === 0) {
        toast.info(t("skills.installFromZip.noSkillsFound"), {
          closeButton: true,
        });
      } else {
        toast.success(
          installed.length === 1
            ? t("skills.installFromZip.successSingle", {
                name: installed[0].name,
              })
            : t("skills.installFromZip.successMultiple", {
                count: installed.length,
              }),
          { closeButton: true },
        );
      }
    } catch (error) {
      toast.error(t("skills.installFailed"), {
        description: String(error),
      });
    }
  };

  const handleOpenRestore = async () => {
    setRestoreDialogOpen(true);
    try {
      await refetchBackups();
    } catch (error) {
      toast.error(t("common.error"), { description: String(error) });
    }
  };

  const handleRestore = async (backupId: string) => {
    try {
      const restored = await restoreMutation.mutateAsync(backupId);
      setRestoreDialogOpen(false);
      toast.success(
        t("skills.restoreFromBackup.success", { name: restored.name }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(t("skills.restoreFromBackup.failed"), {
        description: String(error),
      });
    }
  };

  const handleDeleteBackup = (backup: SkillBackupEntry) => {
    setConfirmDialog({
      title: t("skills.restoreFromBackup.deleteConfirmTitle"),
      message: t("skills.restoreFromBackup.deleteConfirmMessage", {
        name: backup.skill.name,
      }),
      confirmText: t("skills.restoreFromBackup.delete"),
      onConfirm: async () => {
        try {
          await deleteBackupMutation.mutateAsync(backup.backupId);
          setConfirmDialog(null);
          toast.success(
            t("skills.restoreFromBackup.deleteSuccess", {
              name: backup.skill.name,
            }),
            { closeButton: true },
          );
        } catch (error) {
          toast.error(t("skills.restoreFromBackup.deleteFailed"), {
            description: String(error),
          });
        }
      },
    });
  };

  React.useImperativeHandle(ref, () => ({
    refresh: () => {
      void refetch();
    },
    openInstallFromZip: () => {
      void handleInstallFromZip();
    },
    openRestoreFromBackup: () => {
      void handleOpenRestore();
    },
  }));

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <AppCountBar
            totalLabel={
              <div className="flex items-center gap-1">
                <SkillScopeSelect
                  scope={{ kind: "global" }}
                  appOptions={appOptions}
                  onScopeChange={onScopeChange}
                />
                <span>
                  {t("skills.installedCount", { count: skills.length })}
                </span>
              </div>
            }
            counts={linkedCounts}
          />
          {data?.skillsDir && (
            <p
              className="-mt-2 truncate pb-3 text-xs text-zinc-400 dark:text-zinc-500"
              title={data.skillsDir}
            >
              {data.skillsDir}
            </p>
          )}
        </div>
        <div className="flex flex-shrink-0 items-center gap-1.5 pt-3">
          {skillUpdates.length > 0 && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 gap-1 text-xs"
              onClick={() => void handleUpdateAll()}
              disabled={isUpdatingAll || updateMutation.isPending}
            >
              {isUpdatingAll ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <RefreshCw size={12} />
              )}
              {t("skills.updateAll", { count: skillUpdates.length })}
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={() => void handleCheckUpdates()}
            disabled={isCheckingUpdates || skills.length === 0}
          >
            {isCheckingUpdates ? (
              <Loader2 size={12} className="animate-spin" />
            ) : (
              <RefreshCw size={12} />
            )}
            {isCheckingUpdates
              ? t("skills.checkingUpdates")
              : t("skills.checkUpdates")}
          </Button>
          {isFetching && !isLoading && (
            <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto overflow-x-hidden pb-24">
        {isLoading ? (
          <div className="py-12 text-center text-muted-foreground">
            {t("skills.loading")}
          </div>
        ) : skills.length === 0 ? (
          <div className="py-12 text-center">
            <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-zinc-100 dark:bg-zinc-800">
              <Sparkles size={24} className="text-muted-foreground/60" />
            </div>
            <h3 className="mb-2 text-lg font-medium text-foreground">
              {t("skills.global.empty")}
            </h3>
            <p className="text-sm text-muted-foreground">
              {t("skills.global.description")}
            </p>
          </div>
        ) : (
          <TooltipProvider delayDuration={300}>
            <div className="divide-y divide-zinc-100 overflow-hidden rounded-xl border border-zinc-100 bg-white shadow-[0_1px_3px_rgba(0,0,0,0.01)] dark:divide-zinc-900 dark:border-zinc-800 dark:bg-zinc-950">
              {skills.map((skill, index) => (
                <GlobalSkillListItem
                  key={skill.directory}
                  skill={skill}
                  directApps={data?.directApps ?? {}}
                  directAppReasons={directAppReasons}
                  isLast={index === skills.length - 1}
                  isMutating={
                    linkMutation.isPending ||
                    uninstallMutation.isPending ||
                    updateMutation.isPending
                  }
                  hasUpdate={Boolean(updatesMap[skill.id])}
                  isUpdating={
                    updateMutation.isPending &&
                    updateMutation.variables === skill.id
                  }
                  onToggle={(app, enabled) =>
                    void handleToggle(skill, app, enabled)
                  }
                  onUpdate={() => void handleUpdate(skill)}
                  onUninstall={() => handleUninstall(skill)}
                />
              ))}
            </div>
          </TooltipProvider>
        )}
      </div>

      {confirmDialog && (
        <ConfirmDialog
          isOpen
          title={confirmDialog.title}
          message={confirmDialog.message}
          confirmText={confirmDialog.confirmText}
          variant="destructive"
          zIndex="top"
          onConfirm={confirmDialog.onConfirm}
          onCancel={() => setConfirmDialog(null)}
        />
      )}

      <SkillBackupDialog
        backups={backups}
        isDeleting={deleteBackupMutation.isPending}
        isLoading={isFetchingBackups}
        isRestoring={restoreMutation.isPending}
        open={restoreDialogOpen}
        onClose={() => setRestoreDialogOpen(false)}
        onDelete={handleDeleteBackup}
        onRestore={handleRestore}
      />
    </div>
  );
});

function GlobalSkillListItem({
  skill,
  directApps,
  directAppReasons,
  isLast,
  isMutating,
  hasUpdate,
  isUpdating,
  onToggle,
  onUpdate,
  onUninstall,
}: {
  skill: GlobalSkill;
  directApps: Partial<Record<SkillAppId, boolean>>;
  directAppReasons: Partial<Record<SkillAppId, string>>;
  isLast: boolean;
  isMutating: boolean;
  hasUpdate: boolean;
  isUpdating: boolean;
  onToggle: (app: SkillAppId, enabled: boolean) => void;
  onUpdate: () => void;
  onUninstall: () => void;
}) {
  const { t } = useTranslation();
  const source =
    skill.repoOwner && skill.repoName
      ? `${skill.repoOwner}/${skill.repoName}`
      : t("skills.local");

  const openDocs = async () => {
    if (!skill.readmeUrl) return;
    try {
      await settingsApi.openExternal(skill.readmeUrl);
    } catch {
      // Keep the row usable if opening the external URL fails.
    }
  };

  return (
    <ListItemRow isLast={isLast}>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
            {skill.name}
          </span>
          {skill.readmeUrl && (
            <button
              type="button"
              onClick={() => void openDocs()}
              className="flex-shrink-0 text-muted-foreground/60 hover:text-foreground"
              title={t("skills.openDocs")}
            >
              <ExternalLink size={12} />
            </button>
          )}
          <span className="flex-shrink-0 text-xs text-zinc-400 dark:text-zinc-500">
            {source}
          </span>
          <span className="inline-flex h-4 shrink-0 items-center gap-1 rounded-sm bg-emerald-500/10 px-1.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
            <Link2 size={10} />
            {t("skills.global.source")}
          </span>
          {hasUpdate && (
            <span className="inline-flex h-4 shrink-0 items-center rounded-sm bg-amber-500/10 px-1.5 text-[10px] font-medium text-amber-600 dark:text-amber-400">
              {t("skills.updateAvailable")}
            </span>
          )}
        </div>
        {skill.description && (
          <p
            className="truncate text-xs text-zinc-400 dark:text-zinc-500"
            title={skill.description}
          >
            {skill.description}
          </p>
        )}
      </div>

      <AppToggleGroup
        apps={skill.apps}
        disabled={isMutating}
        disabledApps={directApps}
        disabledReasons={directAppReasons}
        onToggle={onToggle}
      />

      {hasUpdate && (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 flex-shrink-0 rounded-lg text-zinc-500 hover:bg-zinc-900/5 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/5 dark:hover:text-zinc-100"
          disabled={isMutating}
          onClick={onUpdate}
          title={t("skills.update")}
        >
          {isUpdating ? (
            <Loader2 size={14} className="animate-spin" />
          ) : (
            <RefreshCw size={14} />
          )}
        </Button>
      )}

      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="h-7 w-7 flex-shrink-0 rounded-lg text-zinc-500 opacity-0 transition-opacity hover:bg-red-500/10 hover:text-red-600 group-hover:opacity-100 dark:text-zinc-400 dark:hover:bg-red-500/15 dark:hover:text-red-400"
        disabled={isMutating}
        onClick={onUninstall}
        title={t("skills.uninstall")}
      >
        {isMutating ? (
          <Loader2 size={14} className="animate-spin" />
        ) : (
          <Trash2 size={14} />
        )}
      </Button>
    </ListItemRow>
  );
}

GlobalSkillsPanel.displayName = "GlobalSkillsPanel";

export default GlobalSkillsPanel;
