import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { motion } from "framer-motion";
import {
  Loader2,
  Save,
  Settings2,
  Globe,
  Key,
  Sliders,
  BarChart3,
  Info,
  ArrowLeft,
} from "lucide-react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { settingsApi } from "@/lib/api";
import { LanguageSettings } from "@/components/settings/LanguageSettings";
import { ThemeSettings } from "@/components/settings/ThemeSettings";
import { WindowSettings } from "@/components/settings/WindowSettings";
import { AppVisibilitySettings } from "@/components/settings/AppVisibilitySettings";
import { TerminalSettings } from "@/components/settings/TerminalSettings";
import { ModelsDevMetadataSettings } from "@/components/settings/ModelsDevMetadataSettings";
import { OpenCodeSmallModelSettings } from "@/components/settings/OpenCodeSmallModelSettings";
import { OpenCodeWebSearchSettings } from "@/components/settings/OpenCodeWebSearchSettings";
import {
  SettingSection,
  SettingsPane,
} from "@/components/settings/SettingSection";
import {
  SettingsDisclosure,
  SettingsDisclosureGroup,
} from "@/components/settings/SettingsDisclosure";
import { getTerminalFooter } from "@/components/settings/TerminalSettings";
import { DirectorySettings } from "@/components/settings/DirectorySettings";
import { ImportExportSection } from "@/components/settings/ImportExportSection";
import { BackupListSection } from "@/components/settings/BackupListSection";
import { WebdavSyncSection } from "@/components/settings/WebdavSyncSection";
import { AboutSection } from "@/components/settings/AboutSection";
import { ProxyTabContent } from "@/components/settings/ProxyTabContent";
import { ModelTestConfigPanel } from "@/components/usage/ModelTestConfigPanel";
import { UsageDashboard } from "@/components/usage/UsageDashboard";
import { LogConfigPanel } from "@/components/settings/LogConfigPanel";
import { AuthCenterPanel } from "@/components/settings/AuthCenterPanel";
import { CodexAuthSettings } from "@/components/settings/CodexAuthSettings";
import { ClaudeDesktopSettings } from "@/components/settings/ClaudeDesktopSettings";
import { useSettings } from "@/hooks/useSettings";
import { useImportExport } from "@/hooks/useImportExport";
import { useTranslation } from "react-i18next";
import { DRAG_REGION_ATTR } from "@/lib/platform";
import type { SettingsFormState } from "@/hooks/useSettings";

interface SettingsPageProps {
  onClose: () => void;
  onImportSuccess?: () => void | Promise<void>;
  defaultTab?: string;
}

