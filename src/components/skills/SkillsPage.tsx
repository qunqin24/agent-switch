import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Loader2, RefreshCw, Search } from "lucide-react";
import { toast } from "sonner";

import { SkillLeaderboardTable } from "./SkillLeaderboardTable";
import { SkillDetailPage } from "./SkillDetailPage";
import { SkillPublisherPage } from "./SkillPublisherPage";
import { SkillRepositoryPage } from "./SkillRepositoryPage";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  useAppSkills,
  useGlobalSkills,
  useInstallGlobalSkill,
  useInstallSkill,
  useSearchSkillsSh,
  useSkillsShLeaderboard,
} from "@/hooks/useSkills";
import type {
  DiscoverableSkill,
  SkillsShDiscoverableSkill,
  SkillsShLeaderboardView,
} from "@/lib/api/skills";
import type { SkillAppId } from "@/lib/api/types";
import { formatSkillError } from "@/lib/errors/skillErrorParser";
import { cn } from "@/lib/utils";

interface SkillsPageProps {
  initialApp?: SkillAppId;
  installTarget?: "app" | "global";
}

export interface SkillsPageHandle {
  refresh: () => void;
}

const SKILLSSH_RESULT_LIMIT = 100;
const LEADERBOARD_TABS: SkillsShLeaderboardView[] = [
  "all-time",
  "trending",
  "hot",
];

type SkillsBrowserRoute =
  | { kind: "leaderboard" }
  | { kind: "publisher"; owner: string }
  | { kind: "repository"; owner: string; repository: string }
  | { kind: "skill"; skill: SkillsShDiscoverableSkill };

/**
 * Skills 发现面板。
 *
 * skills.sh 是唯一的远程目录；应用入口决定安装到当前 CLI 还是全局目录。
 */
