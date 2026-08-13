import {
  Activity,
  BarChart3,
  Check,
  Copy,
  Edit,
  Loader2,
  Minus,
  Play,
  Plus,
  Terminal,
  Trash2,
  Zap,
  MoreHorizontal,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { AppId } from "@/lib/api";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";

interface ProviderActionsProps {
  appId?: AppId;
  isCurrent: boolean;
  isInConfig?: boolean;
  isTesting?: boolean;
  isProxyTakeover?: boolean;
  isOmo?: boolean;
  onSwitch: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onTest?: () => void;
  onConfigureUsage?: () => void;
  onDelete: () => void;
  onRemoveFromConfig?: () => void;
  onDisableOmo?: () => void;
  onOpenTerminal?: () => void;
  isAutoFailoverEnabled?: boolean;
  isInFailoverQueue?: boolean;
  onToggleFailover?: (enabled: boolean) => void;
  isOfficialBlockedByProxy?: boolean;
  // Hermes v12+ providers: dict overlay — edit/delete must go through Web UI
  isReadOnly?: boolean;
  // OpenClaw: default model
  isDefaultModel?: boolean;
  onSetAsDefault?: () => void;
}

// 主按钮的呈现状态。title 用于 disabled 态向用户解释为何不可点击；
// 因 Button 基类带 disabled:pointer-events-none，title 必须挂在外层非禁用
// 的 wrapper 上才会在 hover 时显示（见下方 <span> 包裹）。
interface MainButtonState {
  disabled: boolean;
  variant: "default" | "secondary";
  className: string;
  icon: JSX.Element;
  text: string;
  title?: string;
}

export function ProviderActions({
  appId,
  isCurrent,
  isInConfig = false,
  isTesting,
  isProxyTakeover = false,
  isOmo = false,
  onSwitch,
  onEdit,
  onDuplicate,
  onTest,
  onConfigureUsage,
  onDelete,
  onRemoveFromConfig,
  onDisableOmo,
  onOpenTerminal,
  isAutoFailoverEnabled = false,
  isInFailoverQueue = false,
  onToggleFailover,
  isOfficialBlockedByProxy = false,
  isReadOnly = false,
  // OpenClaw: default model
  isDefaultModel = false,
  onSetAsDefault,
}: ProviderActionsProps) {
  const { t } = useTranslation();

  // 累加模式应用（OpenCode 非 OMO / OpenClaw / Hermes）
  const isAdditiveMode =
    (appId === "opencode" && !isOmo) ||
    appId === "openclaw" ||
    appId === "hermes" ||
    appId === "pi";

  // 故障转移模式下的按钮逻辑（累加模式和 OMO 应用不支持故障转移）
  const isFailoverMode =
    !isAdditiveMode && !isOmo && isAutoFailoverEnabled && onToggleFailover;

  const handleMainButtonClick = () => {
    if (isOmo) {
      if (isCurrent) {
        onDisableOmo?.();
      } else {
        onSwitch();
      }
    } else if (isAdditiveMode) {
      // 累加模式：切换配置状态（添加/移除）
      if (isInConfig) {
        if (onRemoveFromConfig) {
          onRemoveFromConfig();
        } else {
          onDelete();
        }
      } else {
        onSwitch(); // 添加到配置
      }
    } else if (isFailoverMode) {
      onToggleFailover(!isInFailoverQueue);
    } else {
      onSwitch();
    }
  };

  const getMainButtonState = (): MainButtonState => {
    if (isOmo) {
      if (isCurrent) {
        return {
          disabled: false,
          variant: "secondary" as const,
          className:
            "bg-gray-200 text-muted-foreground hover:bg-gray-200 hover:text-muted-foreground dark:bg-gray-700 dark:hover:bg-gray-700",
          icon: <Check className="h-4 w-4" />,
          text: t("provider.inUse"),
        };
      }
      return {
        disabled: false,
        variant: "default" as const,
        className: "",
        icon: <Play className="h-4 w-4" />,
        text: t("provider.enable"),
      };
    }

    // 累加模式（OpenCode 非 OMO / OpenClaw）
    if (isAdditiveMode) {
      if (isInConfig) {
        return {
          disabled: isDefaultModel === true,
          variant: "secondary" as const,
          className: cn(
            "bg-orange-100 text-orange-600 hover:bg-orange-200 dark:bg-orange-900/50 dark:text-orange-400 dark:hover:bg-orange-900/70",
            isDefaultModel && "opacity-40 cursor-not-allowed",
          ),
          icon: <Minus className="h-4 w-4" />,
          text: t("provider.removeFromConfig", { defaultValue: "移除" }),
        };
      }
      return {
        disabled: false,
        variant: "default" as const,
        className:
          "bg-emerald-500 hover:bg-emerald-600 dark:bg-emerald-600 dark:hover:bg-emerald-700",
        icon: <Plus className="h-4 w-4" />,
        text: t("provider.addToConfig", { defaultValue: "添加" }),
      };
    }

    if (isFailoverMode) {
      if (isInFailoverQueue) {
        return {
          disabled: false,
          variant: "secondary" as const,
          className:
            "bg-blue-100 text-blue-600 hover:bg-blue-200 dark:bg-blue-900/50 dark:text-blue-400 dark:hover:bg-blue-900/70",
          icon: <Check className="h-4 w-4" />,
          text: t("failover.inQueue", { defaultValue: "已加入" }),
        };
      }
      return {
        disabled: false,
        variant: "default" as const,
        className:
          "bg-blue-500 hover:bg-blue-600 dark:bg-blue-600 dark:hover:bg-blue-700",
        icon: <Plus className="h-4 w-4" />,
        text: t("failover.addQueue", { defaultValue: "加入" }),
      };
    }

    if (isCurrent) {
      return {
        disabled: true,
        variant: "secondary" as const,
        className:
          "bg-gray-200 text-muted-foreground hover:bg-gray-200 hover:text-muted-foreground dark:bg-gray-700 dark:hover:bg-gray-700",
        icon: <Check className="h-4 w-4" />,
        text: t("provider.inUse"),
      };
    }

    if (isOfficialBlockedByProxy) {
      return {
        disabled: true,
        variant: "default" as const,
        className: "",
        icon: <Play className="h-4 w-4" />,
        text: t("provider.enable"),
        title: t("provider.blockedByProxyHint"),
      };
    }

    return {
      disabled: false,
      variant: "default" as const,
      className: isProxyTakeover
        ? "bg-emerald-500 hover:bg-emerald-600 dark:bg-emerald-600 dark:hover:bg-emerald-700"
        : "",
      icon: <Play className="h-4 w-4" />,
      text: t("provider.enable"),
    };
  };

  const buttonState = getMainButtonState();

  const canDelete =
    !isReadOnly && (isOmo || isAdditiveMode ? true : !isCurrent);

  return (
    <div className="flex items-center gap-1.5">
      {(appId === "openclaw" || appId === "hermes") &&
        isInConfig &&
        onSetAsDefault &&
        (() => {
          const activeLabel =
            appId === "hermes"
              ? t("provider.inUse", { defaultValue: "已在用" })
              : t("provider.isDefault", { defaultValue: "当前默认" });
          const inactiveLabel =
            appId === "hermes"
              ? t("provider.enable", { defaultValue: "启用" })
              : t("provider.setAsDefault", { defaultValue: "设为默认" });
          return (
            <Button
              size="sm"
              variant={isDefaultModel ? "secondary" : "default"}
              onClick={isDefaultModel ? undefined : onSetAsDefault}
              disabled={isDefaultModel}
              className={cn(
                "w-fit px-2.5",
                isDefaultModel
                  ? "bg-gray-200 text-muted-foreground dark:bg-gray-700 opacity-60 cursor-not-allowed"
                  : "bg-blue-500 hover:bg-blue-600 dark:bg-blue-600 dark:hover:bg-blue-700",
              )}
            >
              <Zap className="h-4 w-4" />
              {isDefaultModel ? activeLabel : inactiveLabel}
            </Button>
          );
        })()}

      {/* wrapper span 承接 hover：disabled 按钮自身 pointer-events:none，
          原生 title 与 cursor 都必须挂在未禁用的外层元素上才会生效 */}
      <span
        title={buttonState.title}
        className={cn(
          "inline-flex",
          buttonState.disabled && "cursor-not-allowed",
        )}
      >
        <Button
          size="sm"
          variant={buttonState.variant}
          onClick={handleMainButtonClick}
          disabled={buttonState.disabled}
          className={cn("w-[4.5rem] px-2.5", buttonState.className)}
        >
          {buttonState.icon}
          {buttonState.text}
        </Button>
      </span>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="icon"
            variant="ghost"
            className="h-8 w-8 text-zinc-500 hover:text-zinc-950 dark:hover:text-zinc-50 hover:bg-zinc-900/5 dark:hover:bg-white/5 rounded-lg transition-colors flex-shrink-0"
          >
            {isTesting ? (
              <Loader2 className="h-4 w-4 animate-spin text-zinc-500" />
            ) : (
              <MoreHorizontal className="h-4 w-4" />
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="end"
          className="w-44 bg-white dark:bg-zinc-950 border border-zinc-200/80 dark:border-zinc-800 p-1.5 shadow-lg rounded-xl"
        >
          <DropdownMenuItem
            onClick={isReadOnly ? undefined : onEdit}
            disabled={isReadOnly}
            className="flex items-center gap-2 px-2.5 py-2 text-sm text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 rounded-lg cursor-pointer focus:bg-zinc-50 dark:focus:bg-zinc-900"
          >
            <Edit className="h-4 w-4 text-zinc-400" />
            <span>{t("common.edit")}</span>
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={onDuplicate}
            className="flex items-center gap-2 px-2.5 py-2 text-sm text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 rounded-lg cursor-pointer focus:bg-zinc-50 dark:focus:bg-zinc-900"
          >
            <Copy className="h-4 w-4 text-zinc-400" />
            <span>{t("provider.duplicate")}</span>
          </DropdownMenuItem>

          {onTest && (
            <DropdownMenuItem
              onClick={onTest}
              disabled={isTesting}
              className="flex items-center gap-2 px-2.5 py-2 text-sm text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 rounded-lg cursor-pointer focus:bg-zinc-50 dark:focus:bg-zinc-900"
            >
              {isTesting ? (
                <Loader2 className="h-4 w-4 animate-spin text-zinc-400" />
              ) : (
                <Activity className="h-4 w-4 text-zinc-400" />
              )}
              <span>{t("provider.connectivityCheck", "检测连通")}</span>
            </DropdownMenuItem>
          )}

          {onConfigureUsage && (
            <DropdownMenuItem
              onClick={onConfigureUsage}
              className="flex items-center gap-2 px-2.5 py-2 text-sm text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 rounded-lg cursor-pointer focus:bg-zinc-50 dark:focus:bg-zinc-900"
            >
              <BarChart3 className="h-4 w-4 text-zinc-400" />
              <span>{t("provider.configureUsage")}</span>
            </DropdownMenuItem>
          )}

          {onOpenTerminal && (
            <DropdownMenuItem
              onClick={onOpenTerminal}
              className="flex items-center gap-2 px-2.5 py-2 text-sm text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 rounded-lg cursor-pointer focus:bg-zinc-50 dark:focus:bg-zinc-900"
            >
              <Terminal className="h-4 w-4 text-zinc-400" />
              <span>{t("provider.openTerminal", "打开终端")}</span>
            </DropdownMenuItem>
          )}

          {canDelete && (
            <>
              <DropdownMenuSeparator className="my-1 border-t border-zinc-100 dark:border-zinc-900" />
              <DropdownMenuItem
                onClick={onDelete}
                className="flex items-center gap-2 px-2.5 py-2 text-sm text-red-600 hover:bg-red-50 dark:hover:bg-red-950/30 rounded-lg cursor-pointer focus:bg-red-50 dark:focus:bg-red-950/30"
              >
                <Trash2 className="h-4 w-4 text-red-400" />
                <span>{t("common.delete")}</span>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
