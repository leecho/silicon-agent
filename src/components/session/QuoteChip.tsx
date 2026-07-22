import { Quote, X } from "lucide-react";
import { Tooltip } from "../ui/Tooltip";

// 划词「添加到对话」累积出的引用片段，合并为一个 chip：图标 + 计数 + 清空；悬浮展开全部片段。
// 省略 onClear 即只读模式（已发送的用户消息里用）：不显示清空按钮。
export function QuoteChip({
  fragments,
  onClear,
}: {
  fragments: string[];
  onClear?: () => void;
}) {
  if (fragments.length === 0) return null;
  return (
    <div
      className={`group relative flex shrink-0 items-center gap-2 rounded-lg border border-border-subtle bg-card py-1 pl-3 ${
        onClear ? "pr-5" : "pr-3"
      }`}
    >
      <Tooltip
        content={
          <div className="flex max-w-[320px] flex-col gap-1.5 text-left">
            {fragments.map((f, i) => (
              <p
                key={i}
                className="text-xs leading-5 text-popover-foreground [overflow-wrap:anywhere] [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:3] overflow-hidden"
              >
                "{f}"
              </p>
            ))}
          </div>
        }
      >
        <div className="flex cursor-default items-center gap-2">
          <Quote className="h-4 w-4 shrink-0 text-foreground-secondary" aria-hidden="true" />
          <span className="text-xs leading-5 text-foreground">
            {fragments.length} 个已选文本片段
          </span>
        </div>
      </Tooltip>
      {onClear && (
        <Tooltip content="清空引用">
          <button
            type="button"
            aria-label="清空引用"
            onClick={onClear}
            className="absolute right-1 top-1 grid h-5 w-5 place-items-center rounded-full bg-muted text-foreground-muted opacity-0 transition hover:text-foreground group-hover:opacity-100"
          >
            <X className="h-3 w-3" aria-hidden="true" />
          </button>
        </Tooltip>
      )}
    </div>
  );
}
