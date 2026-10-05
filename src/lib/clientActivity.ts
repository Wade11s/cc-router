/**
 * Live Routing「客户端接入」的被动流量检测: 把后端按 client_tool 聚合的结果,
 * 归并成 4 张便签 (Claude / Codex / OpenCode / Others) 的活跃状态 + 卡片行。
 *
 * 判定是「最近有没有真的发过请求」(零配置), 不做在线/离线判定 —— cc-router
 * 只能看到流量, 看不到客户端是否打开。日志受 `log_retention_days` 清理,
 * 保留期即天然检测窗口。
 */
import { CLIENT_TOOLS } from "@/lib/clientTools";
import type { ClientActivityDto, ClientToolId } from "@/types";

/** 与 RouteFlowDiagram 四张客户端便签一一对应 */
export type ClientGroupKey = "claude" | "codex" | "opencode" | "others";

/** 前三张便签各自覆盖的已知 client tool */
const NOTE_GROUPS: Array<{ key: Exclude<ClientGroupKey, "others">; toolIds: ClientToolId[] }> = [
  // Claude 桌面端与 Claude Code 同属 Anthropic 系, 与 codex-cli/codex-desktop 的分法对齐
  { key: "claude", toolIds: ["claude-code", "claude-desktop"] },
  { key: "codex", toolIds: ["codex-cli", "codex-desktop"] },
  { key: "opencode", toolIds: ["opencode"] },
];

/** 已知 id → 便签分组; 不在表里的 id (理论上不可能, 三处同步锁住) 归 "others" */
const NOTE_GROUP_BY_ID = new Map<string, ClientGroupKey>(
  NOTE_GROUPS.flatMap((g) => g.toolIds.map((id) => [id, g.key] as const)),
);
const KNOWN_IDS = new Set<string>(CLIENT_TOOLS.map((t) => t.id));

export interface ClientGroupActivity {
  /** 保留期内该组有过请求 (未识别的请求只计 "others") */
  active: boolean;
  /** 组内最近一次请求 (ms epoch); 从未有过为 null */
  lastSeen: number | null;
  /** 组内请求总数 */
  count: number;
}

export interface ClientActivityRow {
  /** 已知 client tool; undefined = 未识别行 */
  toolId?: ClientToolId;
  connected: boolean;
  /** 未接入的已知客户端为 0 */
  count: number;
  lastSeen: number | null;
}

export interface ClientActivitySummary {
  /** 保留期内有任何请求。false = 卡片空态文案 */
  hasAny: boolean;
  groups: Record<ClientGroupKey, ClientGroupActivity>;
  /** 已接入 (最近在前) → 未接入 (CLIENT_TOOLS 顺序) → 未识别行 (如有) */
  rows: ClientActivityRow[];
}

const emptyGroup = (): ClientGroupActivity => ({ active: false, lastSeen: null, count: 0 });

/**
 * 汇总后端聚合结果。`data === undefined` (加载中/失败) 返回 null ——
 * 调用方据此保持便签原样, 避免加载瞬间全部变灰的闪烁。
 */
export function summarizeActivity(
  data: ClientActivityDto[] | undefined,
): ClientActivitySummary | null {
  if (!data) return null;

  const groups: Record<ClientGroupKey, ClientGroupActivity> = {
    claude: emptyGroup(),
    codex: emptyGroup(),
    opencode: emptyGroup(),
    others: emptyGroup(),
  };
  const connected: ClientActivityRow[] = [];
  let unknownRow: ClientActivityRow | undefined;

  for (const r of data) {
    const id = r.client_tool;
    const touch = (key: ClientGroupKey) => {
      const g = groups[key];
      g.active = true;
      g.count += r.request_count;
      g.lastSeen = g.lastSeen === null ? r.last_seen : Math.max(g.lastSeen, r.last_seen);
    };
    if (!id) {
      // 未识别桶 (UA 认不出 / 迁移 009 前的老日志)
      touch("others");
      unknownRow = { connected: true, count: r.request_count, lastSeen: r.last_seen };
      continue;
    }
    // 不在 CLIENT_TOOLS 的 id 理论不可能 (三处同步锁住); 真出现时计入 others 活跃, 不建行
    touch(NOTE_GROUP_BY_ID.get(id) ?? "others");
    if (KNOWN_IDS.has(id)) {
      connected.push({
        toolId: id as ClientToolId,
        connected: true,
        count: r.request_count,
        lastSeen: r.last_seen,
      });
    }
  }

  connected.sort((a, b) => (b.lastSeen ?? 0) - (a.lastSeen ?? 0));
  const connectedIds = new Set(connected.map((r) => r.toolId));
  const notConnected: ClientActivityRow[] = CLIENT_TOOLS.filter(
    (t) => !connectedIds.has(t.id),
  ).map((t) => ({ toolId: t.id, connected: false, count: 0, lastSeen: null }));

  return {
    hasAny: data.length > 0,
    groups,
    rows: unknownRow ? [...connected, ...notConnected, unknownRow] : [...connected, ...notConnected],
  };
}
