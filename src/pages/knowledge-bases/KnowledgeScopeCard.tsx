import { BookMarked, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { kbScopedMountedIds } from "../../api";
import { KnowledgeMountPickerDialog } from "./KnowledgeMountPickerDialog";

const SCOPE_HINT: Record<string, string> = {
  agent: "挂到这个智能体的资料库，它的所有对话都能查阅",
  project: "挂到这个项目的资料库，项目内所有会话共享",
  session: "挂到当前对话的资料库",
};

/** 资料库挂载卡片（仿 记忆/技能 卡片）：显示挂载数 + 打开挂载选择抽屉。 */
export function KnowledgeScopeCard({ scopeType, scopeId }: { scopeType: string; scopeId: string }) {
  const [count, setCount] = useState<number | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);

  async function reload() {
    try {
      const ids = await kbScopedMountedIds(scopeType, scopeId);
      setCount(ids.length);
    } catch {
      setCount(null);
    }
  }

  useEffect(() => {
    void reload();
  }, [scopeType, scopeId]);

  return (
    <div className="rounded-xl border border-border-subtle bg-surface p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
          <BookMarked className="h-4 w-4 text-foreground-secondary" aria-hidden="true" />
          资料库
        </h3>
        <button
          type="button"
          onClick={() => setPickerOpen(true)}
          className="flex items-center gap-1 text-[12px] text-primary transition hover:text-foreground"
        >
          <Plus className="h-3.5 w-3.5" aria-hidden="true" /> 挂载资料库
        </button>
      </div>
      <div className="flex items-center gap-3 rounded-lg border border-border-subtle bg-background px-3 py-3">
        <span className="min-w-0 flex-1">
          <span className="block text-lg font-semibold tabular-nums text-foreground">{count == null ? "—" : count}</span>
          <span className="block truncate text-[11px] text-foreground-muted">{SCOPE_HINT[scopeType] ?? ""}</span>
        </span>
      </div>

      {pickerOpen ? (
        <KnowledgeMountPickerDialog
          scopeType={scopeType}
          scopeId={scopeId}
          onClose={() => setPickerOpen(false)}
          onChanged={() => void reload()}
        />
      ) : null}
    </div>
  );
}
