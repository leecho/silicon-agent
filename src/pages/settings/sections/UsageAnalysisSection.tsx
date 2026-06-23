import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { getUsageAnalytics } from "../../../api";
import type { UsageAnalyticsView, UsageRange } from "../../../types";
import { joinClasses } from "../../../components/ui/utils";
import { UsageOverviewPanel } from "./usage/UsageOverviewPanel";
import { UsageModelsPanel } from "./usage/UsageModelsPanel";
import { UsageCachePanel } from "./usage/UsageCachePanel";

type UsageTab = "overview" | "models" | "cache";

const TABS: { id: UsageTab; label: string }[] = [
  { id: "overview", label: "概览" },
  { id: "models", label: "模型" },
  { id: "cache", label: "缓存" },
];

const RANGES: { id: UsageRange; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "30d", label: "30天" },
  { id: "7d", label: "7天" },
];

function Pill({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={joinClasses(
        "rounded-lg px-3 py-1.5 text-sm transition",
        active
          ? "bg-accent font-semibold text-foreground"
          : "text-foreground-secondary hover:bg-accent hover:text-foreground"
      )}
    >
      {children}
    </button>
  );
}

export function UsageAnalysisSection() {
  const [tab, setTab] = useState<UsageTab>("overview");
  const [range, setRange] = useState<UsageRange>("all");
  const [data, setData] = useState<UsageAnalyticsView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    getUsageAnalytics(range)
      .then((view) => {
        if (!cancelled) setData(view);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [range]);

  const empty = !!data && data.totals.calls === 0;

  return (
    <section className="flex flex-col gap-5" aria-label="用量分析">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-1">
          {TABS.map((t) => (
            <Pill key={t.id} active={tab === t.id} onClick={() => setTab(t.id)}>
              {t.label}
            </Pill>
          ))}
        </div>
        <div className="flex gap-1">
          {RANGES.map((r) => (
            <Pill key={r.id} active={range === r.id} onClick={() => setRange(r.id)}>
              {r.label}
            </Pill>
          ))}
        </div>
      </div>

      {error && <p className="text-sm text-destructive">加载失败：{error}</p>}
      {loading && !data && <p className="text-sm text-foreground-muted">加载中…</p>}

      {empty && (
        <div className="rounded-2xl border border-border-subtle bg-surface p-8 text-center">
          <p className="text-sm font-medium text-foreground-secondary">暂无用量数据</p>
          <p className="mt-1 text-xs text-foreground-muted">
            用量自本功能启用后产生的运行开始采集；先发起几次对话再回来查看。
          </p>
        </div>
      )}

      {data && !empty && (
        <>
          {tab === "overview" && <UsageOverviewPanel data={data} range={range} />}
          {tab === "models" && <UsageModelsPanel data={data} />}
          {tab === "cache" && <UsageCachePanel data={data} />}
        </>
      )}
    </section>
  );
}
