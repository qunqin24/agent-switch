import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSessionSearch } from "@/hooks/useSessionSearch";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { toast } from "sonner";
import { useQueryClient } from "@tanstack/react-query";
import {
  RefreshCw,
  Search,
  Play,
  Trash2,
  MessageSquare,
  FolderOpen,
  CheckSquare,
} from "lucide-react";
import {
  useDeleteSessionMutation,
  useSessionMessagesQuery,
  useSessionsQuery,
} from "@/lib/query";
import { sessionsApi } from "@/lib/api";
import type { SessionMeta } from "@/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "@/components/ui/select";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { extractErrorMessage } from "@/utils/errorUtils";
import { isTextEditableTarget } from "@/utils/domUtils";
import { isMac } from "@/lib/platform";
import { ProviderIcon } from "@/components/ProviderIcon";
import { SessionItem } from "./SessionItem";
import { SessionMessageItem } from "./SessionMessageItem";
import { SessionTocDialog, SessionTocSidebar } from "./SessionToc";
import {
  extractCodexPromptPreview,
  formatRelativeTime,
  formatSessionMessagePreview,
  formatSessionTitle,
  getBaseName,
  getProviderIconName,
  getProviderLabel,
  getSessionKey,
  shouldHideCodexMessageFromToc,
} from "./utils";

type ProviderFilter =
  | "all"
  | "codex"
  | "claude"
  | "opencode"
  | "openclaw"
  | "gemini"
  | "hermes"
  | "pi";

