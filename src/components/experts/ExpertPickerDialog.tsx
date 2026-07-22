import { useMemo, useState } from "react";
import { Bot, Check, Search, X } from "lucide-react";

import { avatarEmoji } from "../../lib/avatar";
import { Button } from "../ui/Button";
import type { ExpertSummary } from "../../types";

/** 专家选择弹框：支持单选/多选，供项目成员选择和智能体提示词导入复用。 */
export function ExpertPickerDialog({
  agents,
  confirmText = "确定",
  emptyText = "没有匹配的专家。",
  initial,
  onClose,
  onConfirm,
  selectionMode,
  title,
}: {
  agents: ExpertSummary[];
  confirmText?: string;
  emptyText?: string;
  initial: string[];
  onClose: () => void;
  onConfirm: (names: string[]) => void;
  selectionMode: "single" | "multiple";
  title: string;
}) {
  const [sel, setSel] = useState<Set<string>>(() => new Set(initial));
  const [q, setQ] = useState("");

  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return agents;
    return agents.filter(
      (a) =>
        (a.displayName || a.name).toLowerCase().includes(s) ||
        a.name.toLowerCase().includes(s) ||
        (a.profession || "").toLowerCase().includes(s),
    );
  }, [agents, q]);

  function toggle(name: string) {
    setSel((prev) => {
      if (selectionMode === "single") return new Set([name]);
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  return (
    <div className="fixed inset-0 z-[60] grid place-items-center bg-black/30 p-4" onClick={onClose}>
      <div className="flex max-h-[80vh] w-[480px] flex-col overflow-hidden rounded-xl border border-border bg-popover" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-border-subtle px-4 py-3">
          <h3 className="text-base font-semibold text-foreground">{title}</h3>
          <button type="button" onClick={onClose} className="grid h-8 w-8 place-items-center rounded-md text-foreground-muted transition hover:bg-accent">
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <div className="border-b border-border-subtle px-4 py-2">
          <div className="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5">
            <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
            <input className="min-w-0 flex-1 bg-transparent text-sm outline-none" placeholder="搜索专家（名称/职业）" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {filtered.length === 0 ? (
            <p className="py-8 text-center text-xs text-foreground-muted">{emptyText}</p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {filtered.map((a) => {
                const checked = sel.has(a.name);
                return (
                  <li key={a.id}>
                    <button type="button" onClick={() => toggle(a.name)} className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition ${checked ? "bg-primary/5" : "hover:bg-accent"}`}>
                      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-[15px]">
                        {avatarEmoji(a.avatar) ? <span aria-hidden="true">{avatarEmoji(a.avatar)}</span> : <Bot className="h-3.5 w-3.5 text-foreground-muted" aria-hidden="true" />}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] font-medium text-foreground">{a.displayName || a.name}</span>
                        {a.profession && <span className="block truncate text-[11px] text-foreground-muted">{a.profession}</span>}
                      </span>
                      <span className={`grid h-4 w-4 shrink-0 place-items-center rounded-full ${checked ? "bg-primary text-primary-foreground" : "border border-border"}`}>
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
            <Button tone="outline" onClick={onClose}>取消</Button>
            <Button tone="primary" onClick={() => onConfirm([...sel])} disabled={selectionMode === "single" && sel.size === 0}>{confirmText}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
