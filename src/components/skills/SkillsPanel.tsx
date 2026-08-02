import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ExternalLink,
  Link2,
  Loader2,
  RefreshCw,
  Sparkles,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ListItemRow } from "@/components/common/ListItemRow";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SkillBackupDialog } from "@/components/skills/SkillBackupDialog";
import {
  SkillScopeSelect,
  type SkillManagementScope,
} from "@/components/skills/SkillScopeSelect";
import {
  type AppSkill,
  type CliProvidedSkill,
  type GlobalSkill,
  type SkillAppId,
  type SkillBackupEntry,
  type SkillUpdateInfo,
  useAppSkillBackups,
  useAppSkills,
  useCheckAppSkillUpdates,
  useCliProvidedSkills,
  useDeleteSkillBackup,
  useInstallSkillsFromZip,
  useGlobalSkills,
  useRestoreSkillBackup,
  useUninstallSkill,
  useUpdateAppSkill,
} from "@/hooks/useSkills";
import { settingsApi, skillsApi } from "@/lib/api";

interface SkillsPanelProps {
  appId: SkillAppId;
  appOptions?: readonly SkillAppId[];
  onScopeChange: (scope: SkillManagementScope) => void;
}

export interface SkillsPanelHandle {
  refresh: () => void;
  openInstallFromZip: () => void;
  openRestoreFromBackup: () => void;
}

function skillIdentity(skill: Pick<AppSkill, "directory" | "name">): string {
  const normalizedName = skill.name.trim().toLocaleLowerCase();
  return normalizedName || skill.directory.trim().toLocaleLowerCase();
}

function globalSkillAsAppSkill(skill: GlobalSkill): AppSkill {
  return {
    id: skill.id,
    name: skill.name,
    description: skill.description,
    directory: skill.directory,
    path: skill.path,
    isSymlink: false,
    managedGlobally: true,
    globalSource: true,
    repoOwner: skill.repoOwner,
    repoName: skill.repoName,
    repoBranch: skill.repoBranch,
    readmeUrl: skill.readmeUrl,
    installedAt: skill.installedAt,
    contentHash: skill.contentHash,
    updatedAt: skill.updatedAt,
  };
}

function providedSkillAsAppSkill(skill: CliProvidedSkill): AppSkill {
  return {
    id: skill.id,
    name: skill.name,
    description: skill.description,
    directory: skill.directory,
    path: skill.path,
    isSymlink: false,
    managedGlobally: false,
    globalSource: false,
    providedBy: skill.source,
    installedAt: 0,
    updatedAt: 0,
  };
}

function mergeEffectiveSkills(
  appSkills: AppSkill[],
  globalSkills: GlobalSkill[],
  providedSkills: CliProvidedSkill[],
): AppSkill[] {
  const skillsByIdentity = new Map<string, AppSkill>();

  for (const skill of appSkills) {
    skillsByIdentity.set(skillIdentity(skill), skill);
  }
  for (const skill of globalSkills) {
    const identity = skillIdentity(skill);
    if (!skillsByIdentity.has(identity)) {
      skillsByIdentity.set(identity, globalSkillAsAppSkill(skill));
    }
  }
  for (const skill of providedSkills) {
    const identity = skillIdentity(skill);
    const existing = skillsByIdentity.get(identity);
    if (existing && skill.source.kind === "plugin") {
      skillsByIdentity.set(identity, {
        ...existing,
        providedBy: skill.source,
      });
    } else if (!existing) {
      skillsByIdentity.set(identity, providedSkillAsAppSkill(skill));
    }
  }

  return [...skillsByIdentity.values()].sort((left, right) =>
    left.name.localeCompare(right.name),
  );
}

