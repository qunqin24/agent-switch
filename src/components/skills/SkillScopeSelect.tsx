import { LibraryBig } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { APP_ICON_MAP, SKILL_APP_IDS } from "@/config/appConfig";
import type { SkillAppId } from "@/lib/api/types";

const GLOBAL_SCOPE_VALUE = "global";

export type SkillManagementScope =
  | { kind: "app"; app: SkillAppId }
  | { kind: "global" };

interface SkillScopeSelectProps {
  scope: SkillManagementScope;
  appOptions?: readonly SkillAppId[];
  onScopeChange: (scope: SkillManagementScope) => void;
}

export function SkillScopeSelect({
  scope,
  appOptions = SKILL_APP_IDS,
  onScopeChange,
}: SkillScopeSelectProps) {
  const { t } = useTranslation();
  const value = scope.kind === "global" ? GLOBAL_SCOPE_VALUE : scope.app;
  const label =
    scope.kind === "global" ? t("skills.global.title") : t(`apps.${scope.app}`);

  const handleValueChange = (nextValue: string) => {
    if (nextValue === GLOBAL_SCOPE_VALUE) {
      if (scope.kind !== "global") {
        onScopeChange({ kind: "global" });
      }
      return;
    }

    const selectedApp = appOptions.find((app) => app === nextValue);
    if (selectedApp && (scope.kind !== "app" || selectedApp !== scope.app)) {
      onScopeChange({ kind: "app", app: selectedApp });
    }
  };

  return (
    <Select value={value} onValueChange={handleValueChange}>
      <SelectTrigger
        aria-label={t("skills.switchScope")}
        className="h-7 w-auto min-w-0 gap-1 rounded-md border-0 bg-transparent px-1.5 py-0 text-sm font-medium shadow-none hover:bg-zinc-100 focus:border-transparent dark:hover:bg-zinc-800 [&>svg]:h-3.5 [&>svg]:w-3.5"
      >
        <SelectValue>{label}</SelectValue>
      </SelectTrigger>
      <SelectContent align="start">
        <SelectItem value={GLOBAL_SCOPE_VALUE}>
          <span className="flex items-center gap-2">
            <LibraryBig size={14} />
            <span>{t("skills.global.title")}</span>
          </span>
        </SelectItem>
        <SelectSeparator />
        {appOptions.map((app) => (
          <SelectItem key={app} value={app}>
            <span className="flex items-center gap-2">
              {APP_ICON_MAP[app].icon}
              <span>{t(`apps.${app}`)}</span>
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