export function SettingsPage({
  onClose,
  onImportSuccess,
  defaultTab = "general",
}: SettingsPageProps) {
  const { t } = useTranslation();
  const {
    settings,
    isLoading,
    isSaving,
    isPortable,
    appConfigDir,
    resolvedDirs,
    updateSettings,
    updateDirectory,
    updateAppConfigDir,
    browseDirectory,
    browseAppConfigDir,
    resetDirectory,
    resetAppConfigDir,
    saveSettings,
    autoSaveSettings,
    requiresRestart,
    acknowledgeRestart,
  } = useSettings();

  const {
    selectedFile,
    status: importStatus,
    errorMessage,
    backupId,
    isImporting,
    selectImportFile,
    importConfig,
    exportConfig,
    clearSelection,
    resetStatus,
  } = useImportExport({ onImportSuccess });

  const [activeTab, setActiveTab] = useState<string>("general");
  const [showRestartPrompt, setShowRestartPrompt] = useState(false);
  const tabScrollContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setActiveTab(defaultTab);
    resetStatus();
  }, [resetStatus, defaultTab]);

  useEffect(() => {
    if (requiresRestart) {
      setShowRestartPrompt(true);
    }
  }, [requiresRestart]);

  useLayoutEffect(() => {
    if (tabScrollContainerRef.current) {
      tabScrollContainerRef.current.scrollTop = 0;
    }
  }, [activeTab]);

  const afterSaveCleanup = useCallback(() => {
    acknowledgeRestart();
    clearSelection();
    resetStatus();
  }, [acknowledgeRestart, clearSelection, resetStatus]);

  const handleSave = useCallback(async () => {
    try {
      const result = await saveSettings(undefined, { silent: false });
      if (!result) return;
      if (result.requiresRestart) {
        setShowRestartPrompt(true);
        return;
      }
      afterSaveCleanup();
    } catch (error) {
      console.error("[SettingsPage] Failed to save settings", error);
    }
  }, [afterSaveCleanup, saveSettings]);

  const handleRestartLater = useCallback(() => {
    setShowRestartPrompt(false);
    afterSaveCleanup();
  }, [afterSaveCleanup]);

  const handleRestartNow = useCallback(async () => {
    setShowRestartPrompt(false);
    if (import.meta.env.DEV) {
      toast.success(t("settings.devModeRestartHint"), { closeButton: true });
      afterSaveCleanup();
      return;
    }

    try {
      await settingsApi.restart();
    } catch (error) {
      console.error("[SettingsPage] Failed to restart app", error);
      toast.error(t("settings.restartFailed"));
    } finally {
      afterSaveCleanup();
    }
  }, [afterSaveCleanup, t]);

  // 通用设置即时保存（无需手动点击）
  // 使用 autoSaveSettings 避免误触发系统 API（开机自启、Claude 插件等）
  // 返回保存是否成功：需要在保存成功后追加动作的调用方（如统一会话历史
  // 关闭后的备份还原）据此短路，其余调用方可忽略返回值。
  const handleAutoSave = useCallback(
    async (updates: Partial<SettingsFormState>): Promise<boolean> => {
      if (!settings) return false;
      // 乐观更新前捕获旧值：autoSaveSettings 发送的是全量表单状态，后端按
      // diff 触发副作用（如统一会话开关的 live 重写与历史迁移）。保存失败
      // 不回滚的话，失败的变更会滞留在表单里，被之后任意一次无关保存原样
      // 重放，绕过确认弹窗。
      const previousValues = Object.fromEntries(
        Object.keys(updates).map((key) => [
          key,
          settings[key as keyof SettingsFormState],
        ]),
      ) as Partial<SettingsFormState>;
      updateSettings(updates);
      try {
        await autoSaveSettings(updates);
        return true;
      } catch (error) {
        console.error("[SettingsPage] Failed to autosave settings", error);
        updateSettings(previousValues);
        toast.error(
          t("settings.saveFailedGeneric", {
            defaultValue: "保存失败，请重试",
          }),
        );
        return false;
      }
    },
    [autoSaveSettings, settings, t, updateSettings],
  );

  const isBusy = useMemo(() => isLoading && !settings, [isLoading, settings]);

  return (
    <>
      <div className="flex h-full min-h-0 flex-1 overflow-hidden bg-background">
        {isBusy ? (
          <div className="flex flex-1 items-center justify-center">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <Tabs
            value={activeTab}
            onValueChange={setActiveTab}
            className="flex h-full min-h-0 w-full flex-row overflow-hidden"
          >
            {/* 左侧侧边栏导航 */}
            <div
              className="flex w-[260px] flex-shrink-0 flex-col border-r border-border/60 bg-muted/30"
              style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
            >
              <div
                className="flex shrink-0 items-center gap-2 border-b border-border/50 px-5 py-4"
                {...DRAG_REGION_ATTR}
              >
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={onClose}
                  className="h-8 w-8 rounded-lg"
                  title={t("common.back", { defaultValue: "返回" })}
                >
                  <ArrowLeft className="h-4 w-4" />
                </Button>
                <span className="text-[15px] font-semibold tracking-tight text-foreground">
                  {t("common.settings")}
                </span>
              </div>
              <TabsList className="flex h-auto w-full flex-col items-stretch gap-0.5 overflow-y-auto bg-transparent p-3">
                <SidebarTabTrigger value="general" icon={Settings2}>
                  {t("settings.tabGeneral")}
                </SidebarTabTrigger>
                <SidebarTabTrigger value="proxy" icon={Globe}>
                  {t("settings.tabProxy")}
                </SidebarTabTrigger>
                <SidebarTabTrigger value="auth" icon={Key}>
                  {t("settings.tabAuth", { defaultValue: "认证" })}
                </SidebarTabTrigger>
                <SidebarTabTrigger value="advanced" icon={Sliders}>
                  {t("settings.tabAdvanced")}
                </SidebarTabTrigger>
                <SidebarTabTrigger value="usage" icon={BarChart3}>
                  {t("usage.title")}
                </SidebarTabTrigger>
                <SidebarTabTrigger value="about" icon={Info}>
                  {t("common.about")}
                </SidebarTabTrigger>
              </TabsList>
            </div>

            {/* 右侧主设置内容区 */}
            <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-10 py-8">
                <div
                  ref={tabScrollContainerRef}
                  className="scrollbar-thin min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
                >
                  <TabsContent value="general" className="mt-0">
                    {settings ? (
                      <motion.div
                        initial={{ opacity: 0, y: 6 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ duration: 0.2 }}
                      >
                        <SettingsPane title={t("settings.tabGeneral")}>
                          <SettingSection
                            title={t("settings.sections.appearance")}
                          >
                            <LanguageSettings
                              value={settings.language}
                              onChange={(lang) =>
                                handleAutoSave({ language: lang })
                              }
                            />
                            <ThemeSettings />
                          </SettingSection>

                          <SettingSection
                            title={t("settings.sections.apps")}
                            footer={t("settings.appVisibility.description")}
                          >
                            <AppVisibilitySettings
                              settings={settings}
                              onChange={handleAutoSave}
                            />
                          </SettingSection>

                          <SettingSection
                            title={t("settings.sections.integration")}
                          >
                            <CodexAuthSettings
                              settings={settings}
                              onChange={handleAutoSave}
                            />
                            <ClaudeDesktopSettings />
                          </SettingSection>

                          <SettingSection
                            title={t("settings.sections.opencode")}
                          >
                            <OpenCodeWebSearchSettings />
                            <OpenCodeSmallModelSettings />
                          </SettingSection>

                          <SettingSection
                            title={t("settings.sections.modelMetadata")}
                          >
                            <ModelsDevMetadataSettings
                              autoUpdate={settings.modelsDevAutoUpdate ?? true}
                              intervalHours={
                                settings.modelsDevUpdateIntervalHours ?? 24
                              }
                              onChange={handleAutoSave}
                            />
                          </SettingSection>

                          <SettingSection
                            title={t("settings.sections.system")}
                            footer={getTerminalFooter(t)}
                          >
                            <WindowSettings
                              settings={settings}
                              onChange={handleAutoSave}
                            />
                            <TerminalSettings
                              value={settings.preferredTerminal}
                              onChange={(terminal) =>
                                handleAutoSave({ preferredTerminal: terminal })
                              }
                            />
                          </SettingSection>
                        </SettingsPane>
                      </motion.div>
                    ) : null}
                  </TabsContent>

                  <TabsContent value="proxy" className="mt-0 pb-4">
                    {settings ? (
                      <SettingsPane title={t("settings.tabProxy")}>
                        <ProxyTabContent
                          settings={settings}
                          onAutoSave={handleAutoSave}
                        />
                      </SettingsPane>
                    ) : null}
                  </TabsContent>

                  <TabsContent value="auth" className="mt-0 pb-4">
                    <motion.div
                      initial={{ opacity: 0, y: 6 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.2 }}
                    >
                      <SettingsPane
                        title={t("settings.tabAuth", { defaultValue: "认证" })}
                      >
                        <AuthCenterPanel />
                      </SettingsPane>
                    </motion.div>
                  </TabsContent>

                  <TabsContent value="advanced" className="mt-0 pb-4">
                    {settings ? (
                      <motion.div
                        initial={{ opacity: 0, y: 6 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ duration: 0.2 }}
                      >
                        <SettingsPane title={t("settings.tabAdvanced")}>
                          <SettingsDisclosureGroup>
                            <SettingsDisclosure
                              value="directory"
                              title={t("settings.advanced.configDir.title")}
                              description={t(
                                "settings.advanced.configDir.description",
                              )}
                            >
                              <DirectorySettings
                                appConfigDir={appConfigDir}
                                resolvedDirs={resolvedDirs}
                                onAppConfigChange={updateAppConfigDir}
                                onBrowseAppConfig={browseAppConfigDir}
                                onResetAppConfig={resetAppConfigDir}
                                claudeDir={settings.claudeConfigDir}
                                codexDir={settings.codexConfigDir}
                                geminiDir={settings.geminiConfigDir}
                                opencodeDir={settings.opencodeConfigDir}
                                openclawDir={settings.openclawConfigDir}
                                hermesDir={settings.hermesConfigDir}
                                onDirectoryChange={updateDirectory}
                                onBrowseDirectory={browseDirectory}
                                onResetDirectory={resetDirectory}
                              />
                            </SettingsDisclosure>

                            <SettingsDisclosure
                              value="data"
                              title={t("settings.advanced.data.title")}
                              description={t(
                                "settings.advanced.data.description",
                              )}
                            >
                              <ImportExportSection
                                status={importStatus}
                                selectedFile={selectedFile}
                                errorMessage={errorMessage}
                                backupId={backupId}
                                isImporting={isImporting}
                                onSelectFile={selectImportFile}
                                onImport={importConfig}
                                onExport={exportConfig}
                                onClear={clearSelection}
                              />
                            </SettingsDisclosure>

                            <SettingsDisclosure
                              value="backup"
                              title={t("settings.advanced.backup.title", {
                                defaultValue: "Backup & Restore",
                              })}
                              description={t(
                                "settings.advanced.backup.description",
                                {
                                  defaultValue:
                                    "Manage automatic backups, view and restore database snapshots",
                                },
                              )}
                            >
                              <BackupListSection
                                backupIntervalHours={
                                  settings.backupIntervalHours
                                }
                                backupRetainCount={settings.backupRetainCount}
                                onSettingsChange={(updates) =>
                                  handleAutoSave(updates)
                                }
                              />
                            </SettingsDisclosure>

                            <SettingsDisclosure
                              value="cloudSync"
                              title={t("settings.advanced.cloudSync.title")}
                              description={t(
                                "settings.advanced.cloudSync.description",
                              )}
                            >
                              <WebdavSyncSection
                                config={settings?.webdavSync}
                                s3Config={settings?.s3Sync}
                                settings={settings}
                                onAutoSave={handleAutoSave}
                              />
                            </SettingsDisclosure>

                            <SettingsDisclosure
                              value="test"
                              title={t("settings.advanced.modelTest.title")}
                              description={t(
                                "settings.advanced.modelTest.description",
                              )}
                            >
                              <ModelTestConfigPanel />
                            </SettingsDisclosure>

                            <SettingsDisclosure
                              value="logConfig"
                              title={t("settings.advanced.logConfig.title")}
                              description={t(
                                "settings.advanced.logConfig.description",
                              )}
                            >
                              <LogConfigPanel />
                            </SettingsDisclosure>
                          </SettingsDisclosureGroup>
                        </SettingsPane>
                      </motion.div>
                    ) : null}
                  </TabsContent>

                  <TabsContent value="about" className="mt-0">
                    <SettingsPane title={t("common.about")}>
                      <AboutSection isPortable={isPortable} />
                    </SettingsPane>
                  </TabsContent>

                  <TabsContent value="usage" className="mt-0">
                    <SettingsPane
                      wide
                      title={t("usage.title")}
                      subtitle={t("usage.subtitle")}
                    >
                      <UsageDashboard embedded />
                    </SettingsPane>
                  </TabsContent>
                </div>

                {activeTab === "advanced" && settings && (
                  <div
                    className="flex-shrink-0 pt-4 border-t border-border mt-4 flex items-center justify-end gap-3"
                    style={{ backgroundColor: "hsl(var(--background))" }}
                  >
                    <Button onClick={handleSave} disabled={isSaving}>
                      {isSaving ? (
                        <span className="inline-flex items-center gap-2">
                          <Loader2 className="h-4 w-4 animate-spin" />
                          {t("settings.saving")}
                        </span>
                      ) : (
                        <>
                          <Save className="mr-2 h-4 w-4" />
                          {t("common.save")}
                        </>
                      )}
                    </Button>
                  </div>
                )}
              </div>
            </div>
          </Tabs>
        )}
      </div>

      <Dialog
        open={showRestartPrompt}
        onOpenChange={(open) => !open && handleRestartLater()}
      >
        <DialogContent zIndex="alert" className="max-w-md glass border-border">
          <DialogHeader>
            <DialogTitle>{t("settings.restartRequired")}</DialogTitle>
          </DialogHeader>
          <div className="px-6">
            <p className="text-sm text-muted-foreground">
              {t("settings.restartRequiredMessage")}
            </p>
          </div>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={handleRestartLater}
              className="hover:bg-muted/50"
            >
              {t("settings.restartLater")}
            </Button>
            <Button
              onClick={handleRestartNow}
              className="bg-primary hover:bg-primary/90"
            >
              {t("settings.restartNow")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

interface SidebarTabTriggerProps {
  value: string;
  icon: React.ComponentType<{ className?: string }>;
  children: React.ReactNode;
}

function SidebarTabTrigger({
  value,
  icon: Icon,
  children,
}: SidebarTabTriggerProps) {
  return (
    <TabsTrigger
      value={value}
      className="flex w-full items-center justify-start gap-2.5 rounded-md border-none px-3 py-2 text-left text-[13px] font-medium text-muted-foreground shadow-none transition-colors hover:bg-muted/70 hover:text-foreground data-[state=active]:bg-primary/10 data-[state=active]:text-primary data-[state=active]:shadow-none"
    >
      <Icon className="h-4 w-4 flex-shrink-0 opacity-80" />
      <span>{children}</span>
    </TabsTrigger>
  );
}
