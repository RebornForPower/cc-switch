import { Loader2, Radio } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { Switch } from "@/components/ui/switch";
import { useProxyStatus } from "@/hooks/useProxyStatus";
import { cn } from "@/lib/utils";
import { useAutoFailoverEnabled } from "@/lib/query/failover";
import { getAppLabel } from "@/config/appConfig";

interface ClaudeDesktopRouteToggleProps {
  className?: string;
  target?: "claude" | "codex";
}

const PROXY_APP_IDS = ["claude", "codex", "gemini", "grokbuild"] as const;

export function ClaudeDesktopRouteToggle({
  className,
  target = "claude",
}: ClaudeDesktopRouteToggleProps) {
  const { t } = useTranslation();
  const {
    isRunning,
    status,
    takeoverStatus,
    startProxyServer,
    stopProxyServer,
    isStarting,
    isStoppingServer,
  } = useProxyStatus();

  const isBusy = isStarting || isStoppingServer;
  // Claude Desktop and Codex Desktop share this gateway. Always observe the
  // independent Codex Desktop failover flag so either route toggle protects
  // the gateway while Desktop still depends on it.
  const { data: codexDesktopFailoverEnabled = false } =
    useAutoFailoverEnabled("codex-desktop");

  // Check which other apps have active takeover (excluding the current target)
  const otherAppTakeoverActive = PROXY_APP_IDS.some(
    (appId) => takeoverStatus?.[appId]
  );
  const codexDesktopFailoverActive = codexDesktopFailoverEnabled;

  const routeAddress = status?.address ?? "127.0.0.1";
  const routePort = status?.port ?? 15721;

  const handleToggle = async (checked: boolean) => {
    try {
      if (checked) {
        await startProxyServer();
        return;
      }

      if (codexDesktopFailoverActive) {
        toast.warning(
          t("claudeDesktop.route.stopBlockedByFailover", {
            defaultValue:
              "请先关闭 Codex Desktop 故障转移开关，再停止本地路由。",
          }),
          { duration: 5000 },
        );
        return;
      }

      if (otherAppTakeoverActive) {
        const activeApps = PROXY_APP_IDS.filter(
          (appId) => takeoverStatus?.[appId]
        );
        const appNames = activeApps.map((id) => getAppLabel(id)).join("、");
        toast.warning(
          t("claudeDesktop.route.stopBlockedByTakeoverDetail", {
            app: appNames,
            defaultValue: `${appNames} 正在使用代理接管，请先在设置中关闭后再停止本地路由。`,
          }),
          { duration: 5000 },
        );
        return;
      }

      await stopProxyServer();
    } catch (error) {
      console.error("[DesktopRouteToggle] Toggle route failed:", error);
    }
  };

  const routeKey =
    target === "codex" ? "codexDesktop.route" : "claudeDesktop.route";
  const desktopName = target === "codex" ? "Codex Desktop" : "Claude Desktop";
  const tooltipText = isRunning
    ? t(`${routeKey}.tooltip.active`, {
        address: routeAddress,
        port: routePort,
        defaultValue: `${desktopName} 本地路由已开启 - ${routeAddress}:${routePort}`,
      })
    : t(`${routeKey}.tooltip.inactive`, {
        address: routeAddress,
        port: routePort,
        defaultValue: `开启 ${desktopName} 本地路由，用于需要格式转换或动态认证的供应商。当前配置地址：${routeAddress}:${routePort}`,
      });

  return (
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
              ? "text-emerald-500 status-heartbeat"
              : "text-muted-foreground",
          )}
        />
      )}
      <Switch
        checked={isRunning}
        onCheckedChange={handleToggle}
        disabled={isBusy}
      />
    </div>
  );
}

