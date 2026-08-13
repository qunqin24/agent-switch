import type { AppId } from "@/lib/api";
import type { VisibleApps } from "@/types";
import { ProviderIcon } from "@/components/ProviderIcon";
import { cn } from "@/lib/utils";
import { Monitor, Terminal } from "lucide-react";

const APP_BADGE_ICON: Partial<
  Record<AppId, { icon: typeof Terminal; offsetY?: number }>
> = {
  claude: { icon: Terminal },
  "claude-desktop": { icon: Monitor, offsetY: 0.5 },
};

interface AppSwitcherProps {
  activeApp: AppId;
  onSwitch: (app: AppId) => void;
  visibleApps?: VisibleApps;
  compact?: boolean;
  orientation?: "horizontal" | "vertical";
}

const ALL_APPS: AppId[] = [
  "claude",
  "claude-desktop",
  "codex",
  "gemini",
  "opencode",
  "openclaw",
  "hermes",
  "pi",
];
const STORAGE_KEY = "agentswitch-last-app";

export function AppSwitcher({
  activeApp,
  onSwitch,
  visibleApps,
  compact,
  orientation = "horizontal",
}: AppSwitcherProps) {
  const handleSwitch = (app: AppId) => {
    if (app === activeApp) return;
    localStorage.setItem(STORAGE_KEY, app);
    onSwitch(app);
  };
  const iconSize = 20;
  const appIconName: Record<AppId, string> = {
    claude: "claude",
    "claude-desktop": "claude",
    codex: "openai",
    gemini: "gemini",
    opencode: "opencode",
    openclaw: "openclaw",
    hermes: "hermes",
    pi: "pi",
  };
  const appDisplayName: Record<AppId, string> = {
    claude: "Claude Code",
    "claude-desktop": "Claude Desktop",
    codex: "Codex",
    gemini: "Gemini",
    opencode: "OpenCode",
    openclaw: "OpenClaw",
    hermes: "Hermes",
    pi: "Pi",
  };

  // Filter apps based on visibility settings (default all visible)
  const appsToShow = ALL_APPS.filter((app) => {
    if (!visibleApps) return true;
    return visibleApps[app];
  });

  return (
    <div
      className={cn(
        orientation === "vertical"
          ? "flex flex-col w-full gap-1 bg-transparent border-0 p-0"
          : "inline-flex bg-zinc-100/80 dark:bg-zinc-800/80 backdrop-blur-md p-1 gap-1 border border-black/5 dark:border-white/5 rounded-full",
      )}
    >
      {appsToShow.map((app) => {
        const badgeConfig = APP_BADGE_ICON[app];
        const BadgeIcon = badgeConfig?.icon;
        const isActive = activeApp === app;
        return (
          <button
            key={app}
            type="button"
            onClick={() => handleSwitch(app)}
            className={cn(
              "group relative inline-flex items-center px-3 font-medium transition-all duration-300 ease-out",
              orientation === "vertical"
                ? "w-full justify-start h-10 rounded-xl text-[15px]"
                : "h-8 rounded-full text-sm",
              orientation === "vertical"
                ? isActive
                  ? "bg-zinc-900/5 dark:bg-zinc-100/10 text-zinc-900 dark:text-zinc-100 pl-4"
                  : "text-zinc-500 hover:text-zinc-900 hover:bg-zinc-900/5 dark:text-zinc-400 dark:hover:text-zinc-100 dark:hover:bg-zinc-100/10 pl-4"
                : isActive
                  ? "bg-white text-zinc-900 shadow-[0_2px_8px_rgb(0,0,0,0.08)] dark:bg-zinc-700 dark:text-zinc-50 border border-black/5 dark:border-white/5"
                  : "text-zinc-500 hover:text-zinc-900 hover:bg-black/5 dark:text-zinc-400 dark:hover:text-zinc-50 dark:hover:bg-white/5",
            )}
          >
            {orientation === "vertical" && isActive && (
              <span className="absolute left-0 top-3 bottom-3 w-0.5 bg-zinc-900 dark:bg-zinc-100 rounded-full" />
            )}
            <span
              className={cn(
                "relative inline-flex shrink-0 transition-all duration-200",
                orientation === "vertical" &&
                  !isActive &&
                  "opacity-60 grayscale-[20%] group-hover:opacity-100 group-hover:grayscale-0",
              )}
            >
              <ProviderIcon
                icon={appIconName[app]}
                name={appDisplayName[app]}
                size={iconSize}
              />
              {BadgeIcon && (
                <span
                  className={cn(
                    "absolute -bottom-0.5 -right-0.5 flex items-center justify-center rounded-full border h-[11px] w-[11px]",
                    isActive
                      ? "bg-white border-zinc-200 text-zinc-900 dark:bg-zinc-700 dark:border-zinc-600 dark:text-white"
                      : "bg-zinc-100 border-zinc-200 text-zinc-400 group-hover:bg-zinc-200 group-hover:text-zinc-700 dark:bg-zinc-800 dark:border-zinc-700 dark:text-zinc-500",
                  )}
                  aria-hidden="true"
                >
                  <BadgeIcon
                    className="h-[8px] w-[8px]"
                    strokeWidth={2.5}
                    style={
                      badgeConfig?.offsetY
                        ? { transform: `translateY(${badgeConfig.offsetY}px)` }
                        : undefined
                    }
                  />
                </span>
              )}
            </span>
            <span
              className={cn(
                "transition-all duration-200 whitespace-nowrap overflow-hidden text-left",
                orientation === "vertical" ? "flex-1 ml-3" : "",
                orientation === "vertical" && !isActive
                  ? "text-zinc-500 group-hover:text-zinc-900 dark:text-zinc-400 dark:group-hover:text-zinc-100"
                  : "",
                orientation === "vertical" && isActive
                  ? "text-zinc-900 dark:text-zinc-100 font-semibold"
                  : "",
                compact && orientation !== "vertical"
                  ? "max-w-0 opacity-0 ml-0"
                  : orientation !== "vertical"
                    ? "max-w-[120px] opacity-100 ml-2"
                    : "",
              )}
            >
              {appDisplayName[app]}
            </span>
          </button>
        );
      })}
    </div>
  );
}
