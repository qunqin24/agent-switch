import { useState } from "react";
import { Loader2, Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Switch } from "@/components/ui/switch";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { cn } from "@/lib/utils";

interface ClaudeDesktopRouteToggleProps {
  className?: string;
}

export function ClaudeDesktopRouteToggle({
  className,
}: ClaudeDesktopRouteToggleProps) {
  const { t } = useTranslation();
  const [showStopConfirm, setShowStopConfirm] = useState(false);
  const {
    isRunning,
    status,
    isTakeoverActive,
    startProxyServer,
    stopProxyServer,
    stopWithRestore,
    isStarting,
    isStoppingServer,
    isStopping,
  } = useProxyStatus();

  const isBusy = isStarting || isStoppingServer || isStopping;
  const routeAddress = status?.address ?? "127.0.0.1";
  const routePort = status?.port ?? 15721;

  const handleToggle = async (checked: boolean) => {
    try {
      if (checked) {
        await startProxyServer();
        return;
      }

      if (isTakeoverActive) {
        setShowStopConfirm(true);
        return;
      }

      await stopProxyServer();
    } catch (error) {
      console.error("[ClaudeDesktopRouteToggle] Toggle route failed:", error);
    }
  };

  const handleConfirmStop = async () => {
    setShowStopConfirm(false);
    try {
      await stopWithRestore();
    } catch (error) {
      console.error("[ClaudeDesktopRouteToggle] Stop route failed:", error);
    }
  };

  const tooltipText = isRunning
    ? t("claudeDesktop.route.tooltip.active", {
        address: routeAddress,
        port: routePort,
        defaultValue: `Claude Desktop 本地路由已开启 - ${routeAddress}:${routePort}`,
      })
    : t("claudeDesktop.route.tooltip.inactive", {
        address: routeAddress,
        port: routePort,
        defaultValue: `开启 Claude Desktop 本地路由，用于需要模型映射或格式转换的供应商。当前配置地址：${routeAddress}:${routePort}`,
      });

  return (
    <>
      <div
        className={cn(
          "flex items-center gap-1 px-1.5 h-8 rounded-lg bg-muted/50 transition-all",
          className,
        )}
        title={tooltipText}
      >
        {isBusy ? (
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        ) : (
          <Radio
            className={cn(
              "h-4 w-4 transition-colors",
              isRunning
                ? "text-emerald-500 animate-pulse"
                : "text-muted-foreground",
            )}
          />
        )}
        <Switch
          checked={isRunning}
          onCheckedChange={handleToggle}
          disabled={isBusy || showStopConfirm}
        />
      </div>
      <ConfirmDialog
        isOpen={showStopConfirm}
        title={t("claudeDesktop.route.stopConfirmTitle", {
          defaultValue: "确认停止本地路由？",
        })}
        message={t("claudeDesktop.route.stopConfirmMessage", {
          defaultValue:
            "其它应用仍在使用代理接管。继续停止会同时恢复这些应用的接管配置，并关闭本地路由。\n\n确认仍要停止吗？",
        })}
        confirmText={t("claudeDesktop.route.stopConfirmConfirm", {
          defaultValue: "仍然停止",
        })}
        cancelText={t("claudeDesktop.route.stopConfirmCancel", {
          defaultValue: "保持开启",
        })}
        onConfirm={() => void handleConfirmStop()}
        onCancel={() => setShowStopConfirm(false)}
      />
    </>
  );
}
