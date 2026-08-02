import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  BadgeCheck,
  Check,
  CheckCircle2,
  Clipboard,
  Download,
  Loader2,
  RefreshCw,
  Star,
} from "lucide-react";
import { toast } from "sonner";

import { SafeSkillHtml } from "./SafeSkillHtml";
import { SkillBreadcrumbs } from "./SkillBreadcrumbs";
import { SkillActivityChart } from "./SkillLeaderboardTable";
import { Button } from "@/components/ui/button";
import { useSkillsShDetail } from "@/hooks/useSkills";
import { copyText } from "@/lib/clipboard";
import type { SkillsShDiscoverableSkill } from "@/lib/api/skills";
import { cn } from "@/lib/utils";

interface SkillDetailPageProps {
  skill: SkillsShDiscoverableSkill;
  installed: boolean;
  onHome: () => void;
  onPublisher: () => void;
  onRepository: () => void;
  onInstall: (key: string) => Promise<void>;
}

function AuditStatus({ status }: { status: string }) {
  const normalized = status.toLowerCase();
  const passing = normalized === "pass";
  return (
    <span
      className={cn(
        "rounded px-2 py-1 font-mono text-[10px] uppercase",
        passing
          ? "bg-green-500/10 text-green-600 dark:text-green-400"
          : "bg-amber-500/10 text-amber-600 dark:text-amber-400",
      )}
    >
      {status}
    </span>
  );
}