export function SessionManagerPage({ appId }: { appId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data, isLoading, refetch } = useSessionsQuery();
  const sessions = data ?? [];
  const detailRef = useRef<HTMLDivElement | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);
  const [activeMessageIndex, setActiveMessageIndex] = useState<number | null>(
    null,
  );
  const [tocDialogOpen, setTocDialogOpen] = useState(false);
  const [deleteTargets, setDeleteTargets] = useState<SessionMeta[] | null>(
    null,
  );
  const [selectedSessionKeys, setSelectedSessionKeys] = useState<Set<string>>(
    () => new Set(),
  );
  const [isBatchDeleting, setIsBatchDeleting] = useState(false);
  const [selectionMode, setSelectionMode] = useState(false);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const [search, setSearch] = useState("");
  const [providerFilter, setProviderFilter] = useState<ProviderFilter>(
    appId as ProviderFilter,
  );
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  // 使用 FlexSearch 全文搜索
  const { search: searchSessions } = useSessionSearch({
    sessions,
    providerFilter,
  });

  const filteredSessions = useMemo(() => {
    return searchSessions(search);
  }, [searchSessions, search]);

  useEffect(() => {
    if (filteredSessions.length === 0) {
      setSelectedKey(null);
      return;
    }
    const exists = selectedKey
      ? filteredSessions.some(
          (session) => getSessionKey(session) === selectedKey,
        )
      : false;
    if (!exists) {
      setSelectedKey(getSessionKey(filteredSessions[0]));
    }
  }, [filteredSessions, selectedKey]);

  const selectedSession = useMemo(() => {
    if (!selectedKey) return null;
    return (
      filteredSessions.find(
        (session) => getSessionKey(session) === selectedKey,
      ) || null
    );
  }, [filteredSessions, selectedKey]);

  const { data: messages = [], isLoading: isLoadingMessages } =
    useSessionMessagesQuery(
      selectedSession?.providerId,
      selectedSession?.sourcePath,
    );
  const deleteSessionMutation = useDeleteSessionMutation();
  const isDeleting = deleteSessionMutation.isPending || isBatchDeleting;

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollContainerRef.current,
    estimateSize: () => 90,
    overscan: 5,
    gap: 12,
  });

  useEffect(() => {
    if (scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = 0;
    }
  }, [selectedKey]);

  useEffect(() => {
    const validKeys = new Set(
      sessions.map((session) => getSessionKey(session)),
    );
    setSelectedSessionKeys((current) => {
      let changed = false;
      const next = new Set<string>();
      current.forEach((key) => {
        if (validKeys.has(key)) {
          next.add(key);
        } else {
          changed = true;
        }
      });
      return changed ? next : current;
    });
  }, [sessions]);

  // Ctrl/Cmd+F 聚焦常驻搜索框
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const key = event.key.toLowerCase();
      if ((event.metaKey || event.ctrlKey) && key === "f") {
        if (isTextEditableTarget(document.activeElement)) return;
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      }
    };
    globalThis.addEventListener("keydown", handleKeyDown);
    return () => globalThis.removeEventListener("keydown", handleKeyDown);
  }, []);

  const isCodexSession = selectedSession?.providerId === "codex";

  // 提取用户消息用于目录
  const userMessagesToc = useMemo(() => {
    return messages
      .map((msg, index) => ({ msg, index }))
      .filter(({ msg }) => {
        if (msg.role.toLowerCase() !== "user") return false;
        return !(isCodexSession && shouldHideCodexMessageFromToc(msg.content));
      })
      .map(({ msg, index }) => {
        const previewContent = isCodexSession
          ? extractCodexPromptPreview(msg.content)
          : msg.content;

        return {
          index,
          preview: formatSessionMessagePreview(previewContent),
          ts: msg.ts,
        };
      });
  }, [isCodexSession, messages]);

  const scrollToMessage = (index: number) => {
    virtualizer.scrollToIndex(index, { align: "center", behavior: "smooth" });
    setActiveMessageIndex(index);
    setTocDialogOpen(false);
    setTimeout(() => setActiveMessageIndex(null), 2000);
  };

  const handleCopy = useCallback(
    async (text: string, successMessage: string) => {
      try {
        await navigator.clipboard.writeText(text);
        toast.success(successMessage);
      } catch (error) {
        toast.error(
          extractErrorMessage(error) ||
            t("common.error", { defaultValue: "Copy failed" }),
        );
      }
    },
    [t],
  );

  const handleMessageCopy = useCallback(
    (content: string) => {
      void handleCopy(
        content,
        t("sessionManager.messageCopied", { defaultValue: "已复制消息内容" }),
      );
    },
    [handleCopy, t],
  );

  const handleResume = async () => {
    if (!selectedSession?.resumeCommand) return;

    if (!isMac()) {
      await handleCopy(
        selectedSession.resumeCommand,
        t("sessionManager.resumeCommandCopied"),
      );
      return;
    }

    try {
      await sessionsApi.launchTerminal({
        command: selectedSession.resumeCommand,
        cwd: selectedSession.projectDir ?? undefined,
      });
      toast.success(t("sessionManager.terminalLaunched"));
    } catch (error) {
      const fallback = selectedSession.resumeCommand;
      await handleCopy(fallback, t("sessionManager.resumeFallbackCopied"));
      toast.error(extractErrorMessage(error) || t("sessionManager.openFailed"));
    }
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTargets || deleteTargets.length === 0 || isDeleting) {
      return;
    }

    const targets = deleteTargets.filter((session) => session.sourcePath);
    setDeleteTargets(null);

    if (targets.length === 0) {
      return;
    }

    if (targets.length === 1) {
      const [target] = targets;
      await deleteSessionMutation.mutateAsync({
        providerId: target.providerId,
        sessionId: target.sessionId,
        sourcePath: target.sourcePath!,
      });
      setSelectedSessionKeys((current) => {
        const next = new Set(current);
        next.delete(getSessionKey(target));
        return next;
      });
      return;
    }

    setIsBatchDeleting(true);
    try {
      const results = await sessionsApi.deleteMany(
        targets.map((session) => ({
          providerId: session.providerId,
          sessionId: session.sessionId,
          sourcePath: session.sourcePath!,
        })),
      );

      const deletedKeys = results
        .filter((result) => result.success)
        .map(
          (result) =>
            `${result.providerId}:${result.sessionId}:${result.sourcePath ?? ""}`,
        );

      const failedErrors = results
        .filter((result) => !result.success)
        .map((result) => result.error || t("common.unknown"));

      if (deletedKeys.length > 0) {
        const deletedKeySet = new Set(deletedKeys);
        queryClient.setQueryData<SessionMeta[]>(["sessions"], (current) =>
          (current ?? []).filter(
            (session) => !deletedKeySet.has(getSessionKey(session)),
          ),
        );
      }

      results
        .filter((result) => result.success)
        .forEach((result) => {
          queryClient.removeQueries({
            queryKey: ["sessionMessages", result.providerId, result.sourcePath],
          });
        });

      setSelectedSessionKeys((current) => {
        const next = new Set(current);
        deletedKeys.forEach((key) => next.delete(key));
        return next;
      });

      await queryClient.invalidateQueries({ queryKey: ["sessions"] });

      if (deletedKeys.length > 0) {
        toast.success(
          t("sessionManager.batchDeleteSuccess", {
            defaultValue: "已删除 {{count}} 个会话",
            count: deletedKeys.length,
          }),
        );
      }

      if (failedErrors.length > 0) {
        toast.error(
          t("sessionManager.batchDeleteFailed", {
            defaultValue: "{{failed}} 个会话删除失败",
            failed: failedErrors.length,
          }),
          {
            description: failedErrors[0],
          },
        );
      }
    } catch (error) {
      toast.error(
        extractErrorMessage(error) ||
          t("sessionManager.batchDeleteRequestFailed", {
            defaultValue: "批量删除失败，请稍后重试",
          }),
      );
    } finally {
      setIsBatchDeleting(false);
    }
  };

  const deletableFilteredSessions = useMemo(
    () => filteredSessions.filter((session) => Boolean(session.sourcePath)),
    [filteredSessions],
  );

  const selectedSessions = useMemo(
    () =>
      sessions.filter((session) =>
        selectedSessionKeys.has(getSessionKey(session)),
      ),
    [sessions, selectedSessionKeys],
  );

  const selectedDeletableSessions = useMemo(
    () => selectedSessions.filter((session) => Boolean(session.sourcePath)),
    [selectedSessions],
  );

  useEffect(() => {
    if (!selectionMode) return;

    const visibleKeys = new Set(
      deletableFilteredSessions.map((session) => getSessionKey(session)),
    );

    setSelectedSessionKeys((current) => {
      let changed = false;
      const next = new Set<string>();

      current.forEach((key) => {
        if (visibleKeys.has(key)) {
          next.add(key);
        } else {
          changed = true;
        }
      });

      return changed ? next : current;
    });
  }, [deletableFilteredSessions, selectionMode]);

  const allFilteredSelected =
    deletableFilteredSessions.length > 0 &&
    deletableFilteredSessions.every((session) =>
      selectedSessionKeys.has(getSessionKey(session)),
    );

  const toggleSessionChecked = (session: SessionMeta, checked: boolean) => {
    if (!session.sourcePath) return;
    const key = getSessionKey(session);
    setSelectedSessionKeys((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(key);
      } else {
        next.delete(key);
      }
      return next;
    });
  };

  const handleToggleSelectAll = () => {
    setSelectedSessionKeys((current) => {
      const next = new Set(current);
      if (allFilteredSelected) {
        deletableFilteredSessions.forEach((session) =>
          next.delete(getSessionKey(session)),
        );
      } else {
        deletableFilteredSessions.forEach((session) =>
          next.add(getSessionKey(session)),
        );
      }
      return next;
    });
  };

  const openBatchDeleteDialog = () => {
    if (selectedDeletableSessions.length === 0) return;
    setDeleteTargets(selectedDeletableSessions);
  };

  const exitSelectionMode = () => {
    setSelectionMode(false);
    setSelectedSessionKeys(new Set());
  };

  const iconBtn =
    "size-9 rounded-lg text-zinc-500 hover:text-zinc-900 hover:bg-zinc-900/5 dark:text-zinc-400 dark:hover:text-zinc-100 dark:hover:bg-white/5";

  return (
    <TooltipProvider>
      <div
        className="flex flex-1 min-h-0 overflow-hidden"
        onWheel={(e) => e.stopPropagation()}
      >
        {/* 左栏：会话列表 */}
        <div className="w-[300px] shrink-0 flex flex-col min-h-0 border-r border-zinc-100 dark:border-zinc-800 bg-white dark:bg-zinc-950">
          {/* 左栏头部 */}
          <div className="p-3 border-b border-zinc-100 dark:border-zinc-900 space-y-2">
            <div className="flex items-center gap-1.5">
              <div className="relative flex-1">
                <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground pointer-events-none" />
                <Input
                  ref={searchInputRef}
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder={t("sessionManager.searchPlaceholder")}
                  className="h-9 pl-8 pr-3 text-sm"
                />
              </div>

              <Select
                value={providerFilter}
                onValueChange={(value) =>
                  setProviderFilter(value as ProviderFilter)
                }
              >
                <Tooltip>
                  <TooltipTrigger asChild>
                    <SelectTrigger
                      className={`size-9 p-0 justify-center border-0 bg-transparent ${iconBtn}`}
                    >
                      <ProviderIcon
                        icon={
                          providerFilter === "all"
                            ? "apps"
                            : getProviderIconName(providerFilter)
                        }
                        name={providerFilter}
                        size={16}
                      />
                    </SelectTrigger>
                  </TooltipTrigger>
                  <TooltipContent>
                    {providerFilter === "all"
                      ? t("sessionManager.providerFilterAll")
                      : getProviderLabel(providerFilter, t)}
                  </TooltipContent>
                </Tooltip>
                <SelectContent>
                  <SelectItem value="all">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="apps" name="all" size={14} />
                      <span>{t("sessionManager.providerFilterAll")}</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="codex">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="openai" name="codex" size={14} />
                      <span>Codex</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="claude">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="claude" name="claude" size={14} />
                      <span>Claude Code</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="opencode">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="opencode" name="opencode" size={14} />
                      <span>OpenCode</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="openclaw">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="openclaw" name="openclaw" size={14} />
                      <span>OpenClaw</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="gemini">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="gemini" name="gemini" size={14} />
                      <span>Gemini CLI</span>
                    </div>
                  </SelectItem>
                  <SelectItem value="pi">
                    <div className="flex items-center gap-2">
                      <ProviderIcon icon="pi" name="pi" size={14} />
                      <span>{t("apps.pi")}</span>
                    </div>
                  </SelectItem>
                </SelectContent>
              </Select>

              {(selectionMode || deletableFilteredSessions.length > 0) && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant={selectionMode ? "secondary" : "ghost"}
                      size="icon"
                      className={
                        selectionMode
                          ? "size-9 rounded-lg bg-blue-500/10 text-blue-600 hover:bg-blue-500/15 dark:bg-blue-500/15 dark:text-blue-300 dark:hover:bg-blue-500/20"
                          : iconBtn
                      }
                      aria-label={
                        selectionMode
                          ? t("sessionManager.exitBatchModeTooltip", {
                              defaultValue: "退出批量管理",
                            })
                          : t("sessionManager.manageBatchTooltip", {
                              defaultValue: "批量管理",
                            })
                      }
                      onClick={() => {
                        if (selectionMode) {
                          exitSelectionMode();
                        } else {
                          setSelectionMode(true);
                        }
                      }}
                    >
                      <CheckSquare className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {selectionMode
                      ? t("sessionManager.exitBatchModeTooltip", {
                          defaultValue: "退出批量管理",
                        })
                      : t("sessionManager.manageBatchTooltip", {
                          defaultValue: "批量管理",
                        })}
                  </TooltipContent>
                </Tooltip>
              )}

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className={iconBtn}
                    onClick={() => void refetch()}
                  >
                    <RefreshCw className="size-4" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("common.refresh")}</TooltipContent>
              </Tooltip>
            </div>

            {/* 批量选择条 */}
            {selectionMode && (
              <div className="flex items-center gap-2 rounded-lg border border-zinc-100 dark:border-zinc-800 bg-zinc-50/60 dark:bg-zinc-900/30 px-2.5 py-1.5">
                <span className="text-[11px] font-medium text-zinc-600 dark:text-zinc-300 shrink-0">
                  {t("sessionManager.selectedCount", {
                    defaultValue: "已选 {{count}} 项",
                    count: selectedDeletableSessions.length,
                  })}
                </span>
                <div className="ml-auto flex items-center gap-1">
                  {deletableFilteredSessions.length > 0 && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs"
                      onClick={handleToggleSelectAll}
                    >
                      {allFilteredSelected
                        ? t("sessionManager.clearFilteredSelection", {
                            defaultValue: "取消全选",
                          })
                        : t("sessionManager.selectAllFiltered", {
                            defaultValue: "全选当前",
                          })}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => setSelectedSessionKeys(new Set())}
                  >
                    {t("sessionManager.clearSelection", {
                      defaultValue: "清空已选",
                    })}
                  </Button>
                  <Button
                    variant="destructive"
                    size="sm"
                    className="h-7 gap-1 px-2 text-xs"
                    onClick={openBatchDeleteDialog}
                    disabled={
                      isDeleting || selectedDeletableSessions.length === 0
                    }
                  >
                    <Trash2 className="size-3.5" />
                    {isBatchDeleting
                      ? t("sessionManager.batchDeleting", {
                          defaultValue: "删除中...",
                        })
                      : t("sessionManager.deleteSelected", {
                          defaultValue: "批量删除",
                        })}
                  </Button>
                </div>
              </div>
            )}
          </div>

          {/* 列表 */}
          <div className="flex-1 min-h-0 overflow-y-auto overscroll-contain">
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="size-5 animate-spin text-muted-foreground" />
              </div>
            ) : filteredSessions.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-16 text-center">
                <MessageSquare className="size-8 text-muted-foreground/40 mb-2" />
                <p className="text-sm text-muted-foreground">
                  {t("sessionManager.noSessions")}
                </p>
              </div>
            ) : (
              <div className="divide-y divide-zinc-100 dark:divide-zinc-900">
                {filteredSessions.map((session) => {
                  const isSelected =
                    selectedKey !== null &&
                    getSessionKey(session) === selectedKey;
                  return (
                    <SessionItem
                      key={getSessionKey(session)}
                      session={session}
                      isSelected={isSelected}
                      selectionMode={selectionMode}
                      searchQuery={search}
                      isChecked={selectedSessionKeys.has(
                        getSessionKey(session),
                      )}
                      isCheckDisabled={!session.sourcePath}
                      onSelect={setSelectedKey}
                      onToggleChecked={(checked) =>
                        toggleSessionChecked(session, checked)
                      }
                    />
                  );
                })}
              </div>
            )}
          </div>
        </div>

        {/* 右栏：详情 / 对话流 */}
        <div
          ref={detailRef}
          className="flex-1 flex flex-col min-h-0 bg-white dark:bg-zinc-950"
        >
          {!selectedSession ? (
            <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground p-8">
              <MessageSquare className="size-12 mb-3 opacity-25" />
              <p className="text-sm">{t("sessionManager.selectSession")}</p>
            </div>
          ) : (
            <>
              {/* 详情头部 */}
              <div className="px-5 py-3 border-b border-zinc-100 dark:border-zinc-900 shrink-0">
                <div className="flex items-center gap-2 min-w-0">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="shrink-0">
                        <ProviderIcon
                          icon={getProviderIconName(selectedSession.providerId)}
                          name={selectedSession.providerId}
                          size={18}
                        />
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      {getProviderLabel(selectedSession.providerId, t)}
                    </TooltipContent>
                  </Tooltip>
                  <h2 className="text-base font-semibold truncate flex-1 min-w-0">
                    {formatSessionTitle(selectedSession)}
                  </h2>
                  {isMac() && (
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-8 rounded-lg text-zinc-500 hover:text-zinc-900 hover:bg-zinc-900/5 dark:text-zinc-400 dark:hover:text-zinc-100 dark:hover:bg-white/5"
                          aria-label={t("sessionManager.resume", {
                            defaultValue: "恢复会话",
                          })}
                          onClick={() => void handleResume()}
                          disabled={!selectedSession.resumeCommand}
                        >
                          <Play className="size-4" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom">
                        {selectedSession.resumeCommand ? (
                          <p className="font-mono text-xs break-all max-w-xs">
                            {selectedSession.resumeCommand}
                          </p>
                        ) : (
                          t("sessionManager.noResumeCommand", {
                            defaultValue: "此会话无法恢复",
                          })
                        )}
                      </TooltipContent>
                    </Tooltip>
                  )}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-8 rounded-lg text-zinc-500 hover:text-red-600 hover:bg-red-500/10 dark:text-zinc-400 dark:hover:text-red-400 dark:hover:bg-red-500/15"
                        aria-label={
                          isDeleting
                            ? t("sessionManager.deleting", {
                                defaultValue: "删除中...",
                              })
                            : t("sessionManager.delete", {
                                defaultValue: "删除会话",
                              })
                        }
                        onClick={() => setDeleteTargets([selectedSession])}
                        disabled={!selectedSession.sourcePath || isDeleting}
                      >
                        <Trash2 className="size-4" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="bottom">
                      {t("sessionManager.deleteTooltip", {
                        defaultValue: "永久删除此本地会话记录",
                      })}
                    </TooltipContent>
                  </Tooltip>
                </div>

                {/* 元信息行 */}
                <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground min-w-0">
                  {selectedSession.projectDir && (
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={() =>
                            void handleCopy(
                              selectedSession.projectDir!,
                              t("sessionManager.projectDirCopied"),
                            )
                          }
                          className="flex items-center gap-1 hover:text-foreground transition-colors min-w-0"
                        >
                          <FolderOpen className="size-3 shrink-0" />
                          <span className="truncate max-w-[260px]">
                            {getBaseName(selectedSession.projectDir)}
                          </span>
                        </button>
                      </TooltipTrigger>
                      <TooltipContent side="bottom" className="max-w-xs">
                        <p className="font-mono text-xs break-all">
                          {selectedSession.projectDir}
                        </p>
                        <p className="text-muted-foreground mt-1">
                          {t("sessionManager.clickToCopyPath")}
                        </p>
                      </TooltipContent>
                    </Tooltip>
                  )}
                  {(selectedSession.lastActiveAt ||
                    selectedSession.createdAt) && (
                    <span className="shrink-0">
                      {formatRelativeTime(
                        selectedSession.lastActiveAt ??
                          selectedSession.createdAt,
                        t,
                      )}
                    </span>
                  )}
                </div>
              </div>

              {/* 对话流 */}
              <div className="flex-1 min-h-0 flex">
                <div
                  ref={scrollContainerRef}
                  className="flex-1 min-h-0 overflow-y-auto overscroll-contain min-w-0"
                >
                  <div className="mx-auto max-w-4xl px-5 py-4">
                    {isLoadingMessages ? (
                      <div className="flex items-center justify-center py-12">
                        <RefreshCw className="size-5 animate-spin text-muted-foreground" />
                      </div>
                    ) : messages.length === 0 ? (
                      <div className="flex flex-col items-center justify-center py-16 text-center">
                        <MessageSquare className="size-8 text-muted-foreground/40 mb-2" />
                        <p className="text-sm text-muted-foreground">
                          {t("sessionManager.emptySession")}
                        </p>
                      </div>
                    ) : (
                      <div
                        style={{
                          height: virtualizer.getTotalSize(),
                          position: "relative",
                        }}
                      >
                        {virtualizer.getVirtualItems().map((virtualRow) => (
                          <div
                            key={virtualRow.key}
                            data-index={virtualRow.index}
                            ref={virtualizer.measureElement}
                            style={{
                              position: "absolute",
                              top: 0,
                              left: 0,
                              width: "100%",
                              transform: `translateY(${virtualRow.start}px)`,
                            }}
                          >
                            <SessionMessageItem
                              message={messages[virtualRow.index]}
                              isActive={activeMessageIndex === virtualRow.index}
                              searchQuery={search}
                              onCopy={handleMessageCopy}
                            />
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* 右侧目录 (大屏) */}
                <SessionTocSidebar
                  items={userMessagesToc}
                  onItemClick={scrollToMessage}
                />
              </div>

              {/* 浮动目录按钮 (小屏) */}
              <SessionTocDialog
                items={userMessagesToc}
                onItemClick={scrollToMessage}
                open={tocDialogOpen}
                onOpenChange={setTocDialogOpen}
              />
            </>
          )}
        </div>
      </div>
      <ConfirmDialog
        isOpen={Boolean(deleteTargets)}
        title={
          deleteTargets && deleteTargets.length > 1
            ? t("sessionManager.batchDeleteConfirmTitle", {
                defaultValue: "批量删除会话",
              })
            : t("sessionManager.deleteConfirmTitle", {
                defaultValue: "删除会话",
              })
        }
        message={
          deleteTargets && deleteTargets.length > 1
            ? t("sessionManager.batchDeleteConfirmMessage", {
                defaultValue:
                  "将永久删除已选中的 {{count}} 个本地会话记录。\n\n此操作不可恢复。",
                count: deleteTargets.length,
              })
            : deleteTargets?.[0]
              ? t("sessionManager.deleteConfirmMessage", {
                  defaultValue:
                    "将永久删除本地会话“{{title}}”\nSession ID: {{sessionId}}\n\n此操作不可恢复。",
                  title: formatSessionTitle(deleteTargets[0]),
                  sessionId: deleteTargets[0].sessionId,
                })
              : ""
        }
        confirmText={
          deleteTargets && deleteTargets.length > 1
            ? t("sessionManager.batchDeleteConfirmAction", {
                defaultValue: "删除所选会话",
              })
            : t("sessionManager.deleteConfirmAction", {
                defaultValue: "删除会话",
              })
        }
        cancelText={t("common.cancel", { defaultValue: "取消" })}
        variant="destructive"
        onConfirm={() => void handleDeleteConfirm()}
        onCancel={() => {
          if (!isDeleting) {
            setDeleteTargets(null);
          }
        }}
      />
    </TooltipProvider>
  );
}
