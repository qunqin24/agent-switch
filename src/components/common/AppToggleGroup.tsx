import React from "react";

import { APP_ICON_MAP, SKILL_APP_IDS } from "@/config/appConfig";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { SkillAppId } from "@/lib/api/types";

interface AppToggleGroupProps {
  apps: Partial<Record<SkillAppId, boolean>>;
  onToggle: (app: SkillAppId, enabled: boolean) => void;
  appIds?: SkillAppId[];
  disabled?: boolean;
  disabledApps?: Partial<Record<SkillAppId, boolean>>;
  disabledReasons?: Partial<Record<SkillAppId, string>>;
}

export const AppToggleGroup: React.FC<AppToggleGroupProps> = ({
  apps,
  onToggle,
  appIds = SKILL_APP_IDS,
  disabled = false,
  disabledApps = {},
  disabledReasons = {},
}) => (
  <div className="flex flex-shrink-0 items-center gap-1.5">
    {appIds.map((app) => {
      const { label, icon, activeClass } = APP_ICON_MAP[app];
      const enabled = apps[app] ?? false;
      const appDisabled = disabled || (disabledApps[app] ?? false);
      return (
        <Tooltip key={app}>
          <TooltipTrigger asChild>
            <button
              type="button"
              disabled={appDisabled}
              aria-label={label}
              aria-pressed={enabled}
              onClick={() => onToggle(app, !enabled)}
              className={`flex h-7 w-7 items-center justify-center rounded-lg transition-all disabled:cursor-not-allowed disabled:opacity-40 ${
                enabled ? activeClass : "opacity-40 hover:opacity-80"
              }`}
            >
              {icon}
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            <p>
              {label}
              {enabled ? " ✓" : ""}
              {disabledReasons[app] ? ` · ${disabledReasons[app]}` : ""}
            </p>
          </TooltipContent>
        </Tooltip>
      );
    })}
  </div>
);
