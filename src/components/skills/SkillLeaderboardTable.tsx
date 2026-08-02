import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Area, AreaChart } from "recharts";
import { BadgeCheck, Check, Download, Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { SkillsShDiscoverableSkill } from "@/lib/api/skills";

interface SkillLeaderboardTableProps {
  skills: SkillsShDiscoverableSkill[];
  installedDirectories: ReadonlySet<string>;
  onInstall: (key: string) => Promise<void>;
  onSelect: (skill: SkillsShDiscoverableSkill) => void;
}

interface SkillLeaderboardRowProps {
  index: number;
  skill: SkillsShDiscoverableSkill;
  installed: boolean;
  numberFormatter: Intl.NumberFormat;
  onInstall: (key: string) => Promise<void>;
  onSelect: (skill: SkillsShDiscoverableSkill) => void;
}

export function SkillActivityChart({
  values,
  width = 176,
  height = 44,
}: {
  values: number[];
  width?: number;
  height?: number;
}) {
  const { t } = useTranslation();

  if (values.length < 2) {
    return <span className="text-muted-foreground/40">—</span>;
  }

  const data = values.map((installs, index) => ({ index, installs }));

  return (
    <div
      style={{ width, height }}
      role="img"
      aria-label={t("skills.skillssh.activityChart", {
        values: values.join(", "),
      })}
    >
      <AreaChart
        width={width}
        height={height}
        data={data}
        margin={{ top: 6, right: 1, bottom: 3, left: 1 }}
      >
        <Area
          type="monotone"
          dataKey="installs"
          stroke="hsl(var(--muted-foreground))"
          strokeOpacity={0.72}
          strokeWidth={1.6}
          fill="hsl(var(--muted))"
          fillOpacity={0.32}
          isAnimationActive={false}
        />
      </AreaChart>
    </div>
  );
}

function SkillLeaderboardRow({
  index,
  skill,
  installed,
  numberFormatter,
  onInstall,
  onSelect,
}: SkillLeaderboardRowProps) {
  const { t } = useTranslation();
  const [installing, setInstalling] = useState(false);

  const handleInstall = async () => {
    setInstalling(true);
    try {
      await onInstall(skill.key);
    } finally {
      setInstalling(false);
    }
  };

  return (
    <TableRow
      data-testid={`skill-row-${skill.key}`}
      className="group h-[72px] hover:bg-muted/25"
    >
      <TableCell className="w-14 py-0 pl-1 pr-4 font-mono text-sm tabular-nums text-muted-foreground">
        {index + 1}
      </TableCell>
      <TableCell className="min-w-0 py-0 pl-0 pr-6">
        <button
          type="button"
          onClick={() => onSelect(skill)}
          className="flex max-w-full items-baseline gap-3 text-left outline-none focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-blue-500/40"
          aria-label={t("skills.skillssh.openDetails", { name: skill.name })}
        >
          <span className="max-w-[28rem] truncate text-[15px] font-semibold text-foreground group-hover:text-blue-600 dark:group-hover:text-blue-400">
            {skill.name}
          </span>
          <span className="hidden truncate font-mono text-[13px] text-muted-foreground/75 sm:inline">
            {skill.repoOwner}/{skill.repoName}
          </span>
        </button>
      </TableCell>
      <TableCell className="hidden w-56 py-0 pl-0 pr-8 lg:table-cell">
        <SkillActivityChart values={skill.weeklyInstalls} />
      </TableCell>
      <TableCell className="w-48 py-0 pl-0 pr-1">
        <div className="flex items-center justify-end gap-2.5">
          <span className="flex min-w-0 items-center justify-end gap-2 font-mono text-sm tabular-nums text-foreground">
            {skill.isOfficial && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <BadgeCheck
                    className="h-[17px] w-[17px] shrink-0 text-muted-foreground"
                    aria-label={t("skills.skillssh.official")}
                  />
                </TooltipTrigger>
                <TooltipContent>{t("skills.skillssh.official")}</TooltipContent>
              </Tooltip>
            )}
            {numberFormatter.format(skill.installs)}
          </span>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-8 w-8 shrink-0 rounded-md text-muted-foreground opacity-70 hover:text-foreground group-hover:opacity-100"
                onClick={() => void handleInstall()}
                disabled={installed || installing}
                aria-label={
                  installed ? t("skills.installed") : t("skills.install")
                }
              >
                {installing ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : installed ? (
                  <Check className="h-4 w-4" />
                ) : (
                  <Download className="h-4 w-4" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {installed ? t("skills.installed") : t("skills.install")}
            </TooltipContent>
          </Tooltip>
        </div>
      </TableCell>
    </TableRow>
  );
}

export function SkillLeaderboardTable({
  skills,
  installedDirectories,
  onInstall,
  onSelect,
}: SkillLeaderboardTableProps) {
  const { t, i18n } = useTranslation();
  const numberFormatter = new Intl.NumberFormat(
    i18n.resolvedLanguage || i18n.language || "en",
    {
      notation: "compact",
      maximumFractionDigits: 1,
    },
  );

  return (
    <TooltipProvider delayDuration={250}>
      <Table className="table-fixed">
        <TableHeader>
          <TableRow className="h-12 hover:bg-transparent">
            <TableHead className="w-14 pl-1 pr-4 font-mono text-xs">
              #
            </TableHead>
            <TableHead className="pl-0 pr-6 text-xs">
              {t("skills.skillssh.columns.skill")}
            </TableHead>
            <TableHead className="hidden w-56 pl-0 pr-8 text-right text-xs lg:table-cell">
              {t("skills.skillssh.columns.activity")}
            </TableHead>
            <TableHead className="w-48 pl-0 pr-10 text-right text-xs">
              {t("skills.skillssh.columns.installs")}
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {skills.map((skill, index) => (
            <SkillLeaderboardRow
              key={skill.key}
              index={index}
              skill={skill}
              installed={installedDirectories.has(
                skill.directory.toLowerCase(),
              )}
              numberFormatter={numberFormatter}
              onInstall={onInstall}
              onSelect={onSelect}
            />
          ))}
        </TableBody>
      </Table>
    </TooltipProvider>
  );
}
