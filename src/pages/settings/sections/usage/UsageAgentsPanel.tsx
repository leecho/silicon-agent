import { useMemo } from "react";
import type { UsageAnalyticsView } from "../../../../types";
import { formatTokens, formatPercent } from "./usageFormat";

/** 设置页「智能体」维度排行表（按最近活动角色智能体递归归属）。 */
export function UsageAgentsPanel({ data }: { data: UsageAnalyticsView }) {
  const grandTotal = data.totals.total || 1;
  const rows = useMemo(() => [...data.byAgent].sort((a, b) => b.total - a.total), [data.byAgent]);

  if (rows.length === 0) {
    return <p className="rounded-lg border border-border-subtle bg-surface p-8 text-center text-sm text-foreground-muted">暂无智能体维度用量（仅统计有活动角色智能体的会话）。</p>;
  }

  return (
    <div className="overflow-x-auto rounded-lg border border-border-subtle bg-surface">
      <table className="w-full text-left text-sm">
        <thead className="border-b border-border-subtle text-xs text-foreground-muted">
          <tr>
            <th className="px-3 py-2 font-medium">智能体</th>
            <th className="px-3 py-2 font-medium">调用</th>
            <th className="px-3 py-2 font-medium">输出</th>
            <th className="px-3 py-2 font-medium">总量</th>
            <th className="px-3 py-2 font-medium">占比</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.agentId} className="border-b border-border-subtle last:border-0">
              <td className="px-3 py-2 font-medium text-foreground">{r.name || r.agentId}</td>
              <td className="px-3 py-2 tabular-nums text-foreground-secondary">{r.calls}</td>
              <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(r.output)}</td>
              <td className="px-3 py-2 font-semibold tabular-nums text-foreground">{formatTokens(r.total)}</td>
              <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatPercent(r.total / grandTotal)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
