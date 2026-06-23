import { useMemo } from "react";
import { Tooltip } from "../../../../components/ui";
import type { UsageAnalyticsView, UsageRange } from "../../../../types";
import { formatTokens, formatPercent, recentDates } from "./usageFormat";

function StatCard({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "success" | "info" | "warning" | "neutral";
}) {
  const toneClass =
    tone === "success"
      ? "text-success"
      : tone === "warning"
        ? "text-warning"
        : tone === "info"
          ? "text-primary"
          : "text-foreground";
  return (
    <div className="rounded-lg border border-border-subtle bg-surface p-4">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-foreground-muted">
        {label}
      </div>
      <div className={`mt-2 text-xl font-bold ${toneClass}`}>{value}</div>
    </div>
  );
}

export function UsageOverviewPanel({
  data,
  range,
}: {
  data: UsageAnalyticsView;
  range: UsageRange;
}) {
  const { totals } = data;
  const inputTotal = totals.input + totals.cacheRead + totals.cacheCreate;
  const hitRate = inputTotal > 0 ? totals.cacheRead / inputTotal : 0;

  // 组成条各段比例
  const seg = (n: number) => (totals.total > 0 ? (n / totals.total) * 100 : 0);

  // 活动热力图：取最近天数（all/30d→近 91 天，7d→近 28 天对齐范围视觉）
  const heatDays = range === "7d" ? 28 : 91;
  const dates = useMemo(() => recentDates(heatDays), [heatDays]);
  const totalByDate = useMemo(() => {
    const map = new Map<string, number>();
    for (const b of data.byDate) map.set(b.date, b.total);
    return map;
  }, [data.byDate]);
  const maxDay = useMemo(() => {
    let m = 0;
    for (const d of dates) m = Math.max(m, totalByDate.get(d) ?? 0);
    return m;
  }, [dates, totalByDate]);
  const level = (v: number) => {
    if (v <= 0 || maxDay <= 0) return 0;
    const r = v / maxDay;
    if (r > 0.75) return 4;
    if (r > 0.5) return 3;
    if (r > 0.25) return 2;
    return 1;
  };
  const levelClass = [
    "bg-muted",
    "bg-primary/25",
    "bg-primary/45",
    "bg-primary/70",
    "bg-primary",
  ];

  // 快捷统计
  const activeDays = data.byDate.length;
  const peakHour = data.byHour.reduce(
    (best, h) => (h.total > best.total ? h : best),
    data.byHour[0] ?? { hour: 0, total: 0, calls: 0 }
  );
  const favoriteModel = data.byModel[0]?.model ?? "—";

  // 按 7 列（周）分组热力图
  const weeks: string[][] = [];
  for (let i = 0; i < dates.length; i += 7) weeks.push(dates.slice(i, i + 7));

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard label="缓存命中" value={formatTokens(totals.cacheRead)} tone="success" />
        <StatCard label="缓存未命中" value={formatTokens(totals.input + totals.cacheCreate)} tone="info" />
        <StatCard label="输出" value={formatTokens(totals.output)} tone="warning" />
        <StatCard label="总 Token" value={formatTokens(totals.total)} tone="neutral" />
      </div>

      <div className="rounded-lg border border-border-subtle bg-surface p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-semibold text-foreground">Token 构成</span>
          <span className="text-xs text-foreground-muted">{formatTokens(totals.total)} 总量</span>
        </div>
        <div className="flex h-3 w-full overflow-hidden rounded-full bg-muted">
          <div className="bg-success/70" style={{ width: `${seg(totals.cacheRead)}%` }} />
          <div className="bg-primary" style={{ width: `${seg(totals.input + totals.cacheCreate)}%` }} />
          <div className="bg-warning/80" style={{ width: `${seg(totals.output)}%` }} />
        </div>
        <div className="mt-3 flex flex-wrap gap-4 text-xs text-foreground-secondary">
          <Legend color="bg-success/70" label={`缓存命中 ${formatTokens(totals.cacheRead)}`} />
          <Legend color="bg-primary" label={`未命中 ${formatTokens(totals.input + totals.cacheCreate)}`} />
          <Legend color="bg-warning/80" label={`输出 ${formatTokens(totals.output)}`} />
          <span className="ml-auto text-foreground-muted">命中率 {formatPercent(hitRate)}</span>
        </div>
      </div>

      <div className="rounded-lg border border-border-subtle bg-surface p-4">
        <div className="mb-3 text-sm font-semibold text-foreground">活动热力图</div>
        <div className="flex gap-1 overflow-x-auto">
          {weeks.map((week, wi) => (
            <div key={wi} className="flex flex-col gap-1">
              {week.map((d) => {
                const v = totalByDate.get(d) ?? 0;
                return (
                  <Tooltip key={d} content={`${d}：${formatTokens(v)} token`}>
                    <div className={`h-3 w-3 rounded-sm ${levelClass[level(v)]}`} />
                  </Tooltip>
                );
              })}
            </div>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-5">
        <Quick label="会话数" value={String(data.sessions)} />
        <Quick label="消息数" value={String(data.messages)} />
        <Quick label="活跃天" value={String(activeDays)} />
        <Quick label="峰值时段" value={`${String(peakHour.hour).padStart(2, "0")}:00`} />
        <Quick label="常用模型" value={favoriteModel} />
      </div>
    </div>
  );
}

function Legend({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className={`h-2.5 w-2.5 rounded-sm ${color}`} />
      {label}
    </span>
  );
}

function Quick({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-border-subtle bg-surface p-3">
      <div className="text-[11px] text-foreground-muted">{label}</div>
      <Tooltip content={value}>
        <div className="mt-1 truncate text-sm font-semibold text-foreground">
          {value}
        </div>
      </Tooltip>
    </div>
  );
}
