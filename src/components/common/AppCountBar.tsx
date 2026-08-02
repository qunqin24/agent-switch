import React from "react";

import { APP_ICON_MAP, SKILL_APP_IDS } from "@/config/appConfig";
import type { SkillAppId } from "@/lib/api/types";

interface AppCountBarProps {
  totalLabel: React.ReactNode;
  counts: Partial<Record<SkillAppId, number>>;
  appIds?: SkillAppId[];
}

export const AppCountBar: React.FC<AppCountBarProps> = ({
  totalLabel,
  counts,
  appIds = SKILL_APP_IDS,
}) => (
  <div className="flex flex-shrink-0 items-center justify-between gap-4 py-3">
    <div className="whitespace-nowrap text-sm font-medium text-foreground">
      {totalLabel}
    </div>
    <div className="no-scrollbar flex items-center gap-1 overflow-x-auto">
      {appIds.map((app) => (
        <span
          key={app}
          className="inline-flex h-5 items-center gap-1 rounded-md bg-zinc-100 px-1.5 text-[11px] text-zinc-500 dark:bg-zinc-800/60 dark:text-zinc-400"
        >
          <span className="flex items-center justify-center [&>svg]:size-3">
            {APP_ICON_MAP[app].icon}
          </span>
          <span className="tabular-nums font-medium">{counts[app] ?? 0}</span>
        </span>
      ))}
    </div>
  </div>
);