const SkillsPanel = React.forwardRef<SkillsPanelHandle, SkillsPanelProps>(
  ({ appId, appOptions, onScopeChange }, ref) => {
    const { t } = useTranslation();
    const [restoreDialogOpen, setRestoreDialogOpen] = useState(false);
    const [isUpdatingAll, setIsUpdatingAll] = useState(false);
    const [confirmDialog, setConfirmDialog] = useState<{
      title: string;
      message: string;
      confirmText?: string;
      onConfirm: () => void;
    } | null>(null);

    const { data, isLoading, isFetching, refetch } = useAppSkills(appId);
    const {
      data: globalData,
      isLoading: isLoadingGlobal,
      isFetching: isFetchingGlobal,
      refetch: refetchGlobal,
    } = useGlobalSkills();
    const loadsSharedGlobalSkills = globalData?.directApps[appId] ?? false;
    const {
      data: providedSkills = [],
      isLoading: isLoadingProvided,
      isFetching: isFetchingProvided,
      refetch: refetchProvided,
    } = useCliProvidedSkills(appId);
    const {
      data: backups = [],
      isFetching: isFetchingBackups,
      refetch: refetchBackups,
    } = useAppSkillBackups(appId);
    const uninstallMutation = useUninstallSkill();
    const restoreMutation = useRestoreSkillBackup();
    const deleteBackupMutation = useDeleteSkillBackup(appId);
    const installFromZipMutation = useInstallSkillsFromZip();
    const {
      data: skillUpdates = [],
      isFetching: isCheckingUpdates,
      refetch: checkUpdates,
    } = useCheckAppSkillUpdates(appId);
    const updateMutation = useUpdateAppSkill();

    const handleCheckUpdates = async () => {
      try {
        const result = await checkUpdates();
        const updates = (result.data ?? []).filter(
          (update) => !providedSkillIds.has(update.id),
        );
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

    const handleUpdate = async (skill: AppSkill) => {
      if (skill.providedBy) return;
      try {
        const updated = await updateMutation.mutateAsync({
          app: appId,
          id: skill.id,
        });
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
      if (actionableUpdates.length === 0) return;
      setIsUpdatingAll(true);
      let successCount = 0;
      for (const update of actionableUpdates) {
        try {
          await updateMutation.mutateAsync({ app: appId, id: update.id });
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

    const handleUninstall = (skill: AppSkill) => {
      if (skill.providedBy) return;
      setConfirmDialog({
        title: t("skills.uninstall"),
        message: skill.managedGlobally
          ? t("skills.global.unlinkConfirmForApp", {
              name: skill.name,
              appName: t(`apps.${appId}`),
            })
          : t("skills.uninstallConfirmForApp", {
              name: skill.name,
              appName: t(`apps.${appId}`),
            }),
        onConfirm: async () => {
          try {
            const result = await uninstallMutation.mutateAsync({
              app: appId,
              directory: skill.directory,
            });
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
        const installed = await installFromZipMutation.mutateAsync({
          filePath,
          currentApp: appId,
        });
        if (installed.length === 0) {
          toast.info(t("skills.installFromZip.noSkillsFound"), {
            closeButton: true,
          });
        } else if (installed.length === 1) {
          toast.success(
            t("skills.installFromZip.successSingle", {
              name: installed[0].name,
            }),
            { closeButton: true },
          );
        } else {
          toast.success(
            t("skills.installFromZip.successMultiple", {
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
        const restored = await restoreMutation.mutateAsync({
          backupId,
          currentApp: appId,
        });
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
        void refetchGlobal();
        void refetchProvided();
      },
      openInstallFromZip: () => {
        void handleInstallFromZip();
      },
      openRestoreFromBackup: () => {
        void handleOpenRestore();
      },
    }));

    const skills = React.useMemo(
      () =>
        mergeEffectiveSkills(
          data?.skills ?? [],
          loadsSharedGlobalSkills ? (globalData?.skills ?? []) : [],
          providedSkills,
        ),
      [
        providedSkills,
        data?.skills,
        globalData?.skills,
        loadsSharedGlobalSkills,
      ],
    );
    const providedSkillIds = React.useMemo(
      () =>
        new Set(
          skills
            .filter((skill) => Boolean(skill.providedBy))
            .map((skill) => skill.id),
        ),
      [skills],
    );
    const actionableUpdates = React.useMemo(
      () => skillUpdates.filter((update) => !providedSkillIds.has(update.id)),
      [providedSkillIds, skillUpdates],
    );
    const updatesMap = React.useMemo(
      () =>
        Object.fromEntries(
          actionableUpdates.map((update) => [update.id, update]),
        ) as Record<string, SkillUpdateInfo>,
      [actionableUpdates],
    );
    const isLoadingEffective =
      isLoading || isLoadingProvided || isLoadingGlobal;
    const isFetchingEffective =
      isFetching || isFetchingProvided || isFetchingGlobal;
    const skillDirectories = [
      data?.skillsDir,
      loadsSharedGlobalSkills ? globalData?.skillsDir : undefined,
    ].filter((directory): directory is string => Boolean(directory));

    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-6">
        <div className="flex items-center justify-between pb-3">
          <div className="min-w-0">
            <div className="flex items-center gap-1 text-sm font-medium text-zinc-700 dark:text-zinc-200">
              <SkillScopeSelect
                scope={{ kind: "app", app: appId }}
                appOptions={appOptions}
                onScopeChange={onScopeChange}
              />
              <span>
                {t("skills.installedCount", { count: skills.length })}
              </span>
            </div>
            {skillDirectories.length > 0 && (
              <p
                className="mt-1 truncate text-xs text-zinc-400 dark:text-zinc-500"
                title={skillDirectories.join(" · ")}
              >
                {skillDirectories.join(" · ")}
              </p>
            )}
          </div>
          <div className="flex flex-shrink-0 items-center gap-1.5">
            {actionableUpdates.length > 0 && (
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
                {t("skills.updateAll", { count: actionableUpdates.length })}
              </Button>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-7 gap-1 text-xs"
              onClick={() => void handleCheckUpdates()}
              disabled={isCheckingUpdates || (data?.skills.length ?? 0) === 0}
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
            {isFetchingEffective && !isLoadingEffective && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto overflow-x-hidden pb-24">
          {isLoadingEffective ? (
            <div className="py-12 text-center text-muted-foreground">
              {t("skills.loading")}
            </div>
          ) : skills.length === 0 ? (
            <div className="py-12 text-center">
              <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-zinc-100 dark:bg-zinc-800">
                <Sparkles size={24} className="text-muted-foreground/60" />
              </div>
              <h3 className="mb-2 text-lg font-medium text-foreground">
                {t("skills.noInstalledForApp", {
                  appName: t(`apps.${appId}`),
                })}
              </h3>
              <p className="text-sm text-muted-foreground">
                {t("skills.noInstalledDescription")}
              </p>
            </div>
          ) : (
            <TooltipProvider delayDuration={300}>
              <div className="divide-y divide-zinc-100 overflow-hidden rounded-xl border border-zinc-100 bg-white shadow-[0_1px_3px_rgba(0,0,0,0.01)] dark:divide-zinc-900 dark:border-zinc-800 dark:bg-zinc-950">
                {skills.map((skill, index) => (
                  <SkillListItem
                    key={skill.directory}
                    skill={skill}
                    isLast={index === skills.length - 1}
                    isUninstalling={
                      uninstallMutation.isPending &&
                      uninstallMutation.variables?.directory === skill.directory
                    }
                    hasUpdate={
                      Boolean(updatesMap[skill.id]) && !skill.providedBy
                    }
                    isUpdating={
                      updateMutation.isPending &&
                      updateMutation.variables?.id === skill.id
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
  },
);

SkillsPanel.displayName = "SkillsPanel";

function SkillListItem({
  skill,
  isLast,
  isUninstalling,
  hasUpdate,
  isUpdating,
  onUpdate,
  onUninstall,
}: {
  skill: AppSkill;
  isLast: boolean;
  isUninstalling: boolean;
  hasUpdate: boolean;
  isUpdating: boolean;
  onUpdate: () => void;
  onUninstall: () => void;
}) {
  const { t } = useTranslation();
  const source = skill.providedBy
    ? null
    : skill.repoOwner && skill.repoName
      ? `${skill.repoOwner}/${skill.repoName}`
      : t("skills.local");

  const openDocs = async () => {
    if (!skill.readmeUrl) return;
    try {
      await settingsApi.openExternal(skill.readmeUrl);
    } catch {
      // The row remains usable even if opening the external URL fails.
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
          {source && (
            <span className="flex-shrink-0 text-xs text-zinc-400 dark:text-zinc-500">
              {source}
            </span>
          )}
          {skill.providedBy?.kind === "builtin" && (
            <span className="inline-flex h-4 shrink-0 items-center rounded-sm bg-blue-500/10 px-1.5 text-[10px] font-medium text-blue-600 dark:text-blue-400">
              {t("skills.builtin")}
            </span>
          )}
          {skill.providedBy?.kind === "plugin" && (
            <span className="inline-flex h-4 shrink-0 items-center rounded-sm bg-violet-500/10 px-1.5 text-[10px] font-medium text-violet-600 dark:text-violet-400">
              {t("skills.pluginProvided", {
                pluginName: skill.providedBy.pluginName,
              })}
            </span>
          )}
          {skill.managedGlobally && (
            <span className="inline-flex h-4 shrink-0 items-center gap-1 rounded-sm bg-emerald-500/10 px-1.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
              <Link2 size={10} />
              {skill.globalSource
                ? t("skills.global.source")
                : t("skills.global.linked")}
            </span>
          )}
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
      {hasUpdate && (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 flex-shrink-0 rounded-lg text-zinc-500 hover:bg-zinc-900/5 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/5 dark:hover:text-zinc-100"
          disabled={isUpdating}
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
      {!skill.globalSource && !skill.providedBy && (
        <div className="flex-shrink-0 opacity-0 transition-opacity group-hover:opacity-100">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7 rounded-lg text-zinc-500 hover:bg-red-500/10 hover:text-red-600 dark:text-zinc-400 dark:hover:bg-red-500/15 dark:hover:text-red-400"
            disabled={isUninstalling}
            onClick={onUninstall}
            title={t("skills.uninstall")}
          >
            {isUninstalling ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <Trash2 size={14} />
            )}
          </Button>
        </div>
      )}
    </ListItemRow>
  );
}

export default SkillsPanel;
