import { useMemo, useState } from "react";
import { Search, Users, X } from "lucide-react";

import { avatarEmoji } from "../../lib/avatar";
import type { Team } from "../../types";

/** 团队选择弹框：搜索筛选 + 单选；点选某团队即回传其 id（用于播种项目成员/章程）。 */
export function TeamPickerDialog({ teams, onClose, onPick }: {
  teams: Team[];
  onClose: () => void;
  onPick: (teamId: string) => void;
}) {
  const [q, setQ] = useState("");
  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    if (!s) return teams;
    return teams.filter(
      (t) =>
        (t.displayName || t.name).toLowerCase().includes(s) ||
        t.name.toLowerCase().includes(s) ||
        (t.description || "").toLowerCase().includes(s),
    );
  }, [teams, q]);

  return (
    <div className="fixed inset-0 z-[60] grid place-items-center bg-black/30 p-4" onClick={onClose}>
      <div className="flex max-h-[80vh] w-[480px] flex-col overflow-hidden rounded-xl border border-border bg-popover" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between border-b border-border-subtle px-4 py-3">
          <h3 className="text-base font-semibold text-foreground">从团队中选择</h3>
          <button type="button" onClick={onClose} className="grid h-8 w-8 place-items-center rounded-md text-foreground-muted transition hover:bg-accent"><X className="h-4 w-4" aria-hidden="true" /></button>
        </div>
        <div className="border-b border-border-subtle px-4 py-2">
          <div className="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-1.5">
            <Search className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
            <input className="min-w-0 flex-1 bg-transparent text-sm outline-none" placeholder="搜索团队（名称/描述）" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {filtered.length === 0 ? (
            <p className="py-8 text-center text-xs text-foreground-muted">没有匹配的团队。</p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {filtered.map((t) => (
                <li key={t.id}>
                  <button type="button" onClick={() => onPick(t.id)} className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition hover:bg-accent">
                    <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-[15px]">
                      {avatarEmoji(t.avatar) ? <span aria-hidden="true">{avatarEmoji(t.avatar)}</span> : <Users className="h-3.5 w-3.5 text-foreground-muted" aria-hidden="true" />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] font-medium text-foreground">{t.displayName || t.name}</span>
                      <span className="block truncate text-[11px] text-foreground-muted">{t.memberCount} 成员{t.description ? ` · ${t.description}` : ""}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
