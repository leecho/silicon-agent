import { Tooltip } from "../../../components/ui";

// 上下文窗口占用显示器。数据来自后端 get_session_context_usage：
// used/max 为该会话最近一次主体调用的真实用量（provider 统计）。
// 首条消息发出前无用量记录，percent=0（语义即「尚未占用上下文」）。
export function ContextMeter({
  percent = 0,
  usedTokens,
  maxTokens,
}: {
  percent?: number;
  usedTokens?: number;
  maxTokens?: number;
}) {
  const clamped = Math.max(0, Math.min(100, Math.round(percent)));
  const filled = Math.round((clamped / 100) * 5);
  const title =
    usedTokens != null && maxTokens != null
      ? `上下文窗口占用 ${clamped}%（已用 ${usedTokens.toLocaleString()} / 上限 ${maxTokens.toLocaleString()} tokens）`
      : `上下文窗口占用 ${clamped}%`;
  return (
    <Tooltip content={title}>
      <div className="flex items-center gap-1.5 text-xs text-foreground-muted">
      <div className="flex items-center gap-0.5">
        {Array.from({ length: 5 }).map((_, i) => (
          <span
            key={i}
            className={`h-2.5 w-1.5 rounded-sm ${i < filled ? "bg-foreground-muted" : "bg-border"}`}
          />
        ))}
      </div>
      <span>{clamped}%</span>
      </div>
    </Tooltip>
  );
}
