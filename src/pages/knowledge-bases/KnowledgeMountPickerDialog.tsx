import { BookMarked, Check, Search, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { kbList, kbMountScope, kbScopedMountedIds, kbUnmountScope } from "../../api";
import type { KnowledgeBase } from "../../types";
import { Button } from "../../components/ui/Button";
import { useNotifications } from "../../components/ui/NotificationProvider";

const SCOPE_DESC: Record<string, string> = {
  agent: "挂到这个智能体，它的所有对话都能查阅这些资料。",
  project: "挂到这个项目，项目内所有会话与成员都能查阅这些资料。",
  session: "只在当前对话里查阅这些资料。",
};

/** 资料库挂载弹框：搜索 + 多选；选完点确定一次性应用（仿「选择成员」弹框）。 */
export function KnowledgeMountPickerDialog({
  scopeType,
  scopeId,
  onClose,
  onChanged,
}: {
  scopeType: string;
  scopeId: string;
  onClose: () => void;
  onChanged?: () => void;
}) {
  const notifications = useNotifications();
  const [all, setAll] = useState<KnowledgeBase[]>([]);
  const [initial, setInitial] = useState<Set<string>>(new Set());
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [q, setQ] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const [list, ids] = await Promise.all([kbList(), kbScopedMountedIds(scopeType, scopeId)]);
        setAll(list);
        setInitial(new Set(ids));
        setSel(new Set(ids));
      } catch (err) {
        notifications.error({ message: String(err) });
      } finally {
        setLoading(false);
      }
    })();
  }, [scopeType, scopeId, notifications]);

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return all;
    return all.filter(
      (kb) => kb.name.toLowerCase().includes(s) || (kb.description || "").toLowerCase().includes(s),
    );
  }, [all, q]);

  function toggle(id: string) {
    setSel((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function confirm() {
    setSaving(true);
    try {
      const toMount = [...sel].filter((id) => !initial.has(id));
      const toUnmount = [...initial].filter((id) => !sel.has(id));
      await Promise.all([
        ...toMount.map((id) => kbMountScope(id, scopeType, scopeId)),
        ...toUnmount.map((id) => kbUnmountScope(id, scopeType, scopeId)),
      ]);
      onChanged?.();
      onClose();
    } catch (err) {
      setSaving(false);
      notifications.error({ title: "保存失败", message: String(err) });
    }
  }

  return (
    <div className="fixed inset-0 z-[60] grid place-items-center bg-black/30 p-4" onClick={onClose}>
      <div
        className="flex max-h-[80vh] w-[480px] flex-col overflow-hidden rounded-xl border border-border bg-popover"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-border-subtle px-4 py-3">
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold text-foreground">挂载资料库</h3>
            <button
              type="button"
              onClick={onClose}
              className="grid h-8 w-8 place-items-center rounded-md text-foreground-muted transition hover:bg-accent"
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
          <p className="mt-0.5 text-xs text-foreground-muted">{SCOPE_DESC[scopeType] ?? ""}</p>
        </div>

        <div className="border-b border-border-subtle px-4 py-2">
          <div className="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5">
            <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
            <input
              className="min-w-0 flex-1 bg-transparent text-sm outline-none"
              placeholder="搜索资料库"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              autoFocus
            />
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-2">
          {loading ? (
            <div className="flex flex-col gap-1 p-1">
              {[0, 1, 2].map((i) => (
                <div key={i} className="h-11 animate-pulse rounded-md bg-surface" />
              ))}
            </div>
          ) : filtered.length === 0 ? (
            <p className="py-8 text-center text-xs text-foreground-muted">
              {all.length === 0 ? "还没有资料库，先到「资料库」页创建。" : "没有匹配的资料库。"}
            </p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {filtered.map((kb) => {
                const checked = sel.has(kb.id);
                return (
                  <li key={kb.id}>
                    <button
                      type="button"
                      onClick={() => toggle(kb.id)}
                      className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition ${
                        checked ? "bg-primary/5" : "hover:bg-accent"
                      }`}
                    >
                      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-foreground-muted">
                        <BookMarked className="h-3.5 w-3.5" aria-hidden="true" />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] font-medium text-foreground">{kb.name}</span>
                        <span className="block truncate text-[11px] text-foreground-muted">{kb.docCount} 份资料</span>
                      </span>
                      <span
                        className={`grid h-4 w-4 shrink-0 place-items-center rounded-full ${
                          checked ? "bg-primary text-primary-foreground" : "border border-border"
                        }`}
                      >
                        {checked && <Check className="h-3 w-3" aria-hidden="true" />}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className="flex items-center justify-between border-t border-border-subtle px-4 py-3">
          <span className="text-[12px] text-foreground-muted">已选 {sel.size}</span>
          <div className="flex gap-2">
            <Button tone="outline" onClick={onClose} disabled={saving}>
              取消
            </Button>
            <Button tone="primary" onClick={() => void confirm()} disabled={saving || loading}>
              {saving ? "保存中…" : "确定"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
