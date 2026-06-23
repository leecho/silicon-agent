import { useMemo, useState } from "react";
import { Tooltip } from "../../../../components/ui";
import type { UsageAnalyticsView, UsageModelRow } from "../../../../types";
import { formatTokens, formatPercent, pickColor } from "./usageFormat";

type SortKey = "total" | "calls" | "output" | "cacheRead";

export function UsageModelsPanel({ data }: { data: UsageAnalyticsView }) {
  const [sortKey, setSortKey] = useState<SortKey>("total");

  const grandTotal = data.totals.total || 1;

  const rows = useMemo(() => {
    const copy = [...data.byModel];
    copy.sort((a, b) => b[sortKey] - a[sortKey]);
    return copy;
  }, [data.byModel, sortKey]);

  // 堆叠柱：按日期聚合各模型 total
  const dates = useMemo(
    () => Array.from(new Set(data.byDateModel.map((d) => d.date))).sort(),
    [data.byDateModel]
  );
  const topModels = useMemo(() => rows.slice(0, 5).map((r) => r.model), [rows]);
  const stackByDate = useMemo(() => {
    const map = new Map<string, Map<string, number>>();
    for (const d of data.byDateModel) {
      if (!map.has(d.date)) map.set(d.date, new Map());
      map.get(d.date)!.set(d.model, d.total);
    }
    return map;
  }, [data.byDateModel]);
  const maxDayTotal = useMemo(() => {
    let m = 0;
    for (const d of dates) {
      let sum = 0;
      stackByDate.get(d)?.forEach((v) => (sum += v));
      m = Math.max(m, sum);
    }
    return m || 1;
  }, [dates, stackByDate]);

  return (
    <div className="flex flex-col gap-5">
      <div className="rounded-lg border border-border-subtle bg-surface p-4">
        <div className="mb-3 text-sm font-semibold text-foreground">每日用量（按模型）</div>
        <div className="flex h-40 items-end gap-1 overflow-x-auto">
          {dates.map((d) => {
            const models = stackByDate.get(d);
            return (
              <Tooltip key={d} content={d}>
                <div
                  className="flex w-3 flex-col-reverse"
                  style={{ height: "100%" }}
                >
                {topModels.map((m) => {
                  const v = models?.get(m) ?? 0;
                  const h = (v / maxDayTotal) * 100;
                  if (h <= 0) return null;
                  return (
                    <Tooltip key={m} content={`${d} · ${m}：${formatTokens(v)}`}>
                      <div
                        style={{ height: `${h}%`, backgroundColor: pickColor(m) }}
                      />
                    </Tooltip>
                  );
                })}
                </div>
              </Tooltip>
            );
          })}
        </div>
        <div className="mt-3 flex flex-wrap gap-3 text-xs text-foreground-secondary">
          {topModels.map((m) => (
            <span key={m} className="flex items-center gap-1.5">
              <span className="h-2.5 w-2.5 rounded-sm" style={{ backgroundColor: pickColor(m) }} />
              {m}
            </span>
          ))}
        </div>
      </div>

      <div className="overflow-x-auto rounded-lg border border-border-subtle bg-surface">
        <table className="w-full text-left text-sm">
          <thead className="border-b border-border-subtle text-xs text-foreground-muted">
            <tr>
              <th className="px-3 py-2 font-medium">模型 / 厂家</th>
              <SortableTh label="调用" k="calls" sortKey={sortKey} setSortKey={setSortKey} />
              <SortableTh label="缓存命中" k="cacheRead" sortKey={sortKey} setSortKey={setSortKey} />
              <SortableTh label="输出" k="output" sortKey={sortKey} setSortKey={setSortKey} />
              <SortableTh label="总量" k="total" sortKey={sortKey} setSortKey={setSortKey} />
              <th className="px-3 py-2 font-medium">命中率</th>
              <th className="px-3 py-2 font-medium">占比</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <ModelRow key={`${r.provider}/${r.model}`} row={r} grandTotal={grandTotal} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function SortableTh({
  label,
  k,
  sortKey,
  setSortKey,
}: {
  label: string;
  k: SortKey;
  sortKey: SortKey;
  setSortKey: (k: SortKey) => void;
}) {
  return (
    <th className="px-3 py-2 font-medium">
      <button
        type="button"
        onClick={() => setSortKey(k)}
        className={sortKey === k ? "text-foreground" : "text-foreground-muted hover:text-foreground-secondary"}
      >
        {label}
        {sortKey === k ? " ↓" : ""}
      </button>
    </th>
  );
}

function ModelRow({ row, grandTotal }: { row: UsageModelRow; grandTotal: number }) {
  const inputTotal = row.input + row.cacheRead + row.cacheCreate;
  const hitRate = inputTotal > 0 ? row.cacheRead / inputTotal : 0;
  const share = row.total / grandTotal;
  return (
    <tr className="border-b border-border-subtle last:border-0">
      <td className="px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="h-2.5 w-2.5 shrink-0 rounded-sm" style={{ backgroundColor: pickColor(row.model) }} />
          <div className="min-w-0">
            <div className="truncate font-medium text-foreground">{row.model}</div>
            <div className="truncate text-xs text-foreground-muted">{row.provider}</div>
          </div>
        </div>
      </td>
      <td className="px-3 py-2 tabular-nums text-foreground-secondary">{row.calls}</td>
      <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(row.cacheRead)}</td>
      <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatTokens(row.output)}</td>
      <td className="px-3 py-2 font-semibold tabular-nums text-foreground">{formatTokens(row.total)}</td>
      <td className="px-3 py-2 tabular-nums text-foreground-secondary">{formatPercent(hitRate)}</td>
      <td className="px-3 py-2">
        <div className="flex items-center gap-2">
          <div className="h-1.5 w-16 overflow-hidden rounded-full bg-muted">
            <div className="h-full bg-primary" style={{ width: `${share * 100}%` }} />
          </div>
          <span className="text-xs text-foreground-muted">{formatPercent(share)}</span>
        </div>
      </td>
    </tr>
  );
}
