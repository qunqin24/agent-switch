import {
  Activity,
  AlertTriangle,
  BookOpen,
  Box,
  Github,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import { SkillBreadcrumbs } from "./SkillBreadcrumbs";
import { Button } from "@/components/ui/button";
import { useSkillsShPublisher } from "@/hooks/useSkills";

interface SkillPublisherPageProps {
  owner: string;
  onHome: () => void;
  onSelectRepository: (repository: string) => void;
}

export function SkillPublisherPage({
  owner,
  onHome,
  onSelectRepository,
}: SkillPublisherPageProps) {
  const { t } = useTranslation();
  const { data, isLoading, isError, isFetching, refetch } =
    useSkillsShPublisher(owner);

  return (
    <div className="mx-auto w-full max-w-6xl py-5">
      <SkillBreadcrumbs
        items={[
          { key: "skills", label: "skills", onClick: onHome },
          { key: owner, label: owner },
        ]}
      />

      <h2 className="text-3xl font-semibold tracking-tight text-foreground sm:text-4xl">
        {owner}
      </h2>

      {isLoading ? (
        <div className="flex h-64 items-center justify-center text-muted-foreground">
          <Loader2 className="mr-3 h-6 w-6 animate-spin" />
          {t("skills.skillssh.publisher.loading")}
        </div>
      ) : isError || !data ? (
        <div className="flex h-64 flex-col items-center justify-center text-center">
          <AlertTriangle className="mb-3 h-9 w-9 text-destructive/70" />
          <p className="text-sm font-medium">
            {t("skills.skillssh.publisher.error")}
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
              <BookOpen className="h-4 w-4" />
              {t("skills.skillssh.publisher.sources", {
                count: data.sourceCount,
              })}
            </span>
            <span className="flex items-center gap-1.5 whitespace-nowrap">
              <Box className="h-4 w-4" />
              {t("skills.skillssh.publisher.skills", {
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

          <section className="mt-16">
            <div className="grid grid-cols-[minmax(0,1fr)_9rem] border-b border-border py-3 font-mono text-xs uppercase text-muted-foreground">
              <span>{t("skills.skillssh.publisher.source")}</span>
              <span className="text-right">
                {t("skills.skillssh.columns.installs")}
              </span>
            </div>
            <div className="divide-y divide-border">
              {data.sources.map((source) => (
                <button
                  key={source.name}
                  type="button"
                  onClick={() => onSelectRepository(source.name)}
                  className="group grid w-full grid-cols-[minmax(0,1fr)_9rem] items-start gap-4 py-3 text-left outline-none transition-colors hover:bg-muted/25 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blue-500/40"
                >
                  <span className="min-w-0">
                    <span className="block truncate font-semibold text-foreground group-hover:text-blue-600 dark:group-hover:text-blue-400">
                      {source.name}
                    </span>
                    <span className="mt-0.5 block truncate font-mono text-sm text-muted-foreground">
                      {source.skillSummary}
                    </span>
                  </span>
                  <span className="pt-0.5 text-right font-mono text-sm text-foreground">
                    {source.installs}
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
