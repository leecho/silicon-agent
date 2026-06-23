import { useState } from "react";
import { CornerDownRight, Trash2 } from "lucide-react";
import type { QueuedTask } from "../../types";
import { Tooltip } from "../ui/Tooltip";
import { BubbleConfirm } from "../ui/BubbleConfirm";

/**
 * T70：会话排队消息（嵌在 Composer 内、输入框上方）。参考 Codex：整块圆角容器包住所有排队消息，
 * 行与行之间加分隔边框；比 composer 略窄、底部不圆角并轻微压住下方输入框。删除走气泡二次确认。
 * 只展示 status="queued" 的项（在飞队头不在此显示）；取消 running 队头走 stop_session（不在此）。
 */
export function QueuedMessages({
  tasks,
  onCancel,
}: {
  tasks: QueuedTask[];
  onCancel: (itemId: string) => void;
}) {
  // 正在二次确认删除的项 id（同一时刻只允许一个气泡）。
  const [confirmId, setConfirmId] = useState<string | null>(null);

  const queued = tasks.filter((t) => t.status === "queued");
  if (queued.length === 0) return null;

  return (
    // 比 composer 略窄（mx-2）；底部不圆角（rounded-t-xl）并 -mb-2.5 压住下方输入框。
    <div className="mx-2 -mb-2.5 rounded-t-xl border border-border bg-background shadow-sm">
      <div className="divide-y divide-border">
        {queued.map((t) => (
          <div
            key={t.itemId}
            className="group relative flex items-center gap-2.5 px-3 py-2 text-[13px] transition first:rounded-t-xl hover:bg-accent/40"
          >
            <CornerDownRight
              className="h-3.5 w-3.5 shrink-0 text-foreground-muted"
              aria-hidden="true"
            />
            {/* Tooltip 代替原生 title：hover 文本显示完整内容 */}
            <Tooltip content={t.payload}>
              <span className="flex-1 truncate text-foreground-secondary">
                {t.payload}
              </span>
            </Tooltip>
            <Tooltip content="取消排队">
              <button
                type="button"
                aria-label="取消排队"
                onClick={() => setConfirmId(t.itemId)}
                className="grid h-6 w-6 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-muted hover:text-destructive"
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            </Tooltip>
            {confirmId === t.itemId && (
              <BubbleConfirm
                title="取消这条排队消息？"
                description={t.payload}
                confirmText="取消排队"
                cancelText="保留"
                tone="danger"
                onConfirm={() => {
                  onCancel(t.itemId);
                  setConfirmId(null);
                }}
                onCancel={() => setConfirmId(null)}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
