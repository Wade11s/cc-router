import { NavLink } from "react-router";
import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { useSubscriptions } from "@/hooks/useSubscriptions";
import { useVirtualModels } from "@/hooks/useVirtualModels";
import { useProxyStatus } from "@/hooks/useSettings";
import { useUpdater } from "@/hooks/useUpdater";
import { useT } from "@/i18n";
import { runtime, webLogout, webSession } from "@/runtime";
import { LogoMark } from "@/components/sketch/LogoMark";
import { SidebarIcon, type SidebarIconName } from "@/components/sketch/SidebarIcon";

interface NavItem {
  to: string;
  label: string;
  icon: SidebarIconName;
  badge?: string | (() => string | null);
  dot?: boolean;
  /** 点的语义: 默认 err(红, 有更新) / ok(绿, 代理在跑) */
  dotTone?: "err" | "ok";
  /** 无障碍与 hover 提示文案的 i18n key */
  dotLabelKey?: string;
}

export function Sidebar() {
  const { t } = useT();
  const subs = useSubscriptions();
  const proxy = useProxyStatus();
  const { detected } = useUpdater();
  const vms = useVirtualModels();

  const subsCount = subs.data?.length ?? 0;
  const running = proxy.data?.running ?? false;
  const hasUpdate = detected !== null;

  const items: NavItem[] = [
    { to: "/guide", label: t("sidebar.nav.guide"), icon: "guide" },
    {
      to: "/live-routing",
      label: t("sidebar.nav.liveRouting"),
      icon: "live",
      dot: running,
      dotTone: "ok",
      dotLabelKey: "sidebar.proxyRunning",
    },
    {
      to: "/virtual-models",
      label: t("sidebar.nav.virtualModels"),
      icon: "vm",
      badge: String(vms.data?.length ?? 5),
    },
    { to: "/subscriptions", label: t("sidebar.nav.subscriptions"), icon: "subs", badge: subsCount > 0 ? String(subsCount) : undefined },
    { to: "/request-logs", label: t("sidebar.nav.requestLogs"), icon: "logs" },
    { to: "/statistics", label: t("sidebar.nav.statistics"), icon: "stats" },
    { to: "/receipts", label: t("sidebar.nav.receipts"), icon: "receipts" },
    {
      to: "/updates",
      label: t("sidebar.nav.updates"),
      icon: "updates",
      dot: hasUpdate,
      dotLabelKey: "sidebar.updateAvailable",
    },
    { to: "/settings", label: t("sidebar.nav.settings"), icon: "settings" },
    { to: "/about", label: t("sidebar.nav.about"), icon: "about" },
  ];

  const [showLogout, setShowLogout] = useState(false);
  useEffect(() => {
    if (runtime.kind !== "web") return;
    void webSession().then((s) => setShowLogout(s.auth_enabled));
  }, []);

  return (
    <aside className="sidebar">
      <div className="brand">
        <LogoMark size={44} variant="compact" className="brand-mark" />
        <div className="brand-text">
          <div className="brand-name">cc-router</div>
          <div className="brand-tag">{t("sidebar.brand.tag")}</div>
        </div>
      </div>
      {/* 手画波浪分隔线 */}
      <svg className="brand-rule" viewBox="0 0 240 10" preserveAspectRatio="none" aria-hidden="true">
        <path d="M0 5 C 15 1, 45 9, 60 5 C 75 1, 105 9, 120 5 C 135 1, 165 9, 180 5 C 195 1, 225 9, 240 5" />
      </svg>
      {/* 代理地址/端口与版本号不在这里展示: 地址在「实时路由」页可复制,
       * 版本号在「关于」/「检查更新」页 —— 侧边栏只留导航。 */}
      {items.map((it) => {
        const badge = typeof it.badge === "function" ? it.badge() : it.badge;
        return (
          <NavLink
            key={it.to}
            to={it.to}
            className={({ isActive }) => cn("nav-item", isActive && "active")}
          >
            <span className="nav-icon">
              <SidebarIcon name={it.icon} size={28} />
            </span>
            <span className="nav-label">{it.label}</span>
            {badge && <span className="badge mono">{badge}</span>}
            {!badge && it.dot && (
              <NavDot
                tone={it.dotTone ?? "err"}
                label={t(it.dotLabelKey ?? "sidebar.updateAvailable")}
              />
            )}
          </NavLink>
        );
      })}
      {showLogout && (
        <button
          type="button"
          className="nav-item nav-logout"
          onClick={() => {
            void webLogout().then(() => window.location.reload());
          }}
        >
          <span className="nav-icon">
            <SidebarIcon name="logout" size={28} />
          </span>
          <span className="nav-label">{t("sidebar.logout")}</span>
        </button>
      )}
    </aside>
  );
}

/** 侧栏状态点: ok = 绿色手画圆点带放射短线 (代理在跑), err = 陶土色星号 (有可用更新) */
function NavDot({ tone, label }: { tone: "ok" | "err"; label: string }) {
  if (tone === "ok") {
    return (
      <svg className="nav-dot ok" viewBox="0 0 18 18" width="18" height="18" role="img" aria-label={label}>
        <title>{label}</title>
        <path className="nav-dot-fill" d="M8 5 C 10.3 4.9, 11.7 6.5, 11.6 8.6 C 11.5 10.7, 9.9 12.1, 7.9 12 C 5.8 11.9, 4.4 10.3, 4.5 8.3 C 4.6 6.4, 6.1 5.1, 8 5 Z" />
        <path className="nav-dot-rays" d="M13.4 4.2 L15 2.6 M14.4 8.4 L16.6 8.3 M13.4 12.6 L15 14.2" />
      </svg>
    );
  }
  return (
    <svg className="nav-dot err" viewBox="0 0 16 16" width="15" height="15" role="img" aria-label={label}>
      <title>{label}</title>
      <path d="M8 1.8 L8 14.2 M2.6 4.6 L13.4 11.4 M2.6 11.4 L13.4 4.6" />
    </svg>
  );
}
