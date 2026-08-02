import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  Box,
  Clipboard,
  Github,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { toast } from "sonner";

import { SkillBreadcrumbs } from "./SkillBreadcrumbs";
import { Button } from "@/components/ui/button";
import { useSkillsShRepository } from "@/hooks/useSkills";
import { copyText } from "@/lib/clipboard";
import type {
  SkillsShDiscoverableSkill,
  SkillsShRepositorySkill,
} from "@/lib/api/skills";
import { cn } from "@/lib/utils";

type InstallFormat = "command" | "prompt";

interface SkillRepositoryPageProps {
  owner: string;
  repository: string;
  onHome: () => void;
  onPublisher: () => void;
  onSelectSkill: (skill: SkillsShDiscoverableSkill) => void;
}

function toDiscoverableSkill({
  owner,
  repository,
  skill,
}: {
  owner: string;
  repository: string;
  skill: SkillsShRepositorySkill;
}): SkillsShDiscoverableSkill {
  const detailUrl = `https://skills.sh/${owner}/${repository}/${skill.skillId}`;
  return {
    key: `${owner}/${repository}/${skill.skillId}`,
    name: skill.name,
    directory: skill.skillId,
    repoOwner: owner,
    repoName: repository,
    repoBranch: "main",
    installs: skill.installs,
    weeklyInstalls: [],
    isOfficial: false,
    readmeUrl: detailUrl,
    detailUrl,
  };
}

export function SkillRepositoryPage({
  owner,
  repository,
  onHome,
  onPublisher,
  onSelectSkill,
}: SkillRepositoryPageProps) {
  const { t } = useTranslation();
  const [installFormat, setInstallFormat] = useState<InstallFormat>("command");
  const { data, isLoading, isError, isFetching, refetch } =
    useSkillsShRepository(owner, repository);
  const command = `npx skills add ${owner}/${repository}`;
  const prompt = `Use the skills in "https://github.com/${owner}/${repository}" that are relevant to the current task. Run \`npx skills add "https://github.com/${owner}/${repository}"\` and select the relevant skills, then follow their instructions.`;
  const displayedInstallText = installFormat === "command" ? command : prompt;

  const handleCopy = async () => {
    try {
      await copyText(displayedInstallText);
      toast.success(t("skills.skillssh.detail.copied"));
    } catch (error) {
      console.error("Failed to copy repository install text:", error);
    }
  };

  return (
    <div className="mx-auto w-full max-w-6xl py-5">
      <SkillBreadcrumbs
        items={[
          { key: "skills", label: "skills", onClick: onHome },
          { key: owner, label: owner, onClick: onPublisher },
          { key: repository, label: repository },
        ]}
      />

      <h2 className="text-3xl font-semibold tracking-tight text-foreground sm:text-4xl">
        {owner}/{repository}
      </h2>

      {isLoading ? (
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <Loader2 className="mr-3 h-6 w-6 animate-spin" />
          {t("skills.skillssh.repository.loading")}
        </div>
      ) : isError || !data ? (
        <div className="flex h-64 flex-col items-center justify-center text-center">
          <AlertTriangle className="mb-3 h-9 w-9 text-destructive/70" />
          <p className="text-sm font-medium">
            {t("skills.skillssh.repository.error")}
          </p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="mt-4"
            onClick={() => void refetch()}
            disabled={isFetching}
          >
            <RefreshCw
              className={
                isFetching ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"
              }
            />
            {t("common.refresh")}
          </Button>
        </div>
      ) : (
        <>
          <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 text-sm text-foreground">
            <span className="flex items-center gap-1.5 whitespace-nowrap">
              <Box className="h-4 w-4" />
              {t("skills.skillssh.repository.skills", {
                count: data.skillCount,
              })}
            </span>
            <span className="flex items-center gap-1.5 whitespace-nowrap">
              <Activity className="h-4 w-4" />
              {t("skills.skillssh.publisher.totalInstalls", {
                total: data.totalInstalls,
              })}
            </span>
            <span className="flex items-center gap-1.5 whitespace-nowrap">
              <Github className="h-4 w-4" />
              GitHub
            </span>
          </div>

          <section className="mt-10">
            <div className="flex items-end gap-4">
              <button
                type="button"
                onClick={() => void handleCopy()}
                className="flex min-w-0 flex-1 items-center gap-3 rounded-lg border border-border bg-muted/65 px-4 py-3 text-left outline-none transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-blue-500/40"
                title={t("skills.skillssh.repository.copyInstall")}
              >
                <code className="min-w-0 flex-1 truncate font-mono text-sm text-muted-foreground">
                  {installFormat === "command" && (
                    <span className="mr-2 text-muted-foreground/50">$</span>
                  )}
                  {displayedInstallText}
                </code>
                <Clipboard className="h-4 w-4 shrink-0 text-muted-foreground" />
              </button>

              <div
                role="tablist"
                aria-label={t("skills.skillssh.repository.installFormat")}
                className="hidden shrink-0 items-center gap-1 sm:flex"
              >
                {(["command", "prompt"] satisfies InstallFormat[]).map(
                  (format) => (
                    <button
                      key={format}
                      type="button"
                      role="tab"
                      aria-selected={installFormat === format}
                      onClick={() => setInstallFormat(format)}
                      className={cn(
                        "rounded-md px-3 py-2 font-mono text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-blue-500/40",
                        installFormat === format && "bg-muted text-foreground",
                      )}
                    >
                      {t(`skills.skillssh.repository.${format}`)}
                    </button>
                  ),
                )}
              </div>
            </div>
          </section>

          <section className="mt-16">
            <div className="grid grid-cols-[minmax(0,1fr)_9rem] border-b border-border py-3 font-mono text-xs uppercase text-muted-foreground">
              <span>{t("skills.skillssh.columns.skill")}</span>
              <span className="text-right">
                {t("skills.skillssh.columns.installs")}
              </span>
            </div>
            <div className="divide-y divide-border">
              {data.skills.map((skill) => (
                <button
                  key={skill.skillId}
                  type="button"
                  onClick={() =>
                    onSelectSkill(
                      toDiscoverableSkill({ owner, repository, skill }),
                    )
                  }
                  className="group grid w-full grid-cols-[minmax(0,1fr)_9rem] items-start gap-4 py-3 text-left outline-none transition-colors hover:bg-muted/25 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blue-500/40"
                >
                  <span className="truncate font-semibold text-foreground group-hover:text-blue-600 dark:group-hover:text-blue-400">
                    {skill.name}
                  </span>
                  <span className="text-right font-mono text-sm text-foreground">
                    {skill.installsLabel}
                  </span>
                </button>
              ))}
            </div>
          </section>
        </>
      )}
    </div>
  );
}
