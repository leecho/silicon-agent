import { Plus } from "lucide-react";
import type { SessionInfo } from "../../types";
import { Tooltip } from "../ui/Tooltip";

// 会话列表侧边栏：顶部「+ 新会话」，下方可滚动的会话行列表。
// 当前会话高亮；每行点击切换，行尾「删除」按钮 stopPropagation 后单独触发删除。
export function SessionSidebar({
  sessions,
  currentId,
  onSwitch,
  onNew,
  onDelete,
}: {
  sessions: SessionInfo[];
  currentId: string | null;
  onSwitch: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
}) {
  return (
    <aside className="flex h-full min-h-0 flex-col border-r border-border-subtle bg-card">
      <div className="shrink-0 p-2">
        <button
          type="button"
          onClick={onNew}
          className="flex w-full items-center justify-center gap-1 rounded-lg border border-border bg-transparent px-3 py-2 text-sm font-semibold text-foreground-secondary transition hover:bg-accent hover:text-foreground"
        >
          <Plus size={14} className="shrink-0" />
          新会话
        </button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto p-2">
        {sessions.map((s) => {
          const active = s.id === currentId;
          return (
            <div
              key={s.id}
              onClick={() => onSwitch(s.id)}
              className={`group flex cursor-pointer items-center gap-2 rounded-lg border px-2.5 py-2 transition-colors ${
                active
                  ? "border-ring bg-accent"
                  : "border-transparent hover:bg-accent"
              }`}
            >
              <div className="min-w-0 flex-1 leading-tight">
                <Tooltip content={s.title || "未命名会话"}>
                  <div
                    className={`truncate text-sm text-foreground ${
                      active ? "font-semibold" : ""
                    }`}
                  >
                    {s.title || "未命名会话"}
                  </div>
                </Tooltip>
                <div className="truncate text-[11px] text-foreground-muted">
                  {s.updatedAt}
                </div>
              </div>
              <Tooltip content="删除会话">
                <button
                  type="button"
                  aria-label="删除会话"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(s.id);
                  }}
                  className="shrink-0 rounded-md px-1.5 py-0.5 text-[11px] text-foreground-muted opacity-0 transition hover:bg-accent hover:text-destructive group-hover:opacity-100"
                >
                  删除
                </button>
              </Tooltip>
            </div>
          );
        })}
      </div>
    </aside>
  );
}