export const SkillsPage = forwardRef<SkillsPageHandle, SkillsPageProps>(
  ({ initialApp = "claude", installTarget = "app" }, ref) => {
    const { t, i18n } = useTranslation();
    const [activeView, setActiveView] =
      useState<SkillsShLeaderboardView>("all-time");
    const [allTimeTotal, setAllTimeTotal] = useState<number>();
    const [searchInput, setSearchInput] = useState("");
    const [searchQuery, setSearchQuery] = useState("");
    const [browserRoute, setBrowserRoute] = useState<SkillsBrowserRoute>({
      kind: "leaderboard",
    });
    const currentApp = initialApp;
    const isSearchMode = searchQuery.length >= 2;

    const { data: appSkills, refetch: refetchAppSkills } = useAppSkills(
      currentApp,
      installTarget === "app",
    );
    const { data: globalSkills, refetch: refetchGlobalSkills } =
      useGlobalSkills(installTarget === "global");
    const {
      data: leaderboardResult,
      isLoading: leaderboardLoading,
      isFetching: leaderboardFetching,
      isError: leaderboardError,
      refetch: refetchLeaderboard,
    } = useSkillsShLeaderboard(activeView, SKILLSSH_RESULT_LIMIT);
    const {
      data: searchResult,
      isLoading: searchLoading,
      isFetching: searchFetching,
      isError: searchError,
      refetch: refetchSearch,
    } = useSearchSkillsSh(searchQuery, SKILLSSH_RESULT_LIMIT);

    useEffect(() => {
      if (leaderboardResult) {
        setAllTimeTotal(leaderboardResult.allTimeTotal);
      }
    }, [leaderboardResult]);

    const installMutation = useInstallSkill();
    const installGlobalMutation = useInstallGlobalSkill();
    const installedSkills =
      installTarget === "global" ? globalSkills?.skills : appSkills?.skills;

    const installedDirectories = useMemo(
      () =>
        new Set(
          (installedSkills ?? []).map((skill) =>
            (
              skill.directory.split(/[/\\]/).pop() ?? skill.directory
            ).toLowerCase(),
          ),
        ),
      [installedSkills],
    );

    const displayedSkills = isSearchMode
      ? (searchResult?.skills ?? [])
      : (leaderboardResult?.skills ?? []);
    const contentLoading = isSearchMode
      ? searchLoading && searchResult === undefined
      : leaderboardLoading && leaderboardResult === undefined;
    const contentFetching = isSearchMode ? searchFetching : leaderboardFetching;
    const contentError = isSearchMode ? searchError : leaderboardError;

    useImperativeHandle(ref, () => ({
      refresh: () => {
        if (isSearchMode) {
          void refetchSearch();
        } else {
          void refetchLeaderboard();
        }
        if (installTarget === "global") {
          void refetchGlobalSkills();
        } else {
          void refetchAppSkills();
        }
      },
    }));

    const handleSearch = () => {
      const query = searchInput.trim();
      if (query.length < 2) return;

      if (query === searchQuery) {
        void refetchSearch();
        return;
      }
      setSearchQuery(query);
    };

    const handleTabChange = (view: SkillsShLeaderboardView) => {
      setActiveView(view);
      setSearchInput("");
      setSearchQuery("");
    };

    const toDiscoverableSkill = (
      skill: SkillsShDiscoverableSkill,
    ): DiscoverableSkill => ({
      key: skill.key,
      name: skill.name,
      description: "",
      directory: skill.directory,
      repoOwner: skill.repoOwner,
      repoName: skill.repoName,
      repoBranch: skill.repoBranch,
      readmeUrl: skill.detailUrl,
    });

    const handleInstall = async (key: string) => {
      const result =
        displayedSkills.find((skill) => skill.key === key) ??
        (browserRoute.kind === "skill" && browserRoute.skill.key === key
          ? browserRoute.skill
          : undefined);
      if (!result) {
        toast.error(t("skills.notFound"));
        return;
      }

      const skill = toDiscoverableSkill(result);
      try {
        if (installTarget === "global") {
          await installGlobalMutation.mutateAsync(skill);
        } else {
          await installMutation.mutateAsync({ skill, currentApp });
        }
        toast.success(t("skills.installSuccess", { name: skill.name }), {
          closeButton: true,
        });
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : String(error);
        const { title, description } = formatSkillError(
          errorMessage,
          t,
          "skills.installFailed",
        );
        toast.error(title, {
          description,
          duration: 10000,
        });
        console.error("Install skill failed:", error);
      }
    };

    const refetchContent = () => {
      if (isSearchMode) {
        void refetchSearch();
      } else {
        void refetchLeaderboard();
      }
    };

    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background px-6">
        <div className="flex-1 overflow-x-hidden overflow-y-auto animate-fade-in">
          {browserRoute.kind === "skill" ? (
            <SkillDetailPage
              skill={browserRoute.skill}
              installed={installedDirectories.has(
                browserRoute.skill.directory.toLowerCase(),
              )}
              onHome={() => setBrowserRoute({ kind: "leaderboard" })}
              onPublisher={() =>
                setBrowserRoute({
                  kind: "publisher",
                  owner: browserRoute.skill.repoOwner,
                })
              }
              onRepository={() =>
                setBrowserRoute({
                  kind: "repository",
                  owner: browserRoute.skill.repoOwner,
                  repository: browserRoute.skill.repoName,
                })
              }
              onInstall={handleInstall}
            />
          ) : browserRoute.kind === "publisher" ? (
            <SkillPublisherPage
              owner={browserRoute.owner}
              onHome={() => setBrowserRoute({ kind: "leaderboard" })}
              onSelectRepository={(repository) =>
                setBrowserRoute({
                  kind: "repository",
                  owner: browserRoute.owner,
                  repository,
                })
              }
            />
          ) : browserRoute.kind === "repository" ? (
            <SkillRepositoryPage
              owner={browserRoute.owner}
              repository={browserRoute.repository}
              onHome={() => setBrowserRoute({ kind: "leaderboard" })}
              onPublisher={() =>
                setBrowserRoute({
                  kind: "publisher",
                  owner: browserRoute.owner,
                })
              }
              onSelectSkill={(skill) =>
                setBrowserRoute({ kind: "skill", skill })
              }
            />
          ) : (
            <div className="py-3">
              <div className="flex flex-col-reverse gap-3 border-b border-border md:flex-row md:items-end md:justify-between">
                <div
                  role="tablist"
                  aria-label={t("skills.skillssh.leaderboard")}
                  className="flex min-w-0 items-center gap-6"
                >
                  {LEADERBOARD_TABS.map((view) => {
                    const selected = !isSearchMode && activeView === view;
                    const label =
                      view === "all-time"
                        ? t("skills.skillssh.tabs.allTime", {
                            total:
                              allTimeTotal === undefined
                                ? "…"
                                : allTimeTotal.toLocaleString(
                                    i18n.resolvedLanguage ||
                                      i18n.language ||
                                      "en",
                                  ),
                          })
                        : t(`skills.skillssh.tabs.${view}`);
                    return (
                      <button
                        key={view}
                        type="button"
                        role="tab"
                        aria-selected={selected}
                        onClick={() => handleTabChange(view)}
                        className={cn(
                          "relative h-12 whitespace-nowrap border-b-2 border-transparent px-0 font-mono text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40",
                          selected && "border-foreground text-foreground",
                        )}
                      >
                        {label}
                      </button>
                    );
                  })}
                </div>

                <div className="mb-2 flex min-w-0 items-center gap-2 md:w-80">
                  <div className="relative min-w-0 flex-1">
                    <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                    <Input
                      type="search"
                      placeholder={t("skills.skillssh.searchPlaceholder")}
                      value={searchInput}
                      onChange={(event) => setSearchInput(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") handleSearch();
                      }}
                      className="h-8 border-transparent bg-muted/45 pl-9 pr-3 shadow-none focus:border-border-default"
                    />
                  </div>

                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={handleSearch}
                    disabled={searchInput.trim().length < 2 || searchFetching}
                    className="h-8 w-8 shrink-0 rounded-md"
                    aria-label={t("skills.search")}
                  >
                    {searchFetching ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Search className="h-4 w-4" />
                    )}
                  </Button>
                </div>
              </div>

              {isSearchMode && (
                <div className="border-b border-border py-3 text-sm text-muted-foreground">
                  {t("skills.skillssh.searchResultsFor", {
                    query: searchQuery,
                  })}
                </div>
              )}

              {contentLoading ? (
                <div className="flex h-64 items-center justify-center">
                  <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                  <span className="ml-3 text-sm text-muted-foreground">
                    {isSearchMode
                      ? t("skills.skillssh.loading")
                      : t("skills.skillssh.loadingLeaderboard")}
                  </span>
                </div>
              ) : contentError ? (
                <div className="flex h-64 flex-col items-center justify-center text-center">
                  <AlertTriangle className="mb-4 h-10 w-10 text-destructive/70" />
                  <p className="text-base font-medium text-foreground">
                    {isSearchMode
                      ? t("skills.skillssh.error")
                      : t("skills.skillssh.leaderboardError")}
                  </p>
                  <Button
                    variant="outline"
                    size="sm"
                    className="mt-4"
                    onClick={refetchContent}
                    disabled={contentFetching}
                  >
                    <RefreshCw
                      className={cn(
                        "mr-1.5 h-3.5 w-3.5",
                        contentFetching && "animate-spin",
                      )}
                    />
                    {t("common.refresh")}
                  </Button>
                </div>
              ) : displayedSkills.length === 0 ? (
                <div className="flex h-48 flex-col items-center justify-center text-center">
                  <p className="text-lg font-medium text-foreground">
                    {isSearchMode
                      ? t("skills.skillssh.noResults", { query: searchQuery })
                      : t("skills.skillssh.noLeaderboardResults")}
                  </p>
                </div>
              ) : (
                <SkillLeaderboardTable
                  skills={displayedSkills}
                  installedDirectories={installedDirectories}
                  onInstall={handleInstall}
                  onSelect={(skill) =>
                    setBrowserRoute({ kind: "skill", skill })
                  }
                />
              )}
            </div>
          )}
        </div>
      </div>
    );
  },
);

SkillsPage.displayName = "SkillsPage";
