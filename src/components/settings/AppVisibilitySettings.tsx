import { useTranslation } from "react-i18next";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Switch } from "@/components/ui/switch";
import type { SettingsFormState } from "@/hooks/useSettings";
import type { VisibleApps } from "@/types";
import type { AppId } from "@/lib/api";

interface AppVisibilitySettingsProps {
  settings: SettingsFormState;
  onChange: (updates: Partial<SettingsFormState>) => void;
}

const APP_CONFIG: Array<{
  id: AppId;
  icon: string;
  nameKey: string;
}> = [
  { id: "claude", icon: "claude", nameKey: "apps.claudeCode" },
  {
    id: "claude-desktop",
    icon: "claude",
    nameKey: "apps.claudeDesktop",
  },
  { id: "codex", icon: "openai", nameKey: "apps.codex" },
  { id: "gemini", icon: "gemini", nameKey: "apps.gemini" },
  { id: "opencode", icon: "opencode", nameKey: "apps.opencode" },
  { id: "openclaw", icon: "openclaw", nameKey: "apps.openclaw" },
  { id: "hermes", icon: "hermes", nameKey: "apps.hermes" },
  { id: "pi", icon: "pi", nameKey: "apps.pi" },
];

export function AppVisibilitySettings({
  settings,
  onChange,
}: AppVisibilitySettingsProps) {
  const { t } = useTranslation();

  const visibleApps: VisibleApps = settings.visibleApps ?? {
    claude: true,
    "claude-desktop": true,
    codex: true,
    gemini: true,
    opencode: true,
    openclaw: true,
    hermes: true,
    pi: true,
  };

  const visibleCount = Object.values(visibleApps).filter(Boolean).length;

  const handleToggle = (appId: AppId, checked: boolean) => {
    if (!checked && visibleCount <= 1) return;

    onChange({
      visibleApps: {
        ...visibleApps,
        [appId]: checked,
      },
    });
  };

  return (
    <>
      {APP_CONFIG.map((app) => {
        const isVisible = visibleApps[app.id];
        const isDisabled = isVisible && visibleCount <= 1;
        const label = t(app.nameKey);

        return (
          <div
            key={app.id}
            className="flex items-center justify-between gap-4 px-4 py-2.5"
          >
            <div className="flex min-w-0 items-center gap-2.5">
              <ProviderIcon
                icon={app.icon}
                name={label}
                size={16}
                className="flex-shrink-0"
              />
              <span className="truncate text-[13px] font-medium text-foreground">
                {label}
              </span>
            </div>
            <Switch
              checked={isVisible}
              disabled={isDisabled}
              onCheckedChange={(checked) => handleToggle(app.id, checked)}
              aria-label={label}
            />
          </div>
        );
      })}
    </>
  );
}
