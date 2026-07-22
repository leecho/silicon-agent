import { useEffect, useState } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { getAgentUsage } from "../../api";
import type { ScopedUsageView, UsageRange } from "../../types";
import { formatTokens, sessionLabel } from "../settings/sections/usage/usageFormat";

const USAGE_RANGES: { id: UsageRange; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "30d", label: "30天" },
  { id: "7d", label: "7天" },
];

export function AgentUsage({ agentId, onOpenSession }: { agentId: string; onOpenSession: (id: string) => void }) {
  const [range, setRange] = useState<UsageRange>("all");
  const [data, setData] = useState<ScopedUsageView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    getAgentUsage(agentId, range)
      .then((value) => { if (!cancelled) setData(value); })
      .catch((err) => { if (!cancelled) setError(String(err)); });
    return () => { cancelled = true; };
  }, [agentId, range]);

  return (
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto max-w-[860px]">
        <div className="mb-3 flex items-center justify-end gap-1">
          {USAGE_RANGES.map((item) => (
            <button key={item.id} type="button" onClick={() => setRange(item.id)} className={`rounded-lg px-3 py-1.5 text-sm transition ${range === item.id ? "bg-accent font-semibold text-foreground" : "text-foreground-secondary hover:bg-accent hover:text-foreground"}`}>
              {item.label}
            </button>
          ))}
        </div>
        {error && (
          <p className="flex items-center gap-1.5 text-sm text-destructive">
            <AlertTriangle className="h-4 w-4" aria-hidden="true" />
            加载失败：{error}
          </p>
        )}
        {!data && !error && (
          <p className="flex items-center gap-1.5 text-sm text-foreground-muted">
            <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
            正在加载用量
          </p>
        )}
        {data && (
          <>
            <div className="mb-4 grid grid-cols-2 gap-2 text-center sm:grid-cols-4">
              {([
                ["总量", formatTokens(data.totals.total)],
                ["输入", formatTokens(data.totals.input)],
                ["输出", formatTokens(data.totals.output)],
                ["调用", String(data.totals.calls)],
              ] as const).map(([label, value]) => (
                <div key={label} className="rounded-lg border border-border-subtle bg-surface py-3">
                  <div className="text-lg font-semibold tabular-nums text-foreground">{value}</div>
                  <div className="text-[11px] text-foreground-muted">{label}</div>
                </div>
              ))}
            </div>
            {data.bySession.length === 0 ? (
              <p className="rounded-xl border border-dashed border-border py-12 text-center text-xs text-foreground-muted">该范围内暂无用量。</p>
            ) : (
              <ul className="flex flex-col gap-1.5">
                {data.bySession.map((session) => (
                  <li key={session.sessionId}>
                    <button type="button" onClick={() => onOpenSession(session.sessionId)} className="flex w-full items-center justify-between gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2.5 text-left transition hover:border-border">
                      <span className="min-w-0 truncate text-foreground">{sessionLabel(session.sessionId, session.title)}</span>
                      <span className="shrink-0 tabular-nums text-foreground-secondary">{formatTokens(session.total)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </div>
    </div>
  );
}
