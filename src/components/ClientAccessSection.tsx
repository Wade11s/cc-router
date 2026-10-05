/**
 * Live Routing 的「客户端接入」卡: 哪些客户端真的走过 cc-router (被动流量检测)。
 *
 * 判定来自请求日志按 client_tool 的聚合 (get_client_activity) —— 零配置, 不需要
 * 客户端配合; 代价是只能看到「保留期内发过请求」, 看不到此刻是否在线。日志受
 * log_retention_days 清理, 保留期即天然检测窗口 (卡脚注说明)。上面的路由图便签
 * 与这里共用同一份数据 (useClientActivity), 亮/灰口径一致。
 */
import { ClientToolBadge } from "@/components/ClientToolBadge";
import { useClientActivity } from "@/hooks/useClientActivity";
import { useSettings } from "@/hooks/useSettings";
import { fmtNum, fmtRelativeTime } from "@/lib/format";
import { summarizeActivity } from "@/lib/clientActivity";
import { useT } from "@/i18n";

/**
 * @param variant "sketch" = 手绘卡的 card + ca-* 类; "classic" = 经典主题的
 *   lrc-flush-section + lrc-ca-* 类。两版 DOM 结构一致, 只有类名与容器不同, 逻辑零复制。
 */
export function ClientAccessSection({ variant }: { variant: "sketch" | "classic" }) {
  const { t } = useT();
  const activity = useClientActivity();
  const settings = useSettings();
  const summary = summarizeActivity(activity.data);

  const sketch = variant === "sketch";
  const p = sketch ? "ca" : "lrc-ca";

  // 0 = 永久保留 (设置页把 >=36500 天映射成 0 存)
  const days = settings.data?.log_retention_days ?? 0;
  const forever = days >= 36500;

  return (
    <section className={sketch ? "card alt" : "lrc-flush-section"}>
      <div className={sketch ? "card-head" : "lrc-flush-title"}>
        <span className={sketch ? "card-title" : undefined}>
          {t("liveRouting.clientAccess.title")}
        </span>
        <span className={sketch ? "card-sub" : `${p}-sub`}>
          {t("liveRouting.clientAccess.subtitle")}
        </span>
      </div>

      <div className={sketch ? "card-body" : undefined}>
        {summary && !summary.hasAny ? (
          <div className={`${p}-empty`}>{t("liveRouting.clientAccess.empty")}</div>
        ) : (
          <div className={`${p}-list`}>
            <div className={`${p}-row head`}>
              <span>{t("liveRouting.clientAccess.col.client")}</span>
              <span>{t("liveRouting.clientAccess.col.status")}</span>
              <span>{t("liveRouting.clientAccess.col.lastSeen")}</span>
              <span className={`${p}-count-head`}>{t("liveRouting.clientAccess.col.count")}</span>
            </div>
            {summary?.rows.map((r) => (
              <div className={r.connected ? `${p}-row` : `${p}-row off`} key={r.toolId ?? "unknown"}>
                <span className={`${p}-client`}>
                  <ClientToolBadge toolId={r.toolId} />
                </span>
                <span className={r.connected ? `${p}-dot on` : `${p}-dot`}>
                  {t(
                    r.connected
                      ? "liveRouting.clientAccess.connected"
                      : "liveRouting.clientAccess.notConnected",
                  )}
                </span>
                <span className={`${p}-time`}>
                  {r.lastSeen === null ? "—" : fmtRelativeTime(r.lastSeen, t)}
                </span>
                <span className={`${p}-count`}>{fmtNum(r.count)}</span>
              </div>
            ))}
          </div>
        )}

        <div className={`${p}-foot`}>
          {t(
            forever
              ? "liveRouting.clientAccess.retentionForever"
              : "liveRouting.clientAccess.retention",
            { days },
          )}
        </div>
      </div>
    </section>
  );
}