export function SkillDetailPage({
  skill,
  installed,
  onHome,
  onPublisher,
  onRepository,
  onInstall,
}: SkillDetailPageProps) {
  const { t, i18n } = useTranslation();
  const [installing, setInstalling] = useState(false);
  const { data, isLoading, isError, isFetching, refetch } = useSkillsShDetail(
    skill.repoOwner,
    skill.repoName,
    skill.directory,
  );
  const installCommand = `npx skills add https://github.com/${skill.repoOwner}/${skill.repoName} --skill ${skill.directory}`;
  const numberFormatter = new Intl.NumberFormat(
    i18n.resolvedLanguage || i18n.language || "en",
    { notation: "compact", maximumFractionDigits: 1 },
  );

  const handleInstall = async () => {
    setInstalling(true);
    try {
      await onInstall(skill.key);
    } finally {
      setInstalling(false);
    }
  };

  const handleCopy = async () => {
    try {
      await copyText(installCommand);
      toast.success(t("skills.skillssh.detail.copied"));
    } catch (error) {
      console.error("Failed to copy Skill install command:", error);
    }
  };

  return (
    <div className="mx-auto w-full max-w-6xl py-5">
      <SkillBreadcrumbs
        items={[
          { key: "skills", label: "skills", onClick: onHome },
          {
            key: skill.repoOwner,
            label: skill.repoOwner,
            onClick: onPublisher,
          },
          {
            key: skill.repoName,
            label: skill.repoName,
            onClick: onRepository,
          },
          { key: skill.directory, label: skill.directory },
        ]}
      />

      <h2 className="text-3xl font-semibold tracking-tight text-foreground">
        {skill.name}
      </h2>
      {data?.topic && (
        <span className="mt-3 inline-flex rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground">
          {data.topic}
        </span>
      )}

      <div className="mt-6 grid grid-cols-1 gap-10 lg:grid-cols-[minmax(0,1fr)_210px]">
        <main className="min-w-0">
          <section className="mb-10">
            <div className="flex items-center justify-between border-b border-border pb-3 font-mono text-xs uppercase text-foreground">
              <span>{t("skills.skillssh.detail.installation")}</span>
              <Button
                type="button"
                variant={installed ? "outline" : "mcp"}
                size="sm"
                onClick={() => void handleInstall()}
                disabled={installed || installing}
              >
                {installing ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : installed ? (
                  <Check className="h-3.5 w-3.5" />
                ) : (
                  <Download className="h-3.5 w-3.5" />
                )}
                {installed ? t("skills.installed") : t("skills.install")}
              </Button>
            </div>
            <div className="mt-4 flex items-center gap-3 rounded-lg border border-border bg-muted/65 px-4 py-3">
              <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-xs text-muted-foreground">
                <span className="mr-2 text-muted-foreground/50">$</span>
                {installCommand}
              </code>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                onClick={() => void handleCopy()}
                className="h-7 w-7 shrink-0"
                aria-label={t("common.copy")}
              >
                <Clipboard className="h-3.5 w-3.5" />
              </Button>
            </div>
          </section>

          {isLoading ? (
            <div className="flex h-64 items-center justify-center text-muted-foreground">
              <Loader2 className="mr-3 h-6 w-6 animate-spin" />
              {t("skills.skillssh.detail.loading")}
            </div>
          ) : isError || !data ? (
            <div className="flex h-64 flex-col items-center justify-center text-center">
              <AlertTriangle className="mb-3 h-9 w-9 text-destructive/70" />
              <p className="text-sm font-medium">
                {t("skills.skillssh.detail.error")}
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
                  className={cn("h-3.5 w-3.5", isFetching && "animate-spin")}
                />
                {t("common.refresh")}
              </Button>
            </div>
          ) : (
            <>
              <section className="mb-10">
                <div className="border-b border-border pb-3 font-mono text-xs uppercase text-foreground">
                  {t("skills.skillssh.detail.summary")}
                </div>
                <div className="mt-4 rounded-lg border border-border bg-muted/55 px-5 py-3">
                  <SafeSkillHtml html={data.summaryHtml} compact />
                </div>
              </section>

              <section>
                <div className="border-b border-border pb-3 font-mono text-xs text-foreground">
                  SKILL.md
                </div>
                <div className="pt-5">
                  <SafeSkillHtml html={data.contentHtml} />
                </div>
              </section>
            </>
          )}
        </main>

        <aside className="space-y-1 lg:pt-1">
          <div className="border-b border-border py-5 first:pt-0">
            <div className="mb-2 font-mono text-xs uppercase text-foreground">
              {t("skills.skillssh.columns.installs")}
            </div>
            <div className="font-mono text-3xl font-semibold tabular-nums">
              {numberFormatter.format(skill.installs)}
            </div>
            <div className="mt-3 h-12 w-44">
              <SkillActivityChart
                values={skill.weeklyInstalls}
                width={176}
                height={48}
              />
            </div>
          </div>

          <div className="border-b border-border py-5">
            <div className="mb-2 flex items-center gap-1.5 font-mono text-xs uppercase text-foreground">
              {t("skills.skillssh.detail.repository")}
              {skill.isOfficial && (
                <BadgeCheck className="h-3.5 w-3.5 text-blue-500" />
              )}
            </div>
            <p className="break-all font-mono text-sm">
              {skill.repoOwner}/{skill.repoName}
            </p>
          </div>

          {data?.githubStars && (
            <div className="border-b border-border py-5">
              <div className="mb-2 font-mono text-xs uppercase text-foreground">
                {t("skills.skillssh.detail.githubStars")}
              </div>
              <div className="flex items-center gap-2 font-mono text-sm">
                <Star className="h-4 w-4" />
                {data.githubStars}
              </div>
            </div>
          )}

          {data?.firstSeen && (
            <div className="border-b border-border py-5">
              <div className="mb-2 font-mono text-xs uppercase text-foreground">
                {t("skills.skillssh.detail.firstSeen")}
              </div>
              <p className="font-mono text-sm">{data.firstSeen}</p>
            </div>
          )}

          {data && data.securityAudits.length > 0 && (
            <div className="py-5">
              <div className="mb-2 font-mono text-xs uppercase text-foreground">
                {t("skills.skillssh.detail.securityAudits")}
              </div>
              <div className="divide-y divide-border">
                {data.securityAudits.map((audit) => (
                  <div
                    key={audit.provider}
                    className="flex items-center justify-between gap-3 py-2.5"
                  >
                    <span className="flex min-w-0 items-center gap-2 truncate text-xs">
                      {audit.status.toLowerCase() === "pass" ? (
                        <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-green-500" />
                      ) : null}
                      {audit.provider}
                    </span>
                    <AuditStatus status={audit.status} />
                  </div>
                ))}
              </div>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
