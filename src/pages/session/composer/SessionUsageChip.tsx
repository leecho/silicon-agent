import { Tooltip } from "../../../components/ui";
import { formatTokens } from "../../settings/sections/usage/usageFormat";
import type { UsageTotals } from "../../../types";

// 会话累计用量 chip：展示该会话自创建以来的 token 总消耗（≠ 上下文窗口占用）。
// 无用量时不渲染（保持 composer footer 简洁）。
export function SessionUsageChip({ usage }: { usage?: UsageTotals | null }) {
  if (!usage || usage.calls === 0) return null;
  const title =
    `累计用量 ${usage.total.toLocaleString()} tokens` +
    `（输入 ${usage.input.toLocaleString()} · 输出 ${usage.output.toLocaleString()} · ` +
    `缓存读 ${usage.cacheRead.toLocaleString()} · 缓存写 ${usage.cacheCreate.toLocaleString()} · ` +
    `${usage.calls} 次调用）`;
  return (
    <Tooltip content={title}>
      <div className="flex items-center gap-1 text-xs text-foreground-muted">
        <span>累计 {formatTokens(usage.total)}</span>
      </div>
    </Tooltip>
  );
}
