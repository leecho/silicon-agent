import { useMemo } from "react";
import { Tooltip } from "../../../../components/ui";
import type { UsageAnalyticsView } from "../../../../types";
import { formatTokens, formatPercent, formatTs, sessionLabel, pickColor } from "./usageFormat";

export function UsageCachePanel({ data }: { data: UsageAnalyticsView }) {
  const { totals } = data;
  const inputTotal = totals.input + totals.cacheRead + totals.cacheCreate;
  const hitRate = inputTotal > 0 ? totals.cacheRead / inputTotal : 0;

  const cacheTotal = totals.cacheRead + totals.cacheCreate || 1;

  // 缓存按模型 / 按会话拆分（取缓存 token 最高前 8）
  const byModel = useMemo(
    () =>
      [...data.byModel]
        .map((m) => ({ key: m.model, sub: m.provider, cache: m.cacheRead + m.cacheCreate }))
        .filter((m) => m.cache > 0)
        .sort((a, b) => b.cache - a.cache)
        .slice(0, 8),
    [data.byModel]
  );
  const bySession = useMemo(
    () =>
      [...data.bySession]
        .map((s) => ({
          key: sessionLabel(s.sessionId, s.title),
          sub: "",
          cache: s.cacheRead + s.cacheCreate,
        }))
        .filter((s) => s.cache > 0)
        .sort((a, b) => b.cache - a.cache)
        .slice(0, 8),
    [data.bySession]
  );

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="缓存命中" value={formatTokens(totals.cacheRead)} />
        <Stat label="缓存写入" value={formatTokens(totals.cacheCreate)} />
        <Stat label="未命中输入" value={formatTokens(totals.input)} />
        <Stat label="命中率" value={formatPercent(hitRate)} />
      </div>

      <div className="rounded-lg border border-border-subtle bg-surface p-4">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm font-semibold text-foreground">缓存构成</span>
          <span className="text-xs text-foreground-muted">命中率 {formatPercent(hitRate)}</span>
        </div>
        <div className="flex h-3 w-full overflow-hidden rounded-full bg-muted">
          <div className="bg-success/70" style={{ width: `${(totals.cacheRead / cacheTotal) * 100}%` }} />
          <div className="bg-foreground-secondary" style={{ width: `${(totals.cacheCreate / cacheTotal) * 100}%` }} />
        </div>
        <div className="mt-3 flex flex-wrap gap-4 text-xs text-foreground-secondary">
          <span className="flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-sm bg-success/70" />
            命中 {formatTokens(totals.cacheRead)}
          </span>
          <span className="flex items-center gap-1.5">
            <span className="h-2.5 w-2.5 rounded-sm bg-foreground-secondary" />
            写入 {formatTokens(totals.cacheCreate)}
          </span>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <SplitCard title="按模型" rows={byModel} />
        <SplitCard title="按会话" rows={bySession} />
      </div>

      <div className="overflow-x-auto rounded-lg border border-border-subtle bg-surface">
        <div className="px-4 pt-4 text-sm font-semibold text-foreground">最近缓存调用</div>
        <table className="mt-2 w-full text-left text-sm">
          <thead className="border-b border-border-subtle text-xs text-foreground-muted">
            <tr>
              <th className="px-3 py-2 font-medium">时间</th>
              <th className="px-3 py-2 font-medium">模型</th>
              <th className="px-3 py-2 font-medium">命中</th>
              <th className="px-3 py-2 font-medium">写入</th>
              <th className="px-3 py-2 font-medium">输出</th>
              <th className="px-3 py-2 font-medium">总量</th>
            </tr>
          </thead>
          <tbody>
            {data.recentCacheCalls.length === 0 && (
              <tr>
                <td colSpan={6} className="px-3 py-4 text-center text-xs text-foreground-muted">
                  暂无缓存调用
                </td>
              </tr>
            )}
            {data.recentCacheCalls.map((c, i) => (
              <tr key={`${c.ts}-${c.model}-${i}`} className="border-b border-border-subtle last:border-0">
                <td className="px-3 py-2 text-foreground-secondary">{formatTs(c.ts)}</td>
                <td className="px-3 py-2 text-foreground-secondary">{c.model}</td>
                <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(c.cacheRead)}</td>
                <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(c.cacheCreate)}</td>
                <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(c.output)}</td>
                <td className="px-3 py-2 font-semibold tabular-nums text-foreground">{formatTokens(c.total)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-surface p-4">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-foreground-muted">{label}</div>
      <div className="mt-2 text-xl font-bold text-foreground">{value}</div>
    </div>
  );
}

function SplitCard({
  title,
  rows,
}: {
  title: string;
  rows: { key: string; sub: string; cache: number }[];
}) {
  const max = rows.reduce((m, r) => Math.max(m, r.cache), 0) || 1;
  return (
    <div className="rounded-lg border border-border-subtle bg-surface p-4">
      <div className="mb-3 text-sm font-semibold text-foreground">{title}</div>
      <div className="flex flex-col gap-2.5">
        {rows.length === 0 && <p className="text-xs text-foreground-muted">暂无数据</p>}
        {rows.map((r) => (
          <div key={r.key} className="flex items-center gap-2">
            <span className="h-2.5 w-2.5 shrink-0 rounded-sm" style={{ backgroundColor: pickColor(r.key) }} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center justify-between gap-2">
                <Tooltip content={r.key}>
                  <span className="truncate text-sm text-foreground">
                    {r.key}
                  </span>
                </Tooltip>
                <span className="shrink-0 text-xs tabular-nums text-foreground-muted">
                  {formatTokens(r.cache)}
                </span>
              </div>
              <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-muted">
                <div className="h-full bg-primary" style={{ width: `${(r.cache / max) * 100}%` }} />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
